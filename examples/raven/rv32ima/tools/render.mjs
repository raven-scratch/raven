// Draw the stage the way the pen drew it.
//
//     node tools/render.mjs [baremetal|linux|mini] [--frames N] [--frames N]
//
// What `check.mjs` reads is the terminal's own cell buffer, which is what the
// console *means*; this is what the pen *did*, which is a different question
// and the one a picture answers. It loads the built project into a real Scratch
// VM, stands in a renderer that records every `penLine`, steps the runtime for
// a while, and rasterises those lines into `dist/<name>.png`.
//
// `--frames N` renders after N frames instead of at the end, so a boot can be
// looked at while it is happening; repeat it to write several pictures.
//
// `--type "<text>"` types at the keyboard, `--wait "<text>"` steps until the
// screen shows it, and `--shot <tag>` writes `dist/<name>-<tag>.png` where it
// stands. They are a session, and they are in the order they are written:
//
//     node tools/render.mjs mini --wait "/ #" \
//       --type "screenfetch\r" --wait "Memory:" --shot logo
//
// An image whose programs are not started for you is only reached by typing at
// it, and a picture of one is a picture of that. The mark is explicit rather
// than tied to `--wait` because a wait that wrote a file would drop one into
// `dist/` on every run, and `dist/` is meant to hold the pictures that were
// asked for.
//
// The dwell is 6 frames down and 6 up per key and the guest drains a key every
// ~95 frames, so what makes this work is not the dwell but the `--wait` after
// it: the bytes queue in the machine's console input and the guest reads them
// on its own timer.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { recordingRenderer, rasterise, writePng } from './renderer.mjs';

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

// A renderer is attached, and a runtime with a renderer asks the page whether it
// is hidden before it draws. There is no page here, and the answer only has to
// exist: this one says it is hidden, which is also the answer that skips the
// draw.
globalThis.document = { hidden: true };

const args = process.argv.slice(2);
const marks = [];
// The script is the session: step to a frame, type at the keyboard, wait for the
// screen to show something, or write a picture. `--type`/`--wait`/`--shot` are
// per-image and per-session, so they are read in the order they are written.
const script = [];
for (let i = 0; i < args.length; i++) {
    if (args[i] === '--frames') script.push({ kind: 'frames', at: Number(args[++i]) });
    else if (args[i] === '--type') script.push({ kind: 'type', text: args[++i] });
    else if (args[i] === '--wait') script.push({ kind: 'wait', text: args[++i] });
    else if (args[i] === '--shot') script.push({ kind: 'shot', tag: args[++i] });
}
const name = args.find((a) => !a.startsWith('--') && !/^\d+$/.test(a) && a !== '');
const sb3 = path.join(root, 'dist', `rv32${name}.sb3`);
if (!fs.existsSync(sb3)) {
    console.error(`${sb3} is not built: node tools/build.mjs ${name}`);
    process.exit(2);
}

const vmRoot = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : path.resolve(repo, '..', 'scratch-vm');
const { default: VirtualMachine } = await import('file://' + path.join(vmRoot, 'src/virtual-machine.js').replace(/\\/g, '/'));

console.warn = () => {};
const vm = new VirtualMachine();
const renderer = recordingRenderer();
vm.attachRenderer(renderer);
const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
vm.greenFlag();

const terminal = () => vm.runtime.targets.find((t) => t.getName() === 'Terminal');
const cells = () => Object.values(terminal().variables).find((v) => v.name === 'glyphs').value;
const cellVar = (n) => Object.values(terminal().variables).find((v) => v.name === n).value;
const inkOf = (code) => (code > 31 && code < 127 ? String.fromCharCode(code) : code === 0 ? ' ' : '?');

