// A diagnostic: boot the RISC-V board and say where the guest is.
//
//     node tools/watch-rv32.mjs --budget 300
//     node tools/watch-rv32.mjs --boot                       time the boot
//     node tools/watch-rv32.mjs --boot --slice 1048576       at another slice
//     node tools/watch-rv32.mjs --boot --slice-us 0          with no time bound
//
// This is not `check-rv32.mjs`. It asserts nothing. Every second it prints the
// guest's instruction count, its program counter and the last few bytes it
// transmitted, so a boot that stops can be told from a boot that is slow and
// the place it stopped can be read off the PC ring.
//
// `--boot` stops the run the moment the guest's console says the kernel has
// handed over to `/init`, and prints the two numbers a boot is measured in:
// seconds of wall clock and guest instructions retired. That is the same
// marker `check-rv32.mjs` waits for, without the typing that follows it, which
// is what makes it the instrument to run a change against rather than to
// validate one with. `--until <text>` stops at any other text instead.
//
// `--slice N` sets `machine_slice`, which is how the cost of a shorter or a
// longer slice is measured rather than guessed. A slice stops at
// `machine_slice` instructions *or* `machine_slice_us` microseconds, whichever
// comes first, and `--slice-us N` sets the second: the time bound is what keeps
// a frame a frame, so turning it off (`--slice-us 0`) is how the instruction
// bound is measured on its own. Both defaults are the *board file's*, applied
// by the machine when it powers on, so an override is re-applied every step
// rather than once before the run.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');

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

const vmRoot = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(root, '..', '..', '..', 'ref', 'turbowarp-vm'),
    path.join(root, '..', '..', '..', 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean).find((d) => fs.existsSync(path.join(d, 'src', 'virtual-machine.js')));
const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(vmRoot, 'src', 'virtual-machine.js'));

const args = process.argv.slice(2);
const budgetArg = args.indexOf('--budget');
const budget = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 300) * 1000;
const everyArg = args.indexOf('--every');
const every = (everyArg >= 0 ? Number(args[everyArg + 1]) : 1) * 1000;
const sb3 = args.find((a) => a.endsWith('.sb3')) || path.join(root, 'dist', 'desktop-rv32-linux.sb3');
const pcFrom = args.indexOf('--from');
const from = pcFrom >= 0 ? Number(args[pcFrom + 1]) : 0;

const vm = new VirtualMachine();
vm.attachRenderer({
    setLayerGrouping() {}, setLayerGroupOrdering() {}, setDrawableOrder() {},
    createSVGSkin() { return 1; }, createBitmapSkin() { return 1; }, createTextSkin() { return 1; },
    createPenSkin() { return 1; }, destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {},
    updateTextSkin() {}, getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
    getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [480, 360]; },
    createDrawable() { return 1; }, destroyDrawable() {}, updateDrawableSkinId() {},
    updateDrawablePosition() {}, updateDrawableDirectionScale() {}, updateDrawableVisible() {},
    updateDrawableEffect() {}, getDrawableOrder() { return 0; },
    getFencedPositionOfDrawable(_i, p) { return [p[0], p[1]]; },
    getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
    getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
    pick() { return -1; }, drawableTouching() { return false; },
    drawableTouchingScratchPoint() { return false; }, drawableTouchingScratchRect() { return false; },
    isTouchingColor() { return false; }, isTouchingDrawables() { return false; },
    penClear() {}, penStamp() {}, penLine() {}, penPoint() {}, draw() {}
});

const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

