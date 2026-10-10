// Does the optimiser change what a project costs to run?
//
//     node tools/bench-optimize.mjs --project arm --budget 900
//     node tools/bench-optimize.mjs --project rv32 --budget 400
//
// The optimiser owes its callers two things and this tool answers the second
// one: it must not worsen what the project costs to run. The first -- that it
// must not change a project's results -- is proved in
// `crates/raven-re/tests/roundtrip.rs`, where the optimised build is held
// against the round-trip fingerprint of everything Scratch can observe.
//
// A block count is not that proof. Fewer blocks is the *intent*, and an
// optimiser that removed a block the VM re-executes was cheap in the editor and
// expensive in the run -- so this measures both builds in the same process, one
// after the other, and reports the pair.
//
// # Why the two builds are loaded side by side rather than in two runs
//
// The seconds are the host's as well as the machine's: the README records this
// same check ranging 25.9 s to 55.1 s on one machine. Comparing a run from
// before a change against a run after it therefore measures the host. Both
// builds are loaded into the *same process* and stepped alternately, so the two
// numbers in one row share a host, and the guest instruction count -- which
// belongs to the emulated machine -- is printed beside them as the check that
// the two builds really are running the same program.
//
// The builds are made by this tool from the source, so what it compares is what
// `raven build` and `raven build --no-optimize` produce and not two files
// somebody happened to leave in `dist/`.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');

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

const args = process.argv.slice(2);
const arg = (name, fallback) => {
    const i = args.indexOf(name);
    return i >= 0 ? args[i + 1] : fallback;
};
const budgetMs = Number(arg('--budget', 900)) * 1000;
const which = arg('--project', 'arm');

/// The manifests to compare, and the guest instruction counter each board has.
const PROJECTS = {
    arm: {
        manifest: 'raven.toml',
        counter: 'cpu_instructions',
        what: 'the ARM Versatile-PB, Linux 6.6'
    },
    rv32: {
        manifest: 'raven-rv32.toml',
        counter: 'rv_instructions',
        what: 'the RISC-V mini-rv32, Linux 6.8'
    },
    doom: {
        manifest: 'raven-rv32-doom.toml',
        counter: 'rv_instructions',
        what: 'the RISC-V mini-rv32, bare-metal Doom'
    }
};
const spec = PROJECTS[which];
if (!spec) {
    console.error(`unknown project \`${which}\`; try ${Object.keys(PROJECTS).join(', ')}`);
    process.exit(2);
}
const manifest = path.join(root, spec.manifest);

const VM_ROOT = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : path.join(repo, 'ref', 'turbowarp-vm');
const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// The `project.json` out of an `.sb3`, as text.
///
/// The same central-directory walk `probe-blocks.mjs` does, and for the same
/// reason: raven-asm writes every entry *stored*, so the compression method is
/// read rather than assumed.
function projectJson(bytes) {
    let eocd = bytes.length - 22;
    while (eocd >= 0 && bytes.readUInt32LE(eocd) !== 0x06054b50) eocd--;
    if (eocd < 0) throw new Error('not a zip');
    const count = bytes.readUInt16LE(eocd + 10);
    let at = bytes.readUInt32LE(eocd + 16);
    for (let i = 0; i < count; i++) {
        const nameLen = bytes.readUInt16LE(at + 28);
        const extraLen = bytes.readUInt16LE(at + 30);
        const commentLen = bytes.readUInt16LE(at + 32);
        const name = bytes.toString('latin1', at + 46, at + 46 + nameLen);
        if (name === 'project.json') {
            const method = bytes.readUInt16LE(at + 10);
            const local = bytes.readUInt32LE(at + 42);
            const lName = bytes.readUInt16LE(local + 26);
            const lExtra = bytes.readUInt16LE(local + 28);
            const size = bytes.readUInt32LE(at + 20);
            const start = local + 30 + lName + lExtra;
            const raw = bytes.subarray(start, start + size);
            const json = method === 0 ? raw.toString('utf8')
                : require('node:zlib').inflateRawSync(raw).toString('utf8');
            return JSON.parse(json);
        }
        at += 46 + nameLen + extraLen + commentLen;
    }
    throw new Error('no project.json in the archive');
}

