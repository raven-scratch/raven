// Throwaway: type a command at the mini image's prompt and record every byte the
// guest writes back to the console.
//
//     node tools/sniff.mjs "busybox --list" [--frames N]
//
// The console's output lives in run 2 of the stage's `_gheap` arena, so a proxy
// in front of that list sees every byte in the order it was written, which is
// the only honest way to know what an editor actually emits. The stream is then
// split into escape sequences and text: the sequences are counted and printed
// once each, and the applet-looking words in the text are reported, so the
// output stays readable however much the guest prints.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { rasterise, writePng } from './renderer.mjs';

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
const command = args.find((a) => !a.startsWith('--'));
const framesIndex = args.indexOf('--frames');
const after = framesIndex >= 0 ? Number(args[framesIndex + 1]) : 400;
if (!command) {
    console.error('usage: node tools/sniff.mjs "<command>" [--frames N]');
    process.exit(2);
}

function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    const lines = [];
    return {
        lines,
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
        updateDrawablePosition() {},
        updateDrawableDirectionScale() {},
        updateDrawableVisible() {},
        updateDrawableEffect() {},
        setDrawableOrder() {},
        getDrawableOrder() { return 0; },
        // The stage is a build argument, not something the project measures, so
        // nothing here has to know where the fence is.
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; },
        drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; },
        drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; },
        isTouchingDrawables() { return false; },
        penClear() { lines.length = 0; },
        penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) { lines.push({ x0, y0, x1, y1, pen: attrs.diameter }); },
        penPoint(_skin, attrs, x, y) { lines.push({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter }); },
        draw() {}
    };
}

const vmRoot = process.env.SCRATCH_VM_ROOT || path.resolve(repo, '..', 'scratch-vm');
const { default: VirtualMachine } = await import('file://' +
    path.join(vmRoot, 'src/virtual-machine.js').replace(/\\/g, '/'));
const vm = new VirtualMachine();
const renderer = recordingRenderer();
vm.attachRenderer(renderer);
console.error = () => {};
console.warn = () => {};
const sb3 = path.join(root, 'dist', 'rv32mini.sb3');
const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

const runtime = vm.runtime;
const target = (n) => runtime.targets.find((t) => t.getName() === n);
const bind = (t, n) => Object.values(t.variables).find((v) => v.name === n);
const val = (t, n) => { const v = bind(t, n); return v ? v.value : undefined; };

let frame = 0;
const stream = [];
const heapVar = bind(runtime.getTargetForStage(), '_gheap');
const raw = heapVar.value;
heapVar.value = new Proxy(raw, {
    set(t, k, v) {
        if (typeof k === 'string') {
            const i = Number(k);
            if (i >= 6) {
                const base = t[3] - 1;
                const cap = t[5];
                if (cap > 0 && i >= base && i < base + cap) stream.push(v);
            }
        }
        t[k] = v;
        return true;
    }
});

// Row major: reading the cells in list order walks down one column at a time.
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
const began = Date.now();
let booted = 0;
while (Date.now() - began < 200_000 && frame < 30_000) {
    runtime._step();
    frame++;
    if (frame % 20 === 0 && screen().includes('/ #')) { booted = frame; break; }
}
console.log(`boot      ${booted || 'NOT REACHED'} frames, ${((Date.now() - began) / 1000).toFixed(0)} s`);
if (!booted) {
    rows().forEach((row, i) => { if (row) console.log(String(i).padStart(2) + ' |' + row + '|'); });
    process.exit(1);
}

const type = (text) => {
    for (const key of [...text, 'Enter']) {
        runtime.ioDevices.keyboard.postData({ key, isDown: true });
        for (let i = 0; i < 6; i++) { runtime._step(); frame++; }
        runtime.ioDevices.keyboard.postData({ key, isDown: false });
        for (let i = 0; i < 6; i++) { runtime._step(); frame++; }
    }
};
const from = stream.length;
type(command);
let echoed = 0;
while (echoed < after) { runtime._step(); frame++; echoed++; }

// Split the capture: escape sequences once each with a count, and the text.
const bytes = stream.slice(from);
const sequences = new Map();
const text = [];
for (let i = 0; i < bytes.length; i++) {
    const v = bytes[i];
    if (v === 27) {
        let s = 'ESC';
        let j = i + 1;
        if (bytes[j] === 91 || bytes[j] === 40 || bytes[j] === 41 || bytes[j] === 61) {
            s += String.fromCharCode(bytes[j]); j++;
            while (j < bytes.length && (bytes[j] < 64 || bytes[j] > 126)) {
                s += bytes[j] === 63 ? '?' : String.fromCharCode(bytes[j]);
                j++;
            }
            if (j < bytes.length) { s += String.fromCharCode(bytes[j]); j++; }
        } else if (j < bytes.length) {
            s += ' ' + String.fromCharCode(bytes[j]); j++;
        }
        sequences.set(s, (sequences.get(s) || 0) + 1);
        i = j - 1;
        continue;
    }
    if (v >= 32 && v < 127) text.push(String.fromCharCode(v));
    else if (v === 13) text.push('\r');
    else if (v === 10) text.push('\n');
    else text.push(`<${v}>`);
}
console.log(`bytes     ${bytes.length} in ${after} frames, ${sequences.size} distinct escape sequences`);
console.log('--- escape sequences, most sent first');
for (const [s, n] of [...sequences].sort((a, b) => b[1] - a[1]).slice(0, 30)) {
    console.log(`  ${String(n).padStart(5)}  ${JSON.stringify(s)}`);
}
const joined = text.join('');
console.log(`text      ${JSON.stringify(joined.slice(-260))}`);
const words = [...new Set((joined.match(/[a-z][a-z0-9_-]{1,}/g) || []))];
const applets = words.filter((w) => ['vi', 'vim', 'ed', 'nano', 'awk', 'less', 'more', 'sed', 'sh', 'bash'].includes(w));
console.log(`applets   in the text: ${applets.join(' ') || 'none'}`);
console.log('--- screen at the end');
rows().forEach((row, i) => { if (row) console.log(String(i).padStart(2) + ' |' + row + '|'); });

// `--png <name>` writes the stage the pen drew, the same rasteriser
// `tools/render.mjs` uses, so the picture is of the session that was just typed
// rather than of a screenshot tool's idea of it.
const pngIndex = args.indexOf('--png');
if (pngIndex >= 0 && args[pngIndex + 1]) {
    const file = path.join(root, 'dist', `${args[pngIndex + 1]}.png`);
    writePng(file, rasterise(renderer.lines));
    const image = rasterise(renderer.lines);
    let lit = 0;
    for (let i = 0; i < image.pixels.length; i += 3) if (image.pixels[i] > 8) lit++;
    console.log(`png       ${file}  ${renderer.lines.length} pen lines, ${lit} lit pixels`);
}
