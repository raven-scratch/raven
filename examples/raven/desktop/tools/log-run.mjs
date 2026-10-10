// Read a machine's runtime log: what it was doing when it stopped saying
// anything.
//
//     node tools/log-run.mjs --budget 120              the ARM board, 120 seconds
//     node tools/log-run.mjs --project rv32            the RISC-V board
//     node tools/log-run.mjs --tail 60                 the last 60 records only
//     node tools/log-run.mjs --all                     the whole append-log
//     node tools/log-run.mjs --grep abort              only records mentioning abort
//
// This tool **writes nothing**: no picture, no JSON, no log file. It is for
// reading, and a diagnostic that leaves files behind is a diagnostic the next
// person has to clean up. Everything it knows about the machine it prints to
// standard output.
//
// # Why it exists
//
// A guest that hangs says nothing. The console is silent, the Stage is a frozen
// picture, and there is no way to tell a guest spinning in a fault handler from
// one that has quietly stopped -- which is the situation the ARM board's Linux
// produces for minutes at a time. The machine writes its own account of the run
// into `machine_log` (`src/log.rav`), and this reads it back.
//
// Two lists are printed, because a stop has two questions:
//
//   * the **tail** is a ring of the most recent records, and it answers "what
//     was it doing when it stopped". This is printed by default.
//   * the **log** is append-only from power-on to a cap, and it answers "how far
//     did it get". `--all` prints it.
//
// # Running it
//
// The default runtime is TurboWarp, because the vanilla interpreter retires
// about seven thousand guest instructions a second and would take four hours to
// reach the point where the ARM board has something to say. The log is the same
// either way -- it is the guest that is deterministic, not the host -- so the
// fast one is used and `--vm vanilla` is there for when the question is what the
// *interpreter* is doing.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
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
const budgetMs = Number(arg('--budget', 300)) * 1000;
const vmChoice = arg('--vm', 'turbo');
const tailCount = Number(arg('--tail', 48));
const showAll = args.includes('--all');
const grep = arg('--grep', null);

/// The projects this directory builds. `counter` and `pc` are the guest-state
/// variables each board publishes; they differ because the two boards are two
/// machines and each names its own state.
const PROJECTS = {
    arm: {
        file: 'dist/desktop-arm-virt-linux.sb3',
        counter: 'cpu_instructions',
        pc: 'cpu_pc',
        what: 'ARM Versatile-PB, Linux 6.6'
    },
    rv32: {
        file: 'dist/desktop-rv32-linux.sb3',
        counter: 'rv_instructions',
        pc: 'rv_pc',
        what: 'RISC-V mini-rv32, Linux 6.8'
    },
    doom: {
        file: 'dist/desktop-rv32-doom.sb3',
        counter: 'rv_instructions',
        pc: 'rv_pc',
        what: 'RISC-V mini-rv32, embeddedDOOM'
    }
};
const spec = PROJECTS[arg('--project', 'arm')];
if (!spec) {
    console.error(`unknown project; try ${Object.keys(PROJECTS).join(', ')}`);
    process.exit(2);
}

/// Where the Scratch VM lives. The same three candidates every other tool in
/// this directory tries, and the same rule: an environment variable wins, then
/// the TurboWarp checkout beside the repository, then the vanilla one inside it.
/// Nothing here is specific to one machine -- see `tools/README` on portability.
const candidates = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean);
const VM_ROOT = candidates.find((d) => fs.existsSync(path.join(d, 'src', 'virtual-machine.js')));
if (!VM_ROOT) {
    console.error('no Scratch VM found. Looked in:\n  ' + candidates.join('\n  ') +
        '\nSet SCRATCH_VM_ROOT to a scratch-vm or TurboWarp checkout.');
    process.exit(2);
}

const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

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

const file = path.join(root, spec.file);
if (!fs.existsSync(file)) {
    console.error(`no ${spec.file}; run tools/build.mjs first`);
    process.exit(2);
}

