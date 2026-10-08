// Where the Monitor's block executions actually go.
//
//     node tools/probe-blocks.mjs dist/desktop-rv32-linux.sb3
//     node tools/probe-blocks.mjs dist/desktop-rv32-linux.sb3 --profile
//     node tools/probe-blocks.mjs dist/desktop-rv32-linux.sb3 --profile --steps 3
//
// Two answers, because there are two questions.
//
// The first is *static*: the project's own blocks, read out of the `.sb3`, with
// the size of every procedure the Monitor sprite owns. A procedure's size is
// what it costs to call once, and the Monitor's whole job is one procedure per
// row plus one call per run, so this is the shape of the display.
//
// The second is *dynamic*, and it is the one that matters: how many block
// executions one whole-panel pass really costs. `monitor_runs` says how many
// strokes were drawn and the pen-line count says how many the pen recorded, but
// neither says what a stroke or a row *cost in blocks*, and a static count
// cannot: `monitor_row_direct` is two hundred and thirty blocks, and its inner
// loop runs two hundred and thirty-nine times inside each of three hundred and
// sixty row calls.
//
// So `--profile` boots the project in the VM that compiles blocks to JavaScript
// -- which is what makes a Linux boot checkable at all -- and then turns the
// compiler off for a few frames and counts every primitive the interpreter
// runs, attributed to the sprite that ran it. The picture on the card is
// whatever the guest had drawn at that moment, so the run count is the real
// one; `--steps` says how many runtime steps the count covers.
//
// Blocks are the *interpreter's* unit. In compiled mode there are no block
// executions at all, which is exactly why the compiled machine is fast; the
// count below is what the same picture would cost in blocks.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import zlib from 'node:zlib';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');

/// The `project.json` out of a `.sb3`. A zip is a central directory of entries
/// each pointing at a local header, and this walks the central directory rather
/// than reading the file front to back: the order entries were written in is
/// the writer's business and the directory is the index.
///
/// raven-asm writes every entry *stored* -- the `.sb3` is a container and
/// compressing a project Scratch is about to parse anyway buys nothing -- so
/// the compression method is read rather than assumed: a reader that always
/// inflates reads a stored entry as a deflate stream and fails with "invalid
/// distance too far back".
function projectJson(file) {
    const buf = fs.readFileSync(file);
    let eocd = buf.length - 22;
    while (eocd >= 0 && buf.readUInt32LE(eocd) !== 0x06054b50) eocd--;
    if (eocd < 0) throw new Error('not a zip');
    const count = buf.readUInt16LE(eocd + 10);
    let at = buf.readUInt32LE(eocd + 16);
    for (let i = 0; i < count; i++) {
        const nameLen = buf.readUInt16LE(at + 28);
        const extraLen = buf.readUInt16LE(at + 30);
        const commentLen = buf.readUInt16LE(at + 32);
        const name = buf.toString('latin1', at + 46, at + 46 + nameLen);
        if (name === 'project.json') {
            const method = buf.readUInt16LE(at + 10);
            const local = buf.readUInt32LE(at + 42);
            const lName = buf.readUInt16LE(local + 26);
            const lExtra = buf.readUInt16LE(local + 28);
            const size = buf.readUInt32LE(at + 20);
            const start = local + 30 + lName + lExtra;
            const raw = buf.subarray(start, start + size);
            const json = method === 0 ? raw.toString('utf8')
                : zlib.inflateRawSync(raw).toString('utf8');
            return JSON.parse(json);
        }
        at += 46 + nameLen + extraLen + commentLen;
    }
    throw new Error('no project.json in the zip');
}

/// Every block reachable from `id` through `next` and through the substacks of
/// its inputs, which is what "runs" means for a block.
function bodySize(blocks, id, seen) {
    let n = 0;
    while (id) {
        const b = blocks[id];
        if (!b || seen.has(id)) break;
        seen.add(id);
        n += 1;
        for (const input of Object.values(b.inputs || {})) {
            if (!Array.isArray(input)) continue;
            for (const v of input.slice(1)) {
                if (v && typeof v === 'object' && typeof v.block === 'string') {
                    n += bodySize(blocks, v.block, seen);
                } else if (typeof v === 'string' && blocks[v]) {
                    n += bodySize(blocks, v, seen);
                }
            }
        }
        id = b.next;
    }
    return n;
}

const args = process.argv.slice(2);
const file = args.find((a) => a.endsWith('.sb3') || a.endsWith('.json')) ||
    path.join('dist', 'desktop-rv32-linux.sb3');
const profile = args.includes('--profile');
const stepsArg = args.indexOf('--steps');
const profileSteps = stepsArg >= 0 ? Number(args[stepsArg + 1]) : 3;
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 600) * 1000;
const jsonArg = args.indexOf('--json');