/// Count the blocks in an archive, which is the number this tool is about.
function blockCount(bytes) {
    const project = projectJson(bytes);
    return project.targets.reduce((n, t) => n + Object.keys(t.blocks).length, 0);
}

/// Build the project twice and hand back the two archives.
///
/// `raven` is invoked as a binary rather than through its library so that what
/// is compared is what a user gets from the two command lines, flags included.
/// The block count is read back out of the archive rather than out of the
/// build's own report, because the archive is what runs.
function buildBoth() {
    const out = {};
    for (const optimize of [true, false]) {
        const name = optimize ? 'on' : 'off';
        process.stderr.write(`  building with the optimiser ${name} ...\n`);
        const flags = ['run', '-q', '-p', 'raven', '--', 'build', '-m', manifest];
        if (!optimize) flags.push('--no-optimize');
        execFileSync('cargo', flags, {
            cwd: repo,
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe']
        });
        // The build writes `dist/<name>.sb3`; read it back and keep a copy,
        // because the second build overwrites the first.
        const artifact = path.join(root, 'dist', `desktop-${
            which === 'arm' ? 'arm-virt-linux' : which === 'rv32' ? 'rv32-linux' : 'rv32-doom'
        }.sb3`);
        const bytes = fs.readFileSync(artifact);
        fs.writeFileSync(path.join(root, 'dist', `optimize-${name}.sb3`), bytes);
        out[name] = { bytes, blocks: blockCount(bytes), artifact };
    }
    return out;
}

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

