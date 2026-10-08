// Play the built Doom project: press keys at the machine and look at the card.
//
//     node tools/probe-play.mjs                       dist/desktop-rv32-doom.sb3
//     node tools/probe-play.mjs <file.sb3> --boot 3000 --window 150
//
// `tools/check-rv32.mjs --guest doom` is the check: it asserts, and what it
// asserts about input is the forward key and the picture it moves. This is the
// instrument beside it and the one to reach for when the *mapping* is in
// question rather than the machine -- it boots the same project, holds each key
// the way a player would, and prints how many of the card's pixels changed
// next to the same window with no key pressed at all. Nothing here asserts
// anything; `dist/probe-*.png` are the frames for a reader to look at.
//
// The control matters. This guest redraws its status bar and its own face
// whether or not anyone is playing, so a window with no key is never zero
// pixels, and a key that did nothing is a number of the same size as that
// window's. What each key is supposed to do is in the README:
//
//     up arrow, w    forward       enter     a menu entry
//     down arrow, s  back          space     use/open
//     left arrow, a  turn left     , .       strafe left and right
//     right arrow, d turn right
//
// Fire and run are **not** rows here, and the reason is the runtime rather than
// the board. They are `control` and `shift` on the card's own contract, and a
// *held* modifier is a poll's row: `poll_card` reads `key pressed?` for names
// both runtimes know, and neither of these is one. They are delivered by hats
// instead -- one report per key-down, which the driver's own `FB_KEY_DOWN_TICS`
// holds for four of its tics -- so a shot or a run is a tap here, and on a
// vanilla runtime it is nothing at all, because vanilla drops a key whose name is
// longer than one character before any block sees it
// (`ref/scratch-vm/node_modules/scratch-vm/src/io/keyboard.js:47-67`, `:115-118`).
// Holding `control` to keep firing is therefore not possible; pressing it is, on
// TurboWarp, and the palette that used to do it is gone.
//
// A key is *held* rather than tapped, because that is what the sprite does with
// it: `poll_card` reports a held key again as soon as the guest has read the
// last report, so "hold" is the honest way to press one. The rows that are not
// in `poll_card` -- `space` and `,` -- are hats, so holding one is one report:
// the harness posts a single key-down and the browser's auto-repeat is what
// would make a real held key repeat.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { encodePng } from './png.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');

const STUBS = {
    '@scratch/scratch-svg-renderer': () => ({
        sanitizeSvg: { sanitizeByteStream: (d) => d },
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

const VM_ROOT = [process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')].filter(Boolean)
    .find((d) => fs.existsSync(path.join(d, 'src', 'virtual-machine.js')));
const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// A renderer that records the pen instead of drawing it. The monitor's output
/// is the whole of what the pen does here, and a run of a few thousand frames
/// issues millions of lines, so the oldest are dropped rather than kept: the
/// pictures this writes are the card's, read out of the card's own memory.
function recordingRenderer() {
    let id = 1;
    const lines = [];
    const trim = () => { if (lines.length > 400000) lines.splice(0, lines.length - 200000); };
    return {
        lines,
        setLayerGrouping() {}, setLayerGroupOrdering() {},
        createSVGSkin() { return id++; }, createBitmapSkin() { return id++; },
        createTextSkin() { return id++; }, createPenSkin() { return id++; },
        destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {}, updateTextSkin() {},
        getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [480, 360]; },
        createDrawable() { return id++; }, destroyDrawable() {},
        updateDrawableSkinId() {}, updateDrawablePosition() {}, updateDrawableDirectionScale() {},
        updateDrawableVisible() {}, updateDrawableEffect() {}, setDrawableOrder() {},
        getDrawableOrder() { return 0; }, getFencedPositionOfDrawable(_i, p) { return [p[0], p[1]]; },
        getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; }, drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; }, drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; }, isTouchingDrawables() { return false; },
        penClear() { lines.length = 0; }, penStamp() {},
        penLine(_s, a, x0, y0, x1, y1) {
            lines.push({ x0, y0, x1, y1, pen: a.diameter, colour: a.color4f ? [...a.color4f] : null });
            trim();
        },
        penPoint(_s, a, x, y) {
            lines.push({ x0: x, y0: y, x1: x, y1: y, pen: a.diameter, colour: a.color4f ? [...a.color4f] : null });
            trim();
        },
        draw() {}
    };
}