const project = file.endsWith('.json') ? JSON.parse(fs.readFileSync(file, 'utf8')) : projectJson(file);

// ---------------------------------------------------------------------------
// Static: what the Monitor sprite is made of
// ---------------------------------------------------------------------------

function staticCensus() {
    const monitor = project.targets.find((t) => t.name === 'Monitor') || project.targets[0];
    const blocks = monitor.blocks;

    // The proccode of every definition, so a call can be named rather than
    // counted.
    const definition = new Map();
    for (const [id, b] of Object.entries(blocks)) {
        if (b.opcode === 'procedures_definition') {
            const proto = b.inputs && b.inputs.custom_block &&
                blocks[b.inputs.custom_block[1]];
            const code = proto && proto.mutation ? proto.mutation.proccode : '(unnamed)';
            definition.set(id, code);
        }
    }
    const report = [];
    for (const [id, code] of definition) {
        const body = id && blocks[id] ? blocks[id].next : null;
        const size = body ? bodySize(blocks, body, new Set()) : 0;
        report.push({ code, size });
    }
    report.sort((a, b) => b.size - a.size);

    // A call costs the call block, the reporters that pass its parameters, and
    // the reporters in the callee that read them back.
    let callOverhead = 0;
    for (const b of Object.values(blocks)) {
        if (b.opcode === 'procedures_call') callOverhead += 1;
        if (b.opcode === 'argument_reporter_string_number') callOverhead += 1;
    }

    console.log(`file      ${file}`);
    console.log(`target    ${monitor.name}, ${Object.keys(blocks).length} blocks`);
    for (const r of report) console.log(`  ${String(r.size).padStart(5)}  ${r.code}`);
    console.log(`argument reporter blocks in the whole target: ${callOverhead}`);
}

// ---------------------------------------------------------------------------
// Dynamic: how many blocks one whole-panel pass costs
// ---------------------------------------------------------------------------

const STUBS = {
    '@scratch/scratch-svg-renderer': () => ({
        sanitizeSvg: { sanitizeByteStream: (data) => data },
        loadSvgString: () => Promise.resolve(),
        serializeSvgToString: () => ''
    })
};
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
    if (Object.prototype.hasOwnProperty.call(STUBS, request)) return STUBS[request]();
    return originalLoad.call(this, request, parent, isMain);
};
globalThis.document = { hidden: true };

function findVm() {
    const candidates = [
        process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
        path.join(repo, 'ref', 'turbowarp-vm'),
        path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
    ].filter(Boolean);
    return candidates.find((dir) => fs.existsSync(path.join(dir, 'src', 'virtual-machine.js')));
}

/// A renderer that does nothing at all: this probe is about the blocks the
/// Monitor runs, not about the picture, and a pen that actually rasterised
/// would put its own cost into the measurement.
function nullRenderer() {
    let nextId = 1;
    const drawables = new Map();
    return {
        setLayerGrouping() {}, setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; }, createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; }, createPenSkin() { return nextId++; },
        destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {}, updateTextSkin() {},
        getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [480, 360]; },
        createDrawable() { const id = nextId++; drawables.set(id, { position: [0, 0] }); return id; },
        destroyDrawable(id) { drawables.delete(id); },
        updateDrawableSkinId() {},
        updateDrawablePosition(id, position) {
            const d = drawables.get(id);
            if (d) d.position = [position[0], position[1]];
        },
        updateDrawableDirectionScale() {}, updateDrawableVisible() {}, updateDrawableEffect() {},
        setDrawableOrder() {}, getDrawableOrder() { return 0; },
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; },
        drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; },
        drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; }, isTouchingDrawables() { return false; },
        penClear() {}, penStamp() {}, penLine() {}, penPoint() {}, draw() {}
    };
}