const runtime = vm.runtime;
if (runtime.compilerOptions) runtime.compilerOptions.enabled = !args.includes('--no-compile');
const value = (name) => {
    const t = runtime.getTargetForStage();
    const v = Object.values(t.variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};

const ring = [];
vm.greenFlag();
runtime.currentStepTime = 1000 / 30;

/// The kernel's printk ring, read straight out of the guest's RAM. A kernel
/// that has stopped inside `console_lock` holds messages nobody will ever see
/// on the console, and this is where they are.
const logArg = args.indexOf('--log');
const logAt = logArg >= 0 ? Number(args[logArg + 1]) : 0;
const tailArg = args.indexOf('--tail');
const tailBytes = tailArg >= 0 ? Number(args[tailArg + 1]) : 64;
const dumpLog = () => {
    if (!logAt) return;
    const ram = value('rv_ram') || [];
    const bytes = [];
    for (let i = logAt - 0x80000000; i < logAt - 0x80000000 + 16384; i++) {
        bytes.push(Number(ram[i + 1] || 0) & 0xff);
    }
    const text = Buffer.from(bytes).toString('latin1');
    const runs = text.match(/[\x20-\x7e\n\t]{12,}/g) || [];
    console.log('--- kernel log buffer ---');
    console.log(runs.join('\n'));
    console.log('--- end kernel log buffer ---');
};

// `--stop N` cuts the machine's slice down once the guest has retired N
// instructions, so that the sample below is N/1024-wide instead of one point
// per quarter of a million instructions. A spin in the kernel is a handful of
// addresses and a coarse sample lands in the middle of it by luck; this makes
// it land there by arithmetic.
const machine = runtime.targets.find((t) => t.getName() === 'Machine');
const setSprite = (name, value) => {
    const v = machine && Object.values(machine.variables).find((x) => x.name === name);
    if (v) v.value = value;
};
const getSprite = (name) => {
    const v = machine && Object.values(machine.variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};
const setSlice = (n) => setSprite('machine_slice', n);
const sliceArg = args.indexOf('--slice');
const sliceOverride = sliceArg >= 0 ? Number(args[sliceArg + 1]) : null;
const sliceUsArg = args.indexOf('--slice-us');
const sliceUsOverride = sliceUsArg >= 0 ? Number(args[sliceUsArg + 1]) : null;
/// The board file is where both numbers come from, and the machine applies them
/// at power-on, so an override has to be re-applied after that rather than
/// before it. Once a slice is the same value every slice, which is what a
/// measurement wants anyway.
const applySlice = () => {
    if (sliceOverride !== null) setSlice(sliceOverride);
    if (sliceUsOverride !== null) setSprite('machine_slice_us', sliceUsOverride);
};
applySlice();

// What the run stops at, if anything. The default marker is the one the boot
// check waits for: the last thing the serial port has to say before `/init` is
// the shell.
const untilArg = args.indexOf('--until');
const stopAt = untilArg >= 0 ? args[untilArg + 1]
    : args.includes('--boot') ? 'Run /init as init process'
        : null;

// `--idle N`: after the marker, keep running for N more seconds and report what
// the pen did *in that window*. That is the running display -- a shell sitting
// at a prompt with a blinking cursor -- and the window's guest instructions are
// printed beside it, because the two projects this is used to compare do not
// retire the same number of them in a second and a raw per-second total would
// be a comparison of how fast the host ran rather than of what the monitor cost.
const idleArg = args.indexOf('--idle');
const idle = idleArg >= 0 ? Number(args[idleArg + 1]) : 0;

// The console, rebuilt only when the guest has transmitted something. A boot
// writes a few kilobytes over a few hundred steps, so this is a few hundred
// small strings rather than one per sample.
let traceLen = -1;
let traceText = '';
const consoleNow = () => {
    const trace = value('console_trace') || [];
    if (trace.length !== traceLen) {
        traceLen = trace.length;
        traceText = Buffer.from(trace.map((b) => Number(b) & 0xff)).toString('latin1');
    }
    return traceText;
};

const started = Date.now();
let next = 0;
let hist = new Map();
let chopped = false;
let booted = false;
let bootedAt = 0;
let bootedInstr = 0;
while (Date.now() - started < budget) {
    runtime._step();
    applySlice();
    const pc = Number(value('rv_pc')) >>> 0;
    if (ring[ring.length - 1] !== pc) {
        ring.push(pc);
        if (ring.length > 48) ring.shift();
    }
    hist.set(pc, (hist.get(pc) || 0) + 1);
    const n = Number(value('rv_instructions'));
    if (stopAt !== null && consoleNow().includes(stopAt)) {
        booted = true;
        bootedAt = (Date.now() - started) / 1000;
        bootedInstr = n;
        break;
    }
    if (!chopped && from > 0 && n >= from) {
        chopped = true;
        // 64 and not something smaller: a slice is rounded down to a multiple
        // of 64, so a slice of 16 is a slice of zero and the machine stops.
        // The frame budget goes up with it, or the machine stops itself on the
        // far more slices a small slice needs to reach the same instruction.
        setSlice(64);
        setSprite('machine_budget', 100000000);
        console.log(`--- slice cut to 64 at ${n} instructions ---`);
    }
    if (Date.now() - started >= next) {
        next += every;
        const bytes = value('console_trace') || [];
        const tail = Buffer.from(bytes.slice(-tailBytes).map((b) => Number(b) & 0xff)).toString('latin1');
        const top = [...hist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)
            .map(([p, c]) => `0x${p.toString(16)}:${c}`).join(' ');
        console.log(`t=${((Date.now() - started) / 1000).toFixed(0)}s  instr=${n}  pc=0x${pc.toString(16)}  ` +
            `console=${bytes.length}b\n      tail: ${JSON.stringify(tail.replace(/\r/g, '\\r'))}\n      hot: ${top}`);
        hist = new Map();
        if (chopped) {
            console.log(`      ring: ${ring.map((p) => '0x' + p.toString(16)).join(' ')}`);
        }
        if (logAt && n >= from) dumpLog();
    }
}
if (booted) {
    const slice = getSprite('machine_slice');
    console.log(`boot          ${JSON.stringify(stopAt)}`);
    console.log(`seconds       ${bootedAt.toFixed(1)} s`);
    console.log(`instructions  ${bootedInstr}`);
    console.log(`rate          ${Math.round(bootedInstr / bootedAt)} guest instructions/s`);
    console.log(`slice         ${slice} instructions per machine slice`);
    console.log(`slice         ${getSprite('machine_slice_us')} microseconds per machine slice` +
        ` (0 is no time bound)`);
    console.log(`monitor       ${value('monitor_frames')} frames, ${value('monitor_reads')} pixels read, ` +
        `${value('monitor_runs')} runs drawn`);
    if (idle > 0) {
        const readsAt = Number(value('monitor_reads'));
        const runsAt = Number(value('monitor_runs'));
        const instrAt = Number(value('rv_instructions'));
        const framesAt = Number(value('monitor_frames'));
        const idleStart = Date.now();
        let steps = 0;
        while (Date.now() - idleStart < idle * 1000) { runtime._step(); steps += 1; }
        const idleSeconds = (Date.now() - idleStart) / 1000;
        const idleInstr = Number(value('rv_instructions')) - instrAt;
        const reads = Number(value('monitor_reads')) - readsAt;
        const runs = Number(value('monitor_runs')) - runsAt;
        const passes = Number(value('monitor_frames')) - framesAt;
        console.log(`idle          ${idleSeconds.toFixed(1)} s at the prompt, ${steps} runtime steps`);
        // What the owner's question is in one number: a Scratch frame is 33.3 ms
        // and this is what one runtime step of this machine costs, guest slice
        // and whole-panel repaint together. A step longer than the frame is a
        // display that redraws at 30/n and nothing about the pens can change it.
        console.log(`idle step     ${(idleSeconds / steps * 1000).toFixed(2)} ms, ` +
            `${(steps / idleSeconds).toFixed(1)} steps a second, ` +
            `${passes} whole-panel passes, ` +
            `${Math.round(runs / Math.max(1, passes))} runs a pass, ` +
            `${Math.round(idleInstr / Math.max(1, steps))} guest instructions a step`);
        console.log(`idle guest    ${idleInstr} instructions`);
        console.log(`idle pixels   ${reads} read ` +
            `(${Math.round(reads / idleInstr * 1000000)} a million guest instructions)`);
        console.log(`idle runs     ${runs} drawn ` +
            `(${Math.round(runs / idleInstr * 1000000)} a million guest instructions)`);
    }
} else if (stopAt !== null) {
    console.log(`did not reach ${JSON.stringify(stopAt)} in ${(budget / 1000).toFixed(0)} s`);
    process.exitCode = 1;
}