const vm = new VirtualMachine();
vm.attachRenderer(nullRenderer());
const warnings = console.warn;
console.warn = () => {};
console.error = () => {};
const data = fs.readFileSync(file);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

const runtime = vm.runtime;
if (runtime.compilerOptions) runtime.compilerOptions.enabled = !args.includes('--no-compile');
runtime.currentStepTime = 1000 / 30;

const stage = runtime.getTargetForStage();
const value = (name) => {
    const v = Object.values(stage.variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};
/// A Scratch list as an array of strings. The list is the machine's own, so it
/// holds what the machine pushed and nothing else.
const list = (name) => {
    const v = value(name);
    return Array.isArray(v) ? v.map(String) : [];
};

vm.greenFlag();
const started = Date.now();
let steps = 0;
while (Date.now() - started < budgetMs) {
    runtime._step();
    steps += 1;
}
const secs = (Date.now() - started) / 1000;
console.warn = warnings;

/// The ring in the order it was written.
///
/// Once the ring is full the oldest record is the one about to be overwritten,
/// which is the slot `machine_log_tail_next` points at -- so the run from there
/// to the end is the older half and the run before it is the newer one.
function orderedTail(ring, next, max) {
    if (ring.length < max || next <= 0 || next >= ring.length) return ring;
    return ring.slice(next).concat(ring.slice(0, next));
}

const log = list('machine_log');
const ringMax = Number(value('machine_log_tail_max')) || 0;
const ring = orderedTail(
    list('machine_log_tail'),
    Number(value('machine_log_tail_next')) || 0,
    ringMax
);
const dropped = Number(value('machine_log_dropped')) || 0;
const enabled = Number(value('machine_log_on')) !== 0;

const grepOf = (lines) => (grep === null ? lines : lines.filter((l) => l.includes(grep)));
const numbered = (lines, from) => lines.map((l, i) => `  ${String(from + i).padStart(6)}  ${l}`);

const total = Number(value(spec.counter)) || 0;
const pc = Number(value(spec.pc)) || 0;

console.log('');
console.log(`project       ${spec.file}  (${spec.what})`);
console.log(`vm            ${path.relative(repo, VM_ROOT)}` +
    `${runtime.compilerOptions && !args.includes('--no-compile') ? ' (compiled)' : ' (interpreting)'}`);
console.log(`guest         ${total.toLocaleString('en-US')} instructions retired, ` +
    `pc=0x${(pc >>> 0).toString(16)}`);
console.log(`run           ${secs.toFixed(1)} s, ${steps} runtime steps`);
console.log(`log           ${enabled ? 'on' : 'OFF'} — ${log.length} record(s) kept, ` +
    `${dropped} refused at the cap` +
    (grep === null ? '' : `, filtered by ${JSON.stringify(grep)}`));
console.log('');

if (!enabled) {
    console.log('The log is off for this build (`machine_log_on` is zero), so there is');
    console.log('nothing to read. It is on by default in `src/log.rav`.');
    process.exit(0);
}

if (showAll) {
    const lines = grepOf(log);
    console.log(`--- the whole log, from power-on (${lines.length} record(s)) ---`);
    for (const line of numbered(lines, 0)) console.log(line);
    console.log('--- end of log ---');
} else {
    // The default view is the tail, because a stop is read from its last
    // moments and the append-log's beginning is usually the boot phase.
    const lines = grepOf(ring);
    const shown = lines.slice(Math.max(0, lines.length - tailCount));
    console.log(`--- the last ${shown.length} record(s) before the run ended ---`);
    for (const line of numbered(shown, Math.max(0, lines.length - shown.length))) {
        console.log(line);
    }
    console.log('--- end of tail ---');
    console.log('');
    console.log(`--all prints the ${log.length} record(s) from power-on instead.`);
    console.log(`--grep <text> filters; --tail <n> shows more or fewer of these.`);
}
console.log('');