async function dynamicCensus() {
    const VM_ROOT = findVm();
    if (!VM_ROOT) {
        console.error('no Scratch VM found; set SCRATCH_VM_ROOT');
        process.exit(2);
    }
    const require_ = createRequire(import.meta.url);
    const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

    const vm = new VirtualMachine();
    vm.attachRenderer(nullRenderer());
    const originalWarn = console.warn;
    console.warn = () => {};
    const data = fs.readFileSync(file);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

    const runtime = vm.runtime;
    const turbo = !!runtime.compilerOptions;
    if (turbo) runtime.compilerOptions.enabled = true;
    const value = (t, name) => {
        const v = Object.values(t.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    const stage = () => runtime.getTargetForStage();

    /// Counted per opcode and per sprite. The wrapper is installed on
    /// `getOpcodeFunction` rather than on `_primitives` because the interpreter
    /// caches the function it finds in a `BlockCached` the first time a block
    /// runs; a patch that arrived after that cache was built would count
    /// nothing. `getOpcodeFunction` is what the cache is built from, so it is
    /// the one place a counter can be installed and be seen.
    const counts = new Map();
    const byTarget = new Map();
    const bump = (map, key) => map.set(key, (map.get(key) || 0) + 1);
    let counting = false;
    const real = runtime.getOpcodeFunction.bind(runtime);
    runtime.getOpcodeFunction = function (opcode) {
        const fn = real(opcode);
        if (typeof fn !== 'function') return fn;
        return function (...rest) {
            if (counting) {
                bump(counts, opcode);
                const thread = runtime.sequencer && runtime.sequencer.activeThread;
                const name = thread && thread.target ? thread.target.getName() : '(none)';
                bump(byTarget, name);
            }
            return fn.apply(this, rest);
        };
    };

    originalWarn(`vm        ${path.relative(repo, VM_ROOT)}` +
        `${turbo ? ' (TurboWarp)' : ' (vanilla scratch-vm)'}`);

    vm.greenFlag();
    runtime.currentStepTime = 1000 / 30;

    // Boot compiled, which is the only way a Linux guest gets anywhere, and
    // stop at the same marker `check-rv32.mjs` waits for.
    const bootUntil = Date.now() + budgetMs;
    let booted = false;
    while (Date.now() < bootUntil && !booted) {
        runtime._step();
        const trace = value(stage(), 'console_trace') || [];
        if (trace.length && Buffer.from(trace.map((b) => Number(b) & 0xff))
            .toString('latin1').includes('Run /init as init process')) booted = true;
    }
    const framesAt = Number(value(stage(), 'monitor_frames'));
    const runsAt = Number(value(stage(), 'monitor_runs'));
    const readsAt = Number(value(stage(), 'monitor_reads'));

    // Interpret a few frames, and count. The picture does not move while this
    // runs -- the guest is between slices -- so every counted pass is a pass
    // over the same picture.
    //
    // The compiler option alone is not enough: a thread decides once, at the
    // moment it is pushed, whether it is compiled, and a thread that is already
    // running stays compiled however the option changes. Clearing the flag on
    // the threads themselves is what hands them back to the interpreter, which
    // is where a block execution exists to be counted.
    counting = true;
    if (turbo) {
        runtime.compilerOptions.enabled = false;
        for (const thread of runtime.threads) thread.isCompiled = false;
    }
    for (let i = 0; i < profileSteps; i++) runtime._step();
    counting = false;

    const passes = Number(value(stage(), 'monitor_frames')) - framesAt;
    const runs = Number(value(stage(), 'monitor_runs')) - runsAt;
    const reads = Number(value(stage(), 'monitor_reads')) - readsAt;
    const total = [...counts.values()].reduce((a, b) => a + b, 0);
    const rows = [...counts.entries()].sort((a, b) => b[1] - a[1]);

    originalWarn(`booted    ${booted ? 'yes' : 'NO'} (compiled), profile over ` +
        `${profileSteps} interpreted runtime step(s)`);
    originalWarn(`passes    ${passes} whole-panel pass(es) counted, ${runs} runs, ` +
        `${reads} pixels read`);
    originalWarn(`blocks    ${total} block executions, ${passes ? Math.round(total / passes) : 0} ` +
        `per pass`);
    originalWarn('--- blocks per passing opcode');
    for (const [opcode, n] of rows) {
        originalWarn(`  ${String(n).padStart(9)}  ${String(passes ? Math.round(n / passes) : 0).padStart(9)}/pass  ${opcode}`);
    }
    originalWarn('--- by sprite');
    for (const [name, n] of [...byTarget.entries()].sort((a, b) => b[1] - a[1])) {
        originalWarn(`  ${String(n).padStart(9)}  ${String(passes ? Math.round(n / passes) : 0).padStart(9)}/pass  ${name}`);
    }

    if (jsonArg >= 0) {
        const out = jsonArg + 1 < args.length ? args[jsonArg + 1] : null;
        const payload = {
            file, vm: path.relative(repo, VM_ROOT), passes, runs, reads, total,
            perPass: passes ? total / passes : null,
            opcodes: Object.fromEntries(rows),
            targets: Object.fromEntries(byTarget)
        };
        const text = JSON.stringify(payload, null, 2) + '\n';
        if (out) {
            fs.mkdirSync(path.dirname(path.resolve(out)), { recursive: true });
            fs.writeFileSync(out, text);
        } else {
            process.stdout.write(text);
        }
    }
}

if (profile) await dynamicCensus();
else staticCensus();