/// Load one archive and hand back the handles a run needs.
async function open(bytes, label) {
    const vm = new VirtualMachine();
    vm.attachRenderer(nullRenderer());
    const warnings = console.warn;
    console.warn = () => {};
    console.error = () => {};
    await vm.loadProject(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    console.warn = warnings;
    const runtime = vm.runtime;
    if (runtime.compilerOptions) runtime.compilerOptions.enabled = true;
    const stage = runtime.getTargetForStage();
    const value = (name) => {
        const v = Object.values(stage.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    const machine = runtime.targets.find((t) => t.getName() === 'Machine');
    return {
        label,
        runtime,
        /// The guest's own instruction counter, which is the one number that
        /// belongs to the emulated machine and therefore reproduces.
        instructions: () => Number(value(spec.counter)) || 0,
        /// The guest's console, which is what says a boot got where it was
        /// going rather than merely retiring instructions.
        console: () => Buffer.from(
            (value('console_trace') || []).map((b) => Number(b) & 0xff)
        ).toString('latin1'),
        /// Stop this machine at an exact guest instruction, with the machine's
        /// own `machine_limit`.
        ///
        /// This is the only bound in the project that is the *guest's*. A step
        /// of the harness is budgeted by wall clock and retires a variable
        /// amount of machine, so two builds stopped by "both have printed
        /// /init" end an arbitrary number of slices apart -- the first two
        /// measurements of this tool reported an 8192- and then a 16384-
        /// instruction difference, and both were the stopping rule rather than
        /// the optimiser. `machine_limit` stops at the instruction, so the two
        /// runs can be compared over identical work.
        limit(value) {
            const v = Object.values(machine.variables).find((x) => x.name === 'machine_limit');
            if (v) v.value = value;
        },
        halted: () => Number(value('cpu_halted')) > 0,
        start() {
            vm.greenFlag();
            runtime.currentStepTime = 1000 / 30;
        }
    };
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const builds = buildBoth();
const a = await open(builds.on.bytes, 'optimized');
const b = await open(builds.off.bytes, 'plain');
a.start();
b.start();

// Alternating steps, so both builds see the same host, and the wall clock is
// accumulated per build rather than as a total.
//
// **The comparison runs a fixed number of guest instructions on each build.**
// That is the only bound in this project that belongs to the emulated machine.
// Bounding by wall clock, or by "both have printed /init", leaves the two an
// arbitrary number of 8192-instruction slices apart, and the first two versions
// of this tool reported exactly that gap as though it were a result.
//
// So: step until the marker, then set `machine_limit` on both to the same
// instruction count and step until both halt there. Everything measured is the
// work after the limit is set, so the two rows are the same program doing the
// same amount of it.
let msA = 0;
let msB = 0;
let steps = 0;
const started = Date.now();

// Phase 1: boot to the marker, so the limit is set on a machine that is doing
// real work rather than on the boot ROM.
while (Date.now() - started < budgetMs) {
    a.runtime._step();
    b.runtime._step();
    if (a.console().includes('Run /init as init process')) break;
}

// Phase 2: the measured window. Both stop at the same guest instruction.
const target = Math.max(a.instructions(), b.instructions()) + 4_000_000;
const startA = a.instructions();
const startB = b.instructions();
a.limit(target);
b.limit(target);
const stepsAtStart = steps;
while (Date.now() - started < budgetMs) {
    let t = process.hrtime.bigint();
    a.runtime._step();
    msA += Number(process.hrtime.bigint() - t) / 1e6;
    t = process.hrtime.bigint();
    b.runtime._step();
    msB += Number(process.hrtime.bigint() - t) / 1e6;
    steps += 1;
    if (a.halted() && b.halted()) break;
}
const measured = steps - stepsAtStart;
const instrA = a.instructions() - startA;
const instrB = b.instructions() - startB;

const fmt = (n) => n.toLocaleString('en-US');
console.log('');
console.log(`project       ${which}: ${spec.what}`);
console.log(`vm            ${path.relative(repo, VM_ROOT)}`);
console.log(`measured      ${measured} runtime steps after booting to the marker, ` +
    `${((Date.now() - started) / 1000).toFixed(1)} s total`);
console.log('');
console.log(`  ${'build'.padEnd(11)} ${'blocks'.padStart(8)} ${'guest instr'.padStart(14)} ` +
    `${'ms'.padStart(10)}  ${'s / 1M instr'.padStart(12)}`);
for (const [label, ms, instr] of [['optimized', msA, instrA], ['plain', msB, instrB]]) {
    const blocks = builds[label === 'optimized' ? 'on' : 'off'].blocks;
    console.log(`  ${label.padEnd(11)} ${String(blocks ?? '--').padStart(8)} ${fmt(instr).padStart(14)} ` +
        `${ms.toFixed(0).padStart(10)}  ${(ms / (instr / 1e6)).toFixed(1).padStart(12)}`);
}
console.log('');
const blockDelta = (builds.on.blocks ?? 0) - (builds.off.blocks ?? 0);
console.log(`blocks        ${blockDelta} (${(blockDelta / (builds.off.blocks || 1) * 100).toFixed(1)}%)`);
console.log(`instructions  ${instrA - instrB} (${((instrA - instrB) / (instrB || 1) * 100).toFixed(2)}%) ` +
    `-- a difference here means the two builds are not running the same program`);
const perMillion = (msA / (instrA / 1e6)) - (msB / (instrB / 1e6));
console.log(`cost          ${perMillion >= 0 ? '+' : ''}${perMillion.toFixed(1)} ms per million guest ` +
    `instructions (negative is the optimiser winning)`);
console.log('');
console.log(`archives      dist/optimize-on.sb3, dist/optimize-off.sb3`);

const jsonAt = path.join(root, 'dist', 'bench-optimize.json');
fs.writeFileSync(jsonAt, JSON.stringify({
    project: which,
    vm: path.relative(repo, VM_ROOT),
    steps,
    optimized: { blocks: builds.on.blocks, instructions: instrA, milliseconds: msA },
    plain: { blocks: builds.off.blocks, instructions: instrB, milliseconds: msB }
}, null, 2) + '\n');
console.log(`written       ${path.relative(process.cwd(), jsonAt)}`);