let frame = 0;
const write = (tag) => {
    // The stage the project was built for, which is the size its page is laid
    // out for and therefore the size a picture of it has to be.
    const image = rasterise(renderer.lines, [0, 0, 0], cellVar('stage_w'), cellVar('stage_h'));
    const file = path.join(root, 'dist', `${name}${tag}.png`);
    writePng(file, image);
    let inked = 0;
    for (let i = 0; i < image.pixels.length; i += 3) if (image.pixels[i] > 8) inked++;
    // The page is the terminal's own grid, which is measured from the stage: a
    // reader that assumes 64 by 20 is reading a different project's screen.
    const rows = [];
    const cols = cellVar('cols');
    const lines = cellVar('rows');
    for (let r = 0; r < lines; r++) {
        let line = '';
        for (let c = 0; c < cols; c++) line += inkOf(cells()[c * lines + r]);
        rows.push(line.replace(/\s+$/, ''));
    }
    console.log(`${path.relative(process.cwd(), file)}  frame ${frame}  ` +
        `${renderer.lines.length} pen lines, ${inked} lit pixels; ` +
        `stage ${cellVar('stage_w')}x${cellVar('stage_h')}, grid ${cols}x${lines}, ` +
        `left ${cellVar('left')}, top ${cellVar('top')}`);
    console.log('--- the console, as the terminal means it');
    rows.forEach((row, i) => { if (row) console.log(String(i).padStart(2) + ' |' + row + '|'); });
};

const lines = () => {
    const rows = [];
    const glyphs = cells();
    for (let r = 0; r < 20; r++) {
        let line = '';
        for (let c = 0; c < 64; c++) line += inkOf(glyphs[c * 20 + r]);
        rows.push(line.replace(/\s+$/, ''));
    }
    return rows;
};
const screen = () => lines().join('\n');
// One key, held long enough that the input sprite cannot miss it and released
// long enough that it is a second edge. `check.mjs` and `sniff.mjs` both type
// this way; the 4 frames this used to hold it for is under the guest's own
// ~95 frame drain and lost keys.
const press = (key) => {
    vm.runtime.ioDevices.keyboard.postData({ key, isDown: true });
    for (let i = 0; i < 6; i++) { vm.runtime._step(); frame++; }
    vm.runtime.ioDevices.keyboard.postData({ key, isDown: false });
    for (let i = 0; i < 6; i++) { vm.runtime._step(); frame++; }
};

if (script.length === 0) {
    while (frame < 900) { vm.runtime._step(); frame++; }
    write('');
} else {
    const began = Date.now();
    for (const step of script) {
        if (step.kind === 'frames') {
            while (frame < step.at) { vm.runtime._step(); frame++; }
            write(`-${frame}`);
        } else if (step.kind === 'type') {
            // `\r` is written as two characters on a command line because that
            // is the only way a shell can carry it, so it is read back as the
            // key it means.
            for (const character of step.text.replace(/\\r/g, '\r')) {
                if (character === '\r') press('Enter');
                else press(character);
            }
        } else if (step.kind === 'wait') {
            const until = Date.now() + 240_000;
            while (Date.now() < until && !screen().includes(step.text)) { vm.runtime._step(); frame++; }
            if (!screen().includes(step.text)) {
                // A picture of a frame that is not the one that was asked for is
                // worse than no picture: it is a claim about the project that
                // nothing backs, and it looks exactly like a good one. So a wait
                // that never sees its string refuses the whole run -- nothing is
                // written, not even by a later `--shot` -- and says which string
                // it gave up on and where.
                console.error(`waited for ${JSON.stringify(step.text)}: NOT FOUND at frame ${frame} ` +
                    `(${((Date.now() - began) / 1000).toFixed(0)}s). Refusing to write a picture of ` +
                    `the wrong frame. If you are typing, check the dwell and remember that a ` +
                    `question mark is not the same as a full stop: the guest echoes what you type.`);
                process.exit(1);
            }
            console.log(`waited for ${JSON.stringify(step.text)}: on screen at frame ${frame}`);
        } else {
            write(`-${step.tag}`);
        }
    }
}