const args = process.argv.slice(2);
const sb3 = args.find((a) => a.endsWith('.sb3')) || path.join(root, 'dist', 'desktop-rv32-doom.sb3');
const bootFrames = Number(args[args.indexOf('--boot') + 1] || 3000);
const windowFrames = Number(args[args.indexOf('--window') + 1] || 150);

const vm = new VirtualMachine();
vm.attachRenderer(recordingRenderer());
console.warn = () => {};
const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
const runtime = vm.runtime;
if (runtime.compilerOptions) runtime.compilerOptions.enabled = true;
runtime.currentStepTime = 1000 / 30;

const stage = () => runtime.getTargetForStage();
const value = (name) => {
    const v = Object.values(stage().variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};
const step = (n) => { for (let i = 0; i < n; i++) runtime._step(); };

const board = (await import(pathToFileURL(path.join(root, 'boards', 'mini-rv32.mjs')).href)).default;
const panel = { ...board.display, ...board.formats[board.display.format] };

/// The card's picture as the card holds it: the latched indices through the
/// latched palette, which is what the monitor scans out.
const cardPixels = () => {
    const out = new Uint32Array(panel.width * panel.height);
    const pixels = value('efb_scan_pixels') || [];
    const palette = value('efb_scan_palette') || [];
    for (let i = 0; i < out.length; i++) out[i] = Number(palette[Number(pixels[i + 1] || 0) + 1] || 0) >>> 0;
    return out;
};
const cardRgb = (px) => {
    const rgb = Buffer.alloc(px.length * 3);
    for (let i = 0; i < px.length; i++) {
        rgb[i * 3] = (px[i] >>> 16) & 0xff;
        rgb[i * 3 + 1] = (px[i] >>> 8) & 0xff;
        rgb[i * 3 + 2] = px[i] & 0xff;
    }
    return rgb;
};
const shot = (name, px) =>
    fs.writeFileSync(path.join(root, 'dist', `probe-${name}.png`),
        encodePng(panel.width, panel.height, cardRgb(px)));

/// Doom's status bar is the bottom 32 rows of its 200; everything above it is
/// the level seen from where the player is standing, plus the weapon.
const diff = (a, b) => {
    const viewPixels = panel.width * (panel.height - 32);
    let all = 0, view = 0;
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) { all++; if (i < viewPixels) view++; }
    return { all, view, bar: all - view };
};

vm.greenFlag();
const t0 = Date.now();
console.log(`booting ${bootFrames} frames of ${path.relative(process.cwd(), sb3)}...`);
step(bootFrames);
console.log(`booted in ${((Date.now() - t0) / 1000).toFixed(1)}s: ` +
    `efb_frames=${value('efb_frames')} guest instructions=${value('rv_instructions')}`);

const hold = (key, frames) => {
    runtime.ioDevices.keyboard.postData({ key, isDown: true });
    step(frames);
    runtime.ioDevices.keyboard.postData({ key, isDown: false });
    step(60);
};
/// Two keys at once, which is how Doom is actually walked: forward *and* turn.
const holdAll = (keys, frames) => {
    for (const key of keys) runtime.ioDevices.keyboard.postData({ key, isDown: true });
    step(frames);
    for (const key of keys) runtime.ioDevices.keyboard.postData({ key, isDown: false });
    step(60);
};
const report = (what, a, b) => {
    const d = diff(a, b);
    console.log(`${what.padEnd(22)} ${String(d.all).padStart(6)} of ${a.length} pixels ` +
        `(${(d.all / a.length * 100).toFixed(1)}%): ${d.view} above the status bar, ` +
        `${d.bar} in it; input_buffer ${(value('input_buffer') || []).length}`);
};

let a = cardPixels();
step(windowFrames);
let b = cardPixels();
report('quiet (no key)', a, b);
shot('quiet', b);

for (const [what, keys, tag] of [
    ['up arrow  (forward)', ['ArrowUp'], 'up'],
    ['left arrow (turn)', ['ArrowLeft'], 'left'],
    ['w         (forward)', ['w'], 'w'],
    ['comma     (strafe)', [','], 'comma'],
    ['up + left (walk a corner)', ['ArrowUp', 'ArrowLeft'], 'up-left']
]) {
    a = cardPixels();
    shot(`${tag}-before`, a);
    if (keys.length > 1) holdAll(keys, windowFrames); else hold(keys[0], windowFrames);
    b = cardPixels();
    report(`hold ${what}`, a, b);
    shot(`${tag}-after`, b);
}
