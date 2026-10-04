// Where a frame's time goes: the guest's arithmetic or the pen.
//
//     node tools/profile.mjs [baremetal|linux|scratch] [--keys] [--budget S]
//
// `check.mjs` says the guests run; this says what running costs, which is the
// question a report of "it stutters" asks. It boots the image in a real Scratch
// VM and reports four things:
//
//   * the frame, in milliseconds, and how many of them there were -- the runtime
//     steps at 30 frames a second, so a frame longer than 33 ms is a frame the
//     page did not get;
//   * the same frame split by thread, because the machine is one thread and the
//     console is another and the split *is* the answer to whether the stutter is
//     the guest or the drawing;
//   * the pen's volume, in lines a frame, since a line is a canvas stroke in the
//     browser and a counted nothing here;
//   * with `--keys`, the milliseconds between a keystroke and its echo, which is
//     the number a person typing actually feels.
//
// The pen's own milliseconds are the *project's* half of the drawing -- the
// compiled lookups, the pen blocks, the motion calls. The renderer here counts
// lines instead of stroking them, so the browser's half, a canvas path and a
// stroke a line, is not in these numbers and is the one thing to add by hand.
// The runtime's own compiler is on in this checkout, which is why the guest's
// arithmetic is a compiled JavaScript call and not a Scratch block: the block
// profiler in the VM counts nothing here and is not worth switching on.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';

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
const keys = args.includes('--keys');
const budgetIndex = args.indexOf('--budget');
const budgetMs = (budgetIndex >= 0 ? Number(args[budgetIndex + 1]) : 5) * 1000;
const name = args.find((a) => !a.startsWith('--') && !/^\d+$/.test(a)) ?? 'scratch';

function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    return {
        lines: 0,
        clears: 0,
        setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; },
        createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; },
        createPenSkin() { return nextId++; },
        destroySkin() {},
        updateSVGSkin() {},
        updateBitmapSkin() {},
        updateTextSkin() {},
        getSkinSize() { return [1, 1]; },
        getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; },
        getNativeSize() { return [480, 360]; },
        createDrawable() { const id = nextId++; drawables.set(id, { position: [0, 0] }); return id; },
        destroyDrawable(id) { drawables.delete(id); },
        updateDrawableSkinId() {},
        updateDrawablePosition(id, position) {
            const d = drawables.get(id);
            if (d) d.position = [position[0], position[1]];
        },
        updateDrawableDirectionScale() {},
        updateDrawableVisible() {},
        updateDrawableEffect() {},
        setDrawableOrder() {},
        getDrawableOrder() { return 0; },
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; },
        drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; },
        drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; },
        isTouchingDrawables() { return false; },
        penClear() { this.clears++; },
        penStamp() {},
        penLine() { this.lines++; },
        penPoint() { this.lines++; },
        draw() {}
    };
}

const sb3 = path.join(root, 'dist', `rv32${name}.sb3`);
const vmRoot = process.env.SCRATCH_VM_ROOT || path.resolve(repo, '..', 'scratch-vm');
const { default: VirtualMachine } = await import('file://' +
    path.join(vmRoot, 'src/virtual-machine.js').replace(/\\/g, '/'));
const vm = new VirtualMachine();
const renderer = recordingRenderer();
vm.attachRenderer(renderer);
console.error = () => {};
console.warn = () => {};
const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

const runtime = vm.runtime;
const target = (n) => runtime.targets.find((t) => t.getName() === n);
const bind = (t, n) => Object.values(t.variables).find((v) => v.name === n);
const val = (t, n) => { const v = bind(t, n); return v ? v.value : undefined; };
const instructions = () => val(target('RISCV'), 'instruction_n');

// Frame time by thread: which of them a frame's milliseconds belong to is the
// whole question.
const threadMs = new Map();
const sequencerProto = Object.getPrototypeOf(runtime.sequencer);
const stepThread = sequencerProto.stepThread;
sequencerProto.stepThread = function (thread) {
    const began = performance.now();
    try {
        stepThread.call(this, thread);
    } finally {
        const who = thread.target ? thread.target.getName() : '?';
        threadMs.set(who, (threadMs.get(who) || 0) + (performance.now() - began));
    }
};

// Row major, the way the console means it: reading the cells in list order walks
// down one column at a time, which is not a screen anyone can read.
const rows = () => {
    const g = val(target('Terminal'), 'glyphs');
    const out = [];
    for (let r = 0; r < 20; r++) {
        let s = '';
        for (let c = 0; c < 64; c++) {
            const code = g[c * 20 + r];
            s += code > 31 && code < 127 ? String.fromCharCode(code) : ' ';
        }
        out.push(s.replace(/\s+$/, ''));
    }
    return out;
};
const screen = () => rows().join('\n');

vm.greenFlag();
const prompt = name === 'baremetal' ? /Poweroff/ : (name === 'linux' ? /login:/ : /~ #/);
const began = Date.now();
const i0 = instructions();
const l0 = renderer.lines;
const c0 = renderer.clears;
const frames = [];
const slices = [];
let bootFrames = 0;
while (Date.now() - began < 240_000) {
    const t0 = Date.now();
    runtime._step();
    frames.push(Date.now() - t0);
    slices.push(val(target('RISCV'), 'runframe_elapsed'));
    bootFrames++;
    // Once every twenty frames: the screen is 1280 characters to read and the
    // guest's frame is 30 ms of wall clock, so reading it every frame takes the
    // time the machine was going to spend.
    if (bootFrames % 20 === 0 && prompt.test(screen())) break;
}
const end = Date.now();
const executed = instructions() - i0;
const median = (xs) => [...xs].sort((a, b) => a - b)[xs.length >> 1];

console.log(`${name}${keys ? ' + keys' : ''}`);
console.log(`boot      ${bootFrames} frames in ${((end - began) / 1000).toFixed(1)} s, ` +
    `${(executed / 1e6).toFixed(1)}M instructions, ${(executed / (end - began) * 1000 / 1e6).toFixed(2)}M/s`);
console.log(`frame     median ${median(frames)} ms, ${frames.filter((ms) => ms > 33).length} of ` +
    `${frames.length} over 33 ms (the runtime steps at 30 fps), ` +
    `machine's own slice ${median(slices).toFixed(0)} ms`);
console.log(`pen       ${renderer.lines - l0} lines, ${renderer.clears - c0} clears, ` +
    `${((renderer.lines - l0) / bootFrames).toFixed(0)} lines/frame`);
for (const [who, ms] of [...threadMs].sort((a, b) => b[1] - a[1])) {
    console.log(`thread    ${who.padEnd(9)} ${(ms / bootFrames).toFixed(1)} ms/frame ` +
        `(${(100 * ms / (end - began)).toFixed(0)}% of the clock)`);
}

if (keys) {
    // What a keystroke costs: send one and step until the console shows it.
    const before = screen();
    const from = renderer.lines;
    runtime.ioDevices.keyboard.postData({ key: 'a', isDown: true });
    runtime.ioDevices.keyboard.postData({ key: 'a', isDown: false });
    const keyed = Date.now();
    const waited = [];
    let echo = null;
    while (Date.now() - keyed < 60_000) {
        const t0 = Date.now();
        runtime._step();
        waited.push(Date.now() - t0);
        if (screen() !== before) { echo = Date.now() - keyed; break; }
    }
    console.log(`typing    echo after ${waited.length} frames and ${echo ?? '>60000'} ms; ` +
        `frame median ${median(waited)} ms; ${renderer.lines - from} pen lines`);
    if (!echo) process.exitCode = 1;
}
