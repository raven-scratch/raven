// Run the RISC-V board's project in a real Scratch VM and see whether the
// machine boots, and whether the console reached the graphics card.
//
//     node tools/check-rv32.mjs                     the whole check
//     node tools/check-rv32.mjs --console           print the guest's console
//     node tools/check-rv32.mjs --card              print the card's picture
//     node tools/check-rv32.mjs --budget 600        seconds to give it
//     node tools/check-rv32.mjs --shots 5           write dist/shot-<k>.png every 5 s
//
// `tools/watch-rv32.mjs` is the other end of this: it boots the same project
// and prints only what a boot costs -- seconds, guest instructions, and what
// the pen did in a window at the prompt -- without asserting anything. It is
// the instrument to measure a change with; this is the one to validate it with.
//
// ## What is being checked, and why it is two things
//
// This board has one device that talks and one that shows. The 8250 is the
// guest's `console=ttyS0`, and its byte stream is what the guest actually
// transmitted -- the boot log, the prompt, the echo of what was typed, and
// every program's output. The graphics card is where the kernel's `fbcon`
// writes that console as *pixels*, through the `simple-framebuffer` node the
// device tree gives it, and it is those pixels the monitor scans out onto the
// Stage.
//
// So there are two independent pieces of evidence and neither substitutes for
// the other. The UART says the guest ran, and it is the only place a *word*
// like `Correct operation validated` can be read: the picture is pixels, and
// there is no font table in this project -- not in the example and not in this
// check -- that could turn them back into text. The card says the console
// reached the display the way this machine is supposed to have it reach the
// display, and that is a claim about shape and proportion rather than about
// words: a console is mostly one background colour with a thin minority of ink,
// and the ink is in rows, because the kernel draws it on a font's grid.
//
// `SCRATCH_VM_ROOT` picks the VM. TurboWarp's compiles the blocks to
// JavaScript and is what makes a Linux boot checkable at all; the vanilla VM
// interprets them at a few thousand instructions a second, which is hours.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { encodePng } from './png.mjs';
import { findVmRoot, vmHelp } from '../../../../tools/vm-root.mjs';

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

/// Which VM to run in. `tools/vm-root.mjs` is the one place that answers it for
/// every check in the repository: `SCRATCH_VM_ROOT` when it is set, then the
/// `ref/` checkouts, then a sibling of the repository.
const VM_ROOT = findVmRoot();
if (!VM_ROOT) {
    console.error(vmHelp());
    process.exit(2);
}

const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// A renderer that records what the pen did instead of drawing it. The pen is
/// the monitor's output and nothing else's, so it is the thing being measured.
function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    const lines = [];
    // Every line ever drawn, which `lines` is not: the monitor erases the whole
    // pen layer with `pen clear` on every pass, and `penClear` empties `lines`
    // with it, so the list is what the *last* pass drew. The total is the pen
    // work of the whole run and it is the number an assertion about "the pen
    // drew the card's picture" means.
    let drawn = 0;
    // The Stage itself, kept as the pen draws rather than rasterised from a
    // history at the end: the pen has no erase that takes part of a line, so
    // the picture is the accumulation of every line in order, and keeping it
    // here means the assertion below does not depend on how many lines the run
    // made -- a history that is trimmed is a picture missing its oldest rows.
    const W = 480, H = 360;
    const pixels = new Int32Array(W * H).fill(-1);
    const stamp = (pen, colour, x0, y0, x1, y1) => {
        const c = colour ? (Math.round(colour[0] * 255) << 16) |
            (Math.round(colour[1] * 255) << 8) | Math.round(colour[2] * 255) : 0;
        const rows = Math.max(1, Math.round(pen || 1));
        const r0 = Math.floor(H / 2 - y0 - rows / 2);
        const lo = Math.max(0, Math.floor(Math.min(x0, x1) + W / 2));
        const hi = Math.min(W - 1, Math.floor(Math.max(x0, x1) + W / 2));
        for (let r = r0; r < r0 + rows; r++) {
            if (r < 0 || r >= H) continue;
            for (let x = lo; x <= hi; x++) pixels[r * W + x] = c;
        }
    };
    return {
        lines,
        drawn: () => drawn,
        pixels,
        clears: 0,
        setLayerGrouping() {},
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
        penClear() { this.clears++; lines.length = 0; pixels.fill(-1); },
        penStamp() {},
        // A check that runs for twenty minutes records a lot of lines and every
        // one of them is an object. The *picture* is the raster above and does
        // not need them, so the oldest are dropped in blocks rather than kept:
        // what the list is still for is the pen-line count and the colours.
        trim() {
            if (lines.length > 600000) lines.splice(0, lines.length - 300000);
        },
        penLine(_skin, attrs, x0, y0, x1, y1) {
            drawn += 1;
            lines.push({ x0, y0, x1, y1, pen: attrs.diameter,
                colour: attrs.color4f ? [...attrs.color4f] : null });
            stamp(attrs.diameter, attrs.color4f, x0, y0, x1, y1);
            this.trim();
        },
        penPoint(_skin, attrs, x, y) {
            drawn += 1;
            lines.push({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter,
                colour: attrs.color4f ? [...attrs.color4f] : null });
            stamp(attrs.diameter, attrs.color4f, x, y, x, y);
            this.trim();
        },
        draw() {}
    };
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const STAGE_W = 480;
const STAGE_H = 360;

const args = process.argv.slice(2);
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 900) * 1000;
const showConsole = args.includes('--console');
const showCard = args.includes('--card');
const sb3 = args.find((a) => a.endsWith('.sb3')) || path.join(root, 'dist', 'desktop-rv32-linux.sb3');
const guestArg = args.indexOf('--guest');
const guest = guestArg >= 0 ? args[guestArg + 1]
    : (/doom/.test(path.basename(sb3)) ? 'doom' : 'linux');
/// `--shots N` writes `dist/shot-<k>.png`, the Stage's pen raster, every N
/// seconds of the run. The picture the check asserts on is one instant -- the
/// end -- and an artefact that only exists while the guest is writing, or in
/// the middle of the monitor's own pass, is not in it. This is how a picture
/// that changes over a run is looked at rather than reasoned about.
const shotsArg = args.indexOf('--shots');
const shotsEvery = shotsArg >= 0 ? Number(args[shotsArg + 1]) : 0;
let shotsAt = 0;
let shotsN = 0;

/// The panel this guest's build put on the card, read from the board file the
/// build read it from -- so a check about 480 by 360 `r5g6b5` is a check about
/// what the board says, not about a number written down twice.
async function cardPanel() {
    const board = (await import(pathToFileURL(path.join(root, 'boards', 'mini-rv32.mjs')).href)).default;
    const panel = { ...board.display, ...(board.guests[guest]?.display ?? {}) };
    return { ...panel, ...board.formats[panel.format], indexed: panel.format === 'index8' };
}

const failures = [];
const passes = [];
const check = (ok, what) => (ok ? passes : failures).push(what);

async function main() {
    if (!fs.existsSync(sb3)) {
        console.error(`no project at ${sb3}; run node tools/build.mjs --board boards/mini-rv32.mjs first`);
        process.exit(2);
    }
    const panel = await cardPanel();

    const vm = new VirtualMachine();
    const renderer = recordingRenderer();
    vm.attachRenderer(renderer);

    const originalWarn = console.warn;
    const errors = [];
    console.warn = () => {};
    console.error = (...a) => {
        const line = a.map(String).join(' ');
        if (!/\b(Deprecation|Experimental)?Warning\b/.test(line)) errors.push(line);
    };

    const data = fs.readFileSync(sb3);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

    const runtime = vm.runtime;
    const turbo = !!(runtime.compilerOptions);
    if (turbo) runtime.compilerOptions.enabled = !args.includes('--no-compile');
    const value = (t, name) => {
        const v = Object.values(t.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    /// A stage variable, written. The residue assertion below stops the guest so
    /// that the card cannot move again between the pen's last scan and the
    /// pixels being read, and stopping it is a write to the hart's own state.
    const set = (name, v) => {
        const found = Object.values(stage().variables).find((x) => x.name === name);
        if (found) found.value = v;
    };
    const stage = () => runtime.getTargetForStage();

    /// Everything the guest transmitted, as text. This is the UART's own byte
    /// stream, which is where the guest's *words* are: the boot log, the
    /// prompt, the echo of what was typed, and every program's output.
    const consoleText = () => {
        const bytes = value(stage(), 'console_trace') || [];
        return Buffer.from(bytes.map((b) => Number(b) & 0xff)).toString('latin1');
    };

    /// The card's picture, as the card holds it: one `0xRRGGBB` colour a
    /// pixel.
    ///
    /// The two panels are read from the two places the card keeps them. An
    /// indexed card has a palette and a latched byte a pixel; a direct colour
    /// card has neither, and its pixels are two to a 32 bit word with the even
    /// column in the low half -- which is the card's own layout and not this
    /// check's choice.
    ///
    /// A Scratch list's value is the items themselves, so *item n is at index
    /// n-1*. Reading `words[i + 1]` for word `i` is the next word along, which
    /// shifts the whole picture two columns left and still looks like a console:
    /// that is why every card assertion here passed while the file beside it
    /// showed a card the monitor had never drawn.
    const cardPixels = () => {
        const w = panel.width, h = panel.height;
        const out = new Uint32Array(w * h);
        if (panel.indexed) {
            const pixels = value(stage(), 'efb_scan_pixels') || [];
            const palette = value(stage(), 'efb_scan_palette') || [];
            for (let i = 0; i < w * h; i++) {
                out[i] = Number(palette[Number(pixels[i] || 0)] || 0) >>> 0;
            }
            return out;
        }
        const words = value(stage(), 'efb_words') || [];
        for (let i = 0; i < (w * h) / 2; i++) {
            const word = Number(words[i] || 0);
            for (const half of [0, 1]) {
                const p = half === 0 ? word % 65536 : Math.floor(word / 65536) % 65536;
                out[i * 2 + half] = pack(
                    Math.round((((p >> 11) & 31) * 255) / 31),
                    Math.round((((p >> 5) & 63) * 255) / 63),
                    Math.round(((p & 31) * 255) / 31));
            }
        }
        return out;
    };

    const pack = (r, g, b) => (r << 16) | (g << 8) | b;

    /// The card's picture as eight bits a channel, which is what a PNG wants.
    /// It takes the pixels rather than reading the card, because a check that
    /// compares two pictures has to say *which* picture each file is.
    const cardRgbOf = (px) => {
        const rgb = Buffer.alloc(px.length * 3);
        for (let i = 0; i < px.length; i++) {
            rgb[i * 3] = (px[i] >>> 16) & 0xff;
            rgb[i * 3 + 1] = (px[i] >>> 8) & 0xff;
            rgb[i * 3 + 2] = px[i] & 0xff;
        }
        return rgb;
    };
    const cardRgb = () => cardRgbOf(cardPixels());

    // ---- reading the console back out of the card -------------------------
    //
    // The guest's console is pixels. There is no font table in the example --
    // that is the whole point of the change this check is here for -- so the
    // only way to say what the card is showing is to put the *kernel's* own
    // glyphs back over them. `images/mini-fb-font.bin` is `fontdata_8x16` out
    // of the kernel the guest is running, which `tools/wsl/50-rv32-image.sh`
    // writes beside the image; 4096 bytes, 256 characters of 16 rows of one
    // byte, most significant bit leftmost. The grid is the kernel's: an 8 by
    // 16 cell, origin top left, whole cells only.
    const fontFile = path.join(root, 'images', 'mini-fb-font.bin');
    const font = fs.existsSync(fontFile) ? fs.readFileSync(fontFile) : null;
    const GLYPHS = ' !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~';

    /// What the card is showing, as text, by matching the kernel's own font
    /// against the pixels the monitor scanned out. A pixel is ink when it is
    /// not the panel's most common colour -- a console is one background under
    /// most of its area, and which way round the ink and the paper are is the
    /// console's business and not this check's.
    ///
    /// The grid is the kernel's -- an 8 by 16 cell -- but where it starts is
    /// not: `fbcon` draws its first column a couple of pixels left of the
    /// panel's, and a match that assumed column zero would read noise. So the
    /// offset is *fitted*, by scoring the whole picture against the font at
    /// each of a few offsets and taking the best. An exact fit scores about
    /// one mismatched pixel a cell; a wrong one scores tens.
    const cellBits = (c, r, dx, dy, px, background) => {
        const bits = new Uint8Array(16);
        for (let y = 0; y < 16; y++) {
            let b = 0;
            for (let x = 0; x < 8; x++) {
                const sx = c * 8 + x + dx, sy = r * 16 + y + dy;
                if (sx < 0 || sy < 0 || sx >= panel.width || sy >= panel.height) continue;
                if (px[sy * panel.width + sx] !== background) b |= 1 << (7 - x);
            }
            bits[y] = b;
        }
        return bits;
    };
    const scoreOf = (bits, ch) => {
        const at = ch.charCodeAt(0) * 16;
        let s = 0;
        for (let y = 0; y < 16; y++) { let v = (bits[y] ^ font[at + y]) & 0xff; while (v) { s += v & 1; v >>= 1; } }
        return s;
    };
    let grid = null;

    /// Matching a cell is 96 glyphs of 16 bytes and there are 1320 cells, so
    /// the answer is cached for a quarter of a second: a `settle` that steps
    /// the machine between two reads is asking the same question about the
    /// same picture many times a millisecond. The fitted grid is found once
    /// and kept; a console that moved would be a different console.
    const readCard = () => {
        if (!font) return '';
        const px = cardPixels();
        const counts = new Map();
        for (const p of px) counts.set(p, (counts.get(p) || 0) + 1);
        let background = 0, most = -1;
        for (const [p, n] of counts) if (n > most) { most = n; background = p; }
        const cols = Math.floor(panel.width / 8), rows = Math.floor(panel.height / 16);
        const render = (dx, dy) => {
            const lines = [];
            let total = 0, used = 0;
            for (let r = 0; r < rows; r++) {
                let line = '';
                for (let c = 0; c < cols; c++) {
                    const bits = cellBits(c, r, dx, dy, px, background);
                    let blank = true;
                    for (const b of bits) if (b) { blank = false; break; }
                    if (blank) { line += ' '; continue; }
                    let best = ' ', bestScore = 999;
                    for (const ch of GLYPHS) {
                        const s = scoreOf(bits, ch);
                        if (s < bestScore) { bestScore = s; best = ch; }
                    }
                    total += bestScore; used++;
                    line += bestScore > 24 ? ' ' : best;
                }
                lines.push(line.replace(/\s+$/, ''));
            }
            while (lines.length && lines[lines.length - 1] === '') lines.pop();
            return { text: lines.join('\n'), score: total / Math.max(1, used), used };
        };
        if (!grid) {
            let best = null;
            for (let dx = -4; dx <= 4; dx++) {
                for (let dy = -2; dy <= 2; dy++) {
                    const t = render(dx, dy);
                    if (t.used < 20) continue;
                    if (!best || t.score < best.score) best = { dx, dy, score: t.score };
                }
            }
            if (!best) return '';
            grid = best;
        }
        return render(grid.dx, grid.dy).text;
    };
    let cardTextAt = 0, cardTextValue = '';
    const cardText = () => {
        const now = Date.now();
        if (now - cardTextAt >= 250) { cardTextAt = now; cardTextValue = readCard(); }
        return cardTextValue;
    };

    /// What the card's picture *is*, in numbers a boolean can be made of.
    ///
    /// A console is one background colour under most of the pane with a thin
    /// minority of ink on it, and the ink comes in horizontal bands because
    /// the kernel draws glyphs on a font's grid. Both are properties a
    /// checkerboard, a blank panel or a torn frame do not have.
    const cardShape = () => {
        const px = cardPixels();
        const counts = new Map();
        for (const p of px) counts.set(p, (counts.get(p) || 0) + 1);
        let background = 0, most = -1;
        for (const [p, n] of counts) if (n > most) { most = n; background = p; }
        const ink = px.length - most;
        const rows = [];
        for (let y = 0; y < panel.height; y++) {
            let lit = 0;
            for (let x = 0; x < panel.width; x++) if (px[y * panel.width + x] !== background) lit++;
            rows.push(lit);
        }
        let bands = 0;
        for (let y = 0; y < rows.length; y++) {
            if (rows[y] > 0 && (y === 0 || rows[y - 1] === 0)) bands++;
        }
        return { px, background, backgroundShare: most / px.length, inkShare: ink / px.length, bands, rows };
    };

    const steps = { n: 0 };
    let started = Date.now();
    const step = (n = 1) => {
        for (let i = 0; i < n; i++) { runtime._step(); steps.n++; }
        maybeShot();
    };
    const settle = (test, seconds) => {
        const until = Date.now() + seconds * 1000;
        while (Date.now() < until && !test()) { runtime._step(); steps.n++; }
        return test();
    };
    /// A key pressed the way a *hand* presses it: down, one runtime step, up,
    /// and one more step. The frame in between is the whole of the tap, and it
    /// is what the mechanism needs, because only a key with a hat of its own
    /// carries its identity in the hat's field. The thirty-two keys that have
    /// no hat are found by `when any key pressed` and then `key pressed?`, and
    /// that question is about the *state*: a tap that begins and ends between
    /// two frames leaves nothing for it to ask about. How long a tap has to be
    /// is measured by `measureKeyboard` below rather than assumed.
    const press = (key) => {
        runtime.ioDevices.keyboard.postData({ key, isDown: true });
        step(1);
        runtime.ioDevices.keyboard.postData({ key, isDown: false });
        step(1);
    };
    /// The keyboard's own queue, which is what the project made of the keys.
    const keystrokes = () => (value(stage(), 'input_buffer') || []).map(Number);
    /// Wait for the guest to read what has been typed. A person types at the
    /// speed the machine keeps up with, and the queue holds `INPUT_LIMIT` -- 32
    /// -- bytes and drops what arrives past that, so a check that outran the
    /// guest would lose characters to a full queue and report it as a bad
    /// keyboard.
    const waitForGuest = (frames = 600) => {
        for (let i = 0; i < frames && keystrokes().length > 0; i++) step(1);
        return keystrokes().length;
    };
    /// One character, as the key a person's hand makes of it. Every character a
    /// command line uses is a key on the reader's own keyboard: the letters,
    /// the digits and space have a hat each, `when enter pressed` is the one
    /// that submits a line, and the punctuation is found by the `any` hat and
    /// `key pressed?`. Nothing here is the pointer and nothing here is a
    /// picture of a key.
    const typeChar = (c) => { press(c); waitForGuest(); };
    const type = (text) => {
        for (const c of text) typeChar(c);
        press('Enter');
        waitForGuest();
    };

    /// Everything the guest said after the last line matching `marker`, which
    /// is how a check asks about *this* command's output rather than about
    /// something earlier in the boot log that happens to contain the same
    /// word.
    const textAfter = (text, marker) => {
        const at = text.lastIndexOf(marker);
        return at < 0 ? '' : text.slice(at);
    };
    const lastLines = (text, n) =>
        text.replace(/\r/g, '').split('\n').filter((l) => l.trim()).slice(-n).join('\n');

    let viSaid = '(vi was not tried)';
    /// Coremark's own last lines and what the machine retired to produce them.
    let coremarkSaid = '(coremark was not tried)';
    let coremark = { instructions: 0, seconds: 0 };
    /// How many whole-panel repaints the monitor issued in the forty frames
    /// between the guest being stopped and the picture being read. Zero would
    /// mean the Stage was last written before the guest's last store.
    let passesAfterStop = 0;

    /// The Stage, as the pen left it: the raster the renderer kept as it drew,
    /// which is the accumulation of every run the monitor issued because the pen
    /// has no erase. A pixel no run ever covered is the backdrop, and the
    /// backdrop is the panel's own black, so it is a zero here too.
    const stageRgb = () => {
        const rgb = Buffer.alloc(STAGE_W * STAGE_H * 3);
        for (let i = 0; i < STAGE_W * STAGE_H; i++) {
            const c = renderer.pixels[i] < 0 ? 0 : renderer.pixels[i];
            rgb[i * 3] = (c >>> 16) & 0xff;
            rgb[i * 3 + 1] = (c >>> 8) & 0xff;
            rgb[i * 3 + 2] = c & 0xff;
        }
        return rgb;
    };

    /// One sample of the Stage, if `--shots` asked for them.
    const maybeShot = () => {
        if (!shotsEvery) return;
        const now = Date.now();
        if (now - shotsAt < shotsEvery * 1000) return;
        shotsAt = now;
        const file = path.join(root, 'dist', `shot-${shotsN++}.png`);
        fs.writeFileSync(file, encodePng(STAGE_W, STAGE_H, stageRgb()));
        originalWarn(`shot          ${path.relative(process.cwd(), file)} at ` +
            `${((now - started) / 1000).toFixed(1)} s`);
    };

    vm.greenFlag();
    runtime.currentStepTime = 1000 / 30;

    originalWarn(`file          ${path.relative(process.cwd(), sb3)}`);
    originalWarn(`vm            ${path.relative(repo, VM_ROOT)}` +
        `${turbo && !args.includes('--no-compile') ? ' (compiling blocks to JavaScript)' : ' (interpreting blocks)'}`);
    originalWarn(`card          ${panel.width}x${panel.height} ${panel.format}` +
        `${panel.indexed ? ' (indexed, latched)' : ' (direct colour, scanned live)'}`);

    // Wait for the kernel to hand over to `/init`, which is the last thing the
    // serial port has to say. What the *shell* says is on the card, so that is
    // waited for in pixels below rather than in bytes here. The two guests
    // arrive at two different places: the mini image goes straight to a root
    // shell because its `/init` *is* the shell, and the Doom image is one
    // program with no shell at all.
    const guestIsDoom = guest === 'doom';
    const consoleNow = () => consoleText().replace(/\r/g, '');

    // ---- the keyboard, key by key, measured through the built project -----
    //
    // Every row below is a real key posted into the VM's own keyboard device
    // and the bytes the *built project* made of it, read out of the queue the
    // guest reads. It is run with the guest stopped, so nothing drains the
    // queue between the key and the reading, and what is in it is exactly what
    // the keyboard put there.
    //
    // The expected bytes are the two keyboards' own: a console wants a
    // terminal's byte stream, so an up arrow is `ESC [ A` (27, 91, 65) and
    // Enter is 13 -- a carriage return, which the kernel's line discipline maps
    // to a newline -- while the card wants its own codes, so an up arrow is
    // 128 and Enter is 136. On the card the arrows and `w`, `a`, `s`, `d` are
    // reported from the keyboard's *state* while they are held -- a game wants
    // a key that stays down -- so the second column is empty for them: a state
    // needs the key to be down while a frame runs, and a tap with no frame in it
    // has nothing to read.
    //
    // Every other key has a hat, which is the point of the two columns. A hat
    // is matched on its own `KEY_OPTION` field, so the key is in the thread and
    // both columns are the same: a tap that begins and ends between two frames
    // delivers. The thirty-two printable ASCII keys are here in full for exactly
    // that reason -- the editor's dropdown has no item for them, and this is the
    // measurement that they arrive all the same.
    //
    // The last six rows used to be one limit and are now two. Backspace, delete
    // and escape are keys TurboWarp names and vanilla drops before any block
    // sees them: its `postData` returns before it emits or records anything for
    // a name longer than one character that is not one of its own six
    // (`ref/scratch-vm/node_modules/scratch-vm/src/io/keyboard.js:47-67`,
    // `:115-118`) -- and TurboWarp's own switch (`ref/turbowarp-vm/src/io/
    // keyboard.js:14-27`, `:81-93`) is what gives them a name. So which bytes
    // these three make is a property of the *runtime*, and the expectations say
    // so: `ext` is the byte where the extended runtime is running and nothing
    // where a vanilla one is. Shift and control are gates and not bytes, so they
    // make nothing either way; what they do is measured below. Tab is still
    // nothing at all: neither runtime has a name for it.
    //
    // One row is `[key, one frame of key-down, no frame at all]`.
    const SYMBOLS = '!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~';
    const symbolRows = [...SYMBOLS].map((c) => [c, [c.charCodeAt(0)], [c.charCodeAt(0)]]);
    const ext = (bytes) => (turbo ? bytes : []);
    const keyRows = guestIsDoom
        ? [
            ['a', [130], []], ['d', [131], []], ['s', [129], []], ['w', [128], []],
            ['z', [122], [122]], ['0', [48], [48]], [' ', [32], [32]], ['Enter', [136], [136]],
            ['ArrowUp', [128], []], ['ArrowDown', [129], []],
            ['ArrowLeft', [130], []], ['ArrowRight', [131], []],
            ...symbolRows,
            ['Backspace', ext([127]), ext([127])],
            ['Escape', ext([135]), ext([135])],
            ['Delete', [], []],
            ['Shift', ext([133]), ext([133]), 'the card\'s run'],
            ['Control', ext([132]), ext([132]), 'the card\'s fire'],
            ['Tab', [], []]
        ]
        : [
            ['a', [97], [97]], ['z', [122], [122]], ['0', [48], [48]],
            [' ', [32], [32]], ['Enter', [13], [13]],
            ['ArrowUp', [27, 91, 65], [27, 91, 65]], ['ArrowDown', [27, 91, 66], [27, 91, 66]],
            ['ArrowLeft', [27, 91, 68], [27, 91, 68]], ['ArrowRight', [27, 91, 67], [27, 91, 67]],
            ...symbolRows,
            ['Backspace', ext([127]), ext([127])],
            ['Delete', ext([27, 91, 51, 126]), ext([27, 91, 51, 126])],
            ['Escape', ext([27]), ext([27])],
            ['Tab', [], []],
            ['Shift', [], [], 'a gate, not a byte: it makes the next letter a capital'],
            ['Control', [], [], 'a gate, not a byte: it makes the next letter a control character']
        ];
    /// Empty the keyboard's own queue, so that one key's report is one report.
    /// The guest is stopped for this, so nothing else is taking bytes out.
    const clearQueue = () => {
        const v = Object.values(stage().variables).find((x) => x.name === 'input_buffer');
        if (v && Array.isArray(v.value)) v.value.length = 0;
    };
    /// A key pressed and released between two frames: down and up with no
    /// runtime step in between, so the key is never down while any block runs.
    const tap = (key) => {
        runtime.ioDevices.keyboard.postData({ key, isDown: true });
        runtime.ioDevices.keyboard.postData({ key, isDown: false });
        step(1);
    };
    const same = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
    const measureKeyboard = () => {
        const rows = [];
        for (const [key, one, none, note] of keyRows) {
            clearQueue();
            press(key);
            const oneGot = keystrokes();
            clearQueue();
            tap(key);
            const noneGot = keystrokes();
            rows.push({ key, one, none, note, oneGot, noneGot });
        }
        originalWarn('keys          the built project, one real keypress per row ' +
            '(a frame of key-down, then no frame at all):');
        for (const row of rows) {
            originalWarn(`  ${(JSON.stringify(row.key) + '            ').slice(0, 13)}` +
                `${JSON.stringify(row.oneGot).padEnd(14)}${JSON.stringify(row.noneGot).padEnd(8)}` +
                `${row.one.length === 0
                    ? (row.note ?? 'nothing: this runtime drops the key before any block')
                    : (same(row.oneGot, row.one) && same(row.noneGot, row.none) ? 'ok' :
                        `EXPECTED ${JSON.stringify(row.one)} / ${JSON.stringify(row.none)}`)}`);
        }
        const reachable = rows.filter((r) => r.one.length > 0);
        const unreachable = rows.filter((r) => r.one.length === 0);
        // The keys in the table that are neither a letter, a digit, space, enter,
        // an arrow nor one of the symbols: the ones a runtime has to *name*.
        const base = new Set([' ', 'Enter', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', ...SYMBOLS]);
        for (let i = 0; i < 26; i++) base.add(String.fromCharCode(97 + i));
        for (let i = 0; i <= 9; i++) base.add(String(i));
        const named = reachable.map((r) => r.key).filter((k) => !base.has(k));
        check(reachable.every((r) => same(r.oneGot, r.one) && same(r.noneGot, r.none)),
            `every key a real keyboard can reach arrived as its own byte, frame or no frame ` +
            `(${reachable.length} keys: letters, digits, space, enter, the arrows, all ` +
            `thirty-two printable symbols` +
            `${named.length ? ', ' + named.join(', ') : ''})`);
        check(unreachable.every((r) => r.oneGot.length === 0 && r.noneGot.length === 0),
            `and the rest made no byte of their own, with or without a frame ` +
            `(${unreachable.map((r) => r.key).join(', ')})`);
        // Which keys still need the key to be down while a frame runs, which is
        // the one thing a tap shorter than a frame costs. Nothing on the console
        // board does any more -- every key there is a hat -- and on the card it
        // is the movement keys alone, because those are the poll's.
        const noFrame = rows.filter((r) => r.one.length > 0 && r.none.length === 0)
            .map((r) => JSON.stringify(r.key));
        originalWarn(`keys          ${noFrame.length} of the ${reachable.length} reachable keys ` +
            `need the key down while a frame runs: ${noFrame.join(', ') || '(none)'}`);
        // Shift and control are gates and not bytes, so what they do is a letter
        // *with* one held. On the extended runtime they make the capital and the
        // control character; on vanilla the modifier never reaches any block, so
        // the letter arrives plain -- and, the trap this arrangement is for,
        // nothing asks a question about the letter `S` or `C` on the way.
        if (!guestIsDoom) {
            const withModifier = (mod, key) => {
                clearQueue();
                runtime.ioDevices.keyboard.postData({ key: mod, isDown: true });
                step(1);
                press(key);
                const got = keystrokes();
                runtime.ioDevices.keyboard.postData({ key: mod, isDown: false });
                step(1);
                return got;
            };
            const shifted = withModifier('Shift', 'a');
            const ctrled = withModifier('Control', 'c');
            const wantShift = turbo ? [65] : [97];
            const wantCtrl = turbo ? [3] : [99];
            originalWarn(`keys          shift + "a" -> ${JSON.stringify(shifted)}, ` +
                `control + "c" -> ${JSON.stringify(ctrled)} ` +
                `(${turbo ? 'TurboWarp gives a capital and ETX' : 'vanilla drops both modifiers'}: ` +
                `${JSON.stringify(wantShift)} and ${JSON.stringify(wantCtrl)})`);
            check(same(shifted, wantShift) && same(ctrled, wantCtrl),
                `a modifier a hat can see made the letter it modifies ` +
                `(shift+a ${JSON.stringify(shifted)}, control+c ${JSON.stringify(ctrled)})`);
        }
    };

    let prompted = false;
    const bootUntil = Date.now() + budgetMs;
    while (Date.now() < bootUntil && !prompted) {
        runtime._step();
        steps.n++;
        maybeShot();
        if (guestIsDoom) {
            if (steps.n % 100 === 0) prompted = Number(value(stage(), 'efb_frames')) > 0;
        } else if (steps.n % 300 === 0) {
            prompted = /Run \/init as init process/.test(consoleNow());
        }
    }
    if (!guestIsDoom) prompted = /Run \/init as init process/.test(consoleText());
    if (!guestIsDoom && prompted) {
        // The shell's prompt is pixels on the card, so the wait for it is a
        // wait for the card to say something rather than for the port to.
        settle(() => /[#$]/.test(cardText()), Math.min(budgetMs / 1000, 180));
        check(/[#$]/.test(cardText()),
            `the shell prompt reached the card's framebuffer and reads back out of it ` +
            `(${JSON.stringify(cardText().split('\n').filter((l) => l.trim()).slice(-1)[0] || '')})`);
    }

    const bootSeconds = (Date.now() - started) / 1000;
    originalWarn(`boot          ${steps.n} frames in ${bootSeconds.toFixed(1)} s, ` +
        `${value(stage(), 'rv_instructions')} guest instructions`);
    originalWarn(`console       ${consoleText().length} bytes`);
    originalWarn(`card          ${value(stage(), 'efb_writes')} pixel stores, ` +
        `${value(stage(), 'efb_frames')} frames latched, ack ${value(stage(), 'efb_ack')}`);

    if (guestIsDoom) {
        // Let the guest run on while the monitor draws what it has latched, so
        // the pen counters and the picture are about a picture the card
        // actually produced rather than about the first latch.
        settle(() => Number(value(stage(), 'monitor_reads')) >= panel.width * panel.height,
            Math.min(budgetMs / 1000, 300));

        // ---- input: a key pressed on the Stage has to reach the game --------
        //
        // The card's keyboard is two registers, and reading them back only says
        // the *host* queued something. The check above passed for a round while
        // the game was unplayable for exactly that reason: the guest's driver
        // polled a card whose registers were implemented and whose queue was
        // never filled with a byte Doom could use. So this does not read a
        // register. It presses the forward key and looks at the picture, because
        // E1M1 is rendered from the player's own position and a frame in which
        // the player walked forward is a picture drawn from somewhere else.
        //
        // The control is a window of the *same length* with no key pressed at
        // all. This guest redraws its status bar and its own face whether or not
        // anyone is playing, so what a key did is only a measurement next to
        // what the game did on its own.
        const pixelDiff = (a, b) => {
            const barRows = 32; // Doom's status bar: the bottom 32 rows of 200
            const viewPixels = panel.width * (panel.height - barRows);
            let all = 0, view = 0;
            for (let i = 0; i < a.length; i++) {
                if (a[i] !== b[i]) { all++; if (i < viewPixels) view++; }
            }
            return { all, view, bar: all - view };
        };
        // The guest renders E1M1 from about its ninetieth frame, one frame
        // every few of the machine's, and what it needs before a key means
        // anything is *time in the level* rather than a wall clock. So the wait
        // is for the card to have latched two hundred frames of the level,
        // which is a state a player would recognise: standing still in the
        // starting room with the game running.
        settle(() => Number(value(stage(), 'efb_frames')) >= 200,
            Math.min(budgetMs / 1000, 240));
        // The guest lags the machine, so the picture is allowed to stop moving
        // before it is asked what a key does to it.
        let still = cardPixels();
        let waited = 0;
        for (let i = 0; i < 20; i++) {
            step(20); waited += 20;
            const now = cardPixels();
            if (pixelDiff(still, now).all === 0) break;
            still = now;
        }

        const window = 120;
        const quietBefore = cardPixels();
        step(window);
        const quietAfter = cardPixels();
        const noise = pixelDiff(quietBefore, quietAfter).all;

        const before = cardPixels();
        fs.writeFileSync(path.join(root, 'dist', 'card-doom-before.png'),
            encodePng(panel.width, panel.height, cardRgbOf(before)));
        // The forward key, held down: Doom's `key_up` is the up arrow and a
        // held key is reported again as soon as the guest has read the last
        // report, so this is a player walking rather than a single step.
        runtime.ioDevices.keyboard.postData({ key: 'ArrowUp', isDown: true });
        step(window);
        runtime.ioDevices.keyboard.postData({ key: 'ArrowUp', isDown: false });
        step(window);
        const after = cardPixels();
        fs.writeFileSync(path.join(root, 'dist', 'card-doom-after.png'),
            encodePng(panel.width, panel.height, cardRgbOf(after)));

        const moved = pixelDiff(before, after);
        const area = panel.width * panel.height;
        originalWarn(`input         ${waited} frames to settle, then ${moved.all} of ${area} ` +
            `pixels changed holding the up arrow; a ${window}-frame window with no key ` +
            `changed ${noise}; ${moved.view} of the change is the view and ${moved.bar} ` +
            `is the status bar; input_buffer ${(value(stage(), 'input_buffer') || []).length} left`);
        check(moved.all > noise * 8 && moved.all > area * 0.02,
            `holding the forward key moved the picture: ${moved.all} pixels of ${area} ` +
            `changed (${(moved.all / area * 100).toFixed(1)}%), against ${noise} in a ` +
            `window of the same length with no key`);
        check(moved.view > moved.bar * 4,
            `and it is the level that moved, not the status bar: ${moved.view} pixels ` +
            `above the status bar against ${moved.bar} in it`);
        check((value(stage(), 'input_buffer') || []).length === 0,
            'the guest read the queue dry, so the key was consumed exactly once');
        originalWarn('pictures      dist/card-doom-before.png and dist/card-doom-after.png ' +
            '(the card before and after the key)');

        // The picture was the question the card's keyboard had to answer. Now
        // that the game is not the thing being watched any more, the guest is
        // stopped and the keyboard is asked what it can reach at all.
        set('rv_state', 0);
        step(40);
        measureKeyboard();
    } else {
        check(prompted, 'it booted to a root shell with no login');
        check(/Linux version/.test(consoleText()), 'the kernel booted');
        check(/Run \/init as init process/.test(consoleText()), 'the initramfs ran /init');
    }

    // ---- the programs, typed at the prompt --------------------------------
    //
    // The order is the order a person would type them in, and each one is a
    // real command typed at a real prompt: nothing here is a variable the
    // machine also reads. What is read is the *card*: the shell's output is on
    // the framebuffer console, so "coremark validated its results" is a
    // sentence this check finds in the pixels the guest wrote, matched against
    // the kernel's own font. That is the claim the example is making -- the
    // console is on the card and nowhere else -- and it is the only way to
    // test it that is about the display rather than about the emulator.
    let cardBefore = null;
    if (prompted && !guestIsDoom) {
        cardBefore = cardShape();
        const onCard = (pattern, seconds) => settle(() => pattern.test(cardText()), seconds);

        const coreBefore = Number(value(stage(), 'rv_instructions'));
        const coreAt = Date.now();
        type('coremark');
        check(onCard(/Correct operation validated/, 300),
            'coremark ran and validated its results, read back out of the card');
        // What coremark itself said, and what the machine retired to make it
        // say it. The two together are the only honest way to read its score:
        // the score is `iterations / the guest's own seconds`, and the guest's
        // seconds come from the CLINT, so a clock that is wrong is a score that
        // is wrong by exactly that factor and nothing else.
        coremarkSaid = cardText().trim();
        coremark = {
            instructions: Number(value(stage(), 'rv_instructions')) - coreBefore,
            seconds: (Date.now() - coreAt) / 1000
        };
        originalWarn(`coremark      ${coremark.instructions} guest instructions retired, ` +
            `${coremark.seconds.toFixed(1)} s of host; the card says:\n${coremarkSaid}`);

        type("duktape -e 'console.log(12345)'");
        check(onCard(/12345/, 180),
            'duktape evaluated a program typed at the prompt');

        type('duktape /root/fizzbuzz.js');
        check(onCard(/FizzBuzz|Fizz/, 180),
            'duktape ran a script file from the guest');

        // `ed` for real, and by the owner's own sequence: `ed test.js`, then
        // append, a line of text, the `.` that ends the append, `w` to write and
        // `q` to quit, then read the file back with something that is not `ed`.
        // A shell with an editor is the thing the guest was built for, and an
        // editor that only starts is not evidence that it edits. The `.` is the
        // whole test: it has no hat of its own -- a hat's key is a field and the
        // field's values are the dropdown -- so the only thing that can deliver
        // it is the `any` hat and `key pressed?`, which is to say a real `.` on
        // the reader's own keyboard. A run that reads the text back has pressed
        // one.
        type('ed test.js');
        settle(() => false, 5);
        type('a');
        type('written-by-ed');
        type('.');
        type('w');
        type('q');
        type('cat test.js');
        check(onCard(/written-by-ed/, 180),
            'ed received a real `.` keypress, wrote test.js and the shell read it back');

        // `vi` is in this busybox as a link to the `ed`-family applet, and the
        // image was built with the `vi` entry spliced out; whichever of those
        // is true is what the check reports rather than asserts.
        type('vi --help');
        settle(() => false, 8);
        viSaid = lastLines(cardText(), 3);

        // Stop the guest, so the picture the pen drew and the picture the card
        // holds are two readings of the same instant rather than of two moments,
        // and give the monitor the passes it needs to repaint whatever the
        // guest's last write changed. Everything below -- the two files, the
        // card assertions and the residue assertion -- is then about one frozen
        // machine.
        //
        // **The card is marked dirty first, on purpose.** The monitor draws only
        // when `efb_dirty` is set (see `src/rvmonitor/efb.rav`), so a stopped
        // guest on a card nothing has touched since the last pass would
        // correctly produce no pass at all -- and this assertion is about
        // whether a pass *can* bring the Stage up to date, which needs one to
        // happen. Setting the flag is the test asking the monitor to draw, not
        // the monitor being made to draw when it should not: with the guest
        // stopped nothing else can set it, so a pass here is a repaint of the
        // frozen card and nothing else.
        set('rv_state', 0);
        const passesBeforeStop = Number(value(stage(), 'monitor_frames'));
        set('efb_dirty', 1);
        step(40);
        passesAfterStop = Number(value(stage(), 'monitor_frames')) - passesBeforeStop;

        // The guest is stopped and the card is frozen, so the keyboard can be
        // asked what it can and cannot reach without any of it moving.
        measureKeyboard();
    }

    const text = consoleText();

    // The two pictures, as files. A boolean says the console reached the card;
    // only a file says what the console *looks* like, and neither this machine
    // nor a reader can be asked to wait for a browser to draw it.
    const tag = guestIsDoom ? 'doom' : 'linux';
    const stageFile = path.join(root, 'dist', `screen-${tag}.png`);
    const cardFile = path.join(root, 'dist', `card-${tag}.png`);
    // The Stage's picture is the raster the renderer kept, not one rebuilt from
    // the line list: the pen has no erase, so the raster *is* the accumulation,
    // and a history that has been trimmed is a picture missing its oldest rows.
    const draws = renderer.pixels;
    if (args.includes('--no-png')) {
        originalWarn(`pictures      (--no-png)`);
    } else {
        fs.writeFileSync(stageFile, encodePng(STAGE_W, STAGE_H, stageRgb()));
        fs.writeFileSync(cardFile, encodePng(panel.width, panel.height, cardRgb()));
        // And what the card *says*, as text, for a reader who would rather read
        // it than look at it. It is the same match the assertions make.
        fs.writeFileSync(path.join(root, 'dist', `card-${tag}.txt`), cardText() + '\n');
        originalWarn(`pictures      dist/screen-${tag}.png (the Stage's pen), ` +
            `dist/card-${tag}.png (the card's own pixels), dist/card-${tag}.txt (what it says)`);
    }

    originalWarn(`frames        ${steps.n} runtime steps in ${((Date.now() - started) / 1000).toFixed(1)} s`);
    originalWarn(`pen           ${renderer.drawn()} lines drawn over the whole run ` +
        `(${renderer.lines.length} in the last pass, which is what \`pen clear\` leaves)`);
    originalWarn(`monitor       ${value(stage(), 'monitor_frames')} frames, ` +
        `${value(stage(), 'monitor_reads')} pixels read, ` +
        `${value(stage(), 'monitor_runs')} runs drawn`);
    originalWarn(`vi            ${viSaid}`);

    // The card's own colours, out of the card's own memory: what the guest
    // actually wrote, as 16 bit `r5g6b5` words, beside what the pen drew from
    // them. A console's text colour is a claim about *this* and not about a
    // screenshot, so it is read here rather than inferred from the picture.
    if (!guestIsDoom) {
        const counts = new Map();
        for (const word of (value(stage(), 'efb_words') || [])) {
            const n = Number(word) >>> 0;
            if (n === 0) continue;
            // A word holds two pixels: the low half is the even column. Both are
            // counted, because the row's picture is pixels and not words.
            counts.set(n % 65536, (counts.get(n % 65536) || 0) + 1);
            counts.set(Math.floor(n / 65536) % 65536, (counts.get(Math.floor(n / 65536) % 65536) || 0) + 1);
        }
        counts.delete(0);
        const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3);
        // The expansion the *monitor* applies -- `efb_r5`/`efb_g6` in
        // `src/rvmonitor/efb.rav` -- which is a 5 or 6 bit channel scaled to
        // eight bits by the display's own rounding, not by a shift.
        const wide = (v, bits) => Math.round(v * 255 / ((1 << bits) - 1));
        originalWarn(`card colours  ${top.map(([w, n]) =>
            `0x${w.toString(16).toUpperCase().padStart(4, '0')} (r${(w >> 11) & 31} g${(w >> 5) & 63} b${w & 31}) x${n}`)
            .join(', ')}`);
        originalWarn(`pen colours   ${top.map(([w]) =>
            `#${[wide((w >> 11) & 31, 5), wide((w >> 5) & 63, 6), wide(w & 31, 5)]
                .map((v) => v.toString(16).toUpperCase().padStart(2, '0')).join('')}`).join(', ')}`);
    }

    // ---- the console reached the card, and the card reached the Stage ------
    //
    // This is the assertion the whole example is about, and it is the same one
    // for both panels even though the two panels carry it there differently:
    // the card holds a picture, the monitor read all of it, and the pen drew
    // it. What is *not* asserted is that the picture is legible text -- there
    // is no font table in this project to ask, and the pictures beside this
    // check are what a reader looks at instead.
    if (guestIsDoom) {
        // The bare metal guest's own handshake: it commits a frame and waits
        // for the acknowledgement, so a sequence with a matching ack is the
        // guest's driver completing.
        const frames = Number(value(stage(), 'efb_frames'));
        const ack = Number(value(stage(), 'efb_ack'));
        const seq = Number(value(stage(), 'efb_seq'));
        check(frames > 0, `the card latched ${frames} frames from the guest`);
        check(ack > 0 && ack === seq,
            `the guest's frame sequence was acknowledged (seq ${seq}, ack ${ack})`);
        check(Number(value(stage(), 'monitor_reads')) >= panel.width * panel.height,
            `the monitor read a whole ${panel.width}x${panel.height} frame ` +
            `(${value(stage(), 'monitor_reads')} pixels)`);
        check(Number(value(stage(), 'monitor_runs')) > 1000,
            `the monitor scanned the card out as runs (${value(stage(), 'monitor_runs')} runs)`);
    }

    if (!guestIsDoom) {
        // 1. The guest's console wrote into the card's own memory.
        const writes = Number(value(stage(), 'efb_writes'));
        check(writes > 0, `the guest's framebuffer console wrote ${writes} pixels into the card`);
        check(Number(value(stage(), 'monitor_reads')) >= panel.width * panel.height,
            `the monitor scanned the card out (${value(stage(), 'monitor_reads')} pixels read)`);
        check(Number(value(stage(), 'monitor_runs')) > 1000,
            `the monitor drew that scanout as runs (${value(stage(), 'monitor_runs')} runs)`);
        check(renderer.drawn() > 1000,
            `the pen drew the card's picture (${renderer.drawn()} pen lines)`);

        // 2. What the card holds is a console: a background, a thin minority
        //    of ink, and that ink in rows.
        const shape = cardShape();
        check(shape.backgroundShare > 0.5,
            `the card's picture is mostly one background colour ` +
            `(${(shape.backgroundShare * 100).toFixed(1)}% of the panel)`);
        check(shape.inkShare > 0.0005 && shape.inkShare < 0.5,
            `and a minority of it is ink (${(shape.inkShare * 100).toFixed(2)}%)`);
        check(shape.bands >= 8 && shape.bands <= 40,
            `the ink is in ${shape.bands} horizontal bands, which is rows of text`);

        // 3. The console moved the picture. A card that held the boot banner
        //    and nothing else would pass everything above; a card that the
        //    shell is writing to changes when a program runs.
        if (cardBefore) {
            let changed = 0;
            for (let i = 0; i < shape.px.length; i++) if (shape.px[i] !== cardBefore.px[i]) changed++;
            check(changed > shape.px.length * 0.01,
                `typing three programs scrolled the card by ${(changed / shape.px.length * 100).toFixed(1)}% of it`);
        }

        // 4. The invariant the whole display rests on: *after the monitor has
        //    drawn, every pixel of the Stage's panel region is the card's.*
        //    Every pass erases the pen layer with `pen clear` and repaints the
        //    whole panel from the card, so there is no bookkeeping left to be
        //    wrong about what was left alone: the only question is whether a
        //    pass ran after the guest's last write. The guest was stopped above
        //    and the monitor has had its forty passes, so the card cannot have
        //    moved between the last scan and the pixels being read.
        //
        //    The tolerance is zero columns: a row is drawn from its first pixel
        //    to its last, so the pen's round cap bleeds half a pixel into the
        //    *next row's* own pixels, which that row's own runs paint in the
        //    same pass, and no column of a drawn row is left holding an older
        //    picture. What this does not model is a browser's antialiasing of
        //    that cap, which is why dist/screen-*.png is written beside it for
        //    a reader to look at.
        check(passesAfterStop > 0,
            `the monitor repainted the whole panel after the guest stopped ` +
            `(${passesAfterStop} pass(es) in the 40 frames of grace)`);
        {
            const stageShot = stageRgb();
            const cardShot = cardRgb();
            let residue = 0;
            const rows = new Set();
            for (let i = 0; i < panel.width * panel.height; i++) {
                const at = i * 3;
                if (stageShot[at] === cardShot[at] && stageShot[at + 1] === cardShot[at + 1] &&
                    stageShot[at + 2] === cardShot[at + 2]) continue;
                residue++;
                rows.add(Math.floor(i / panel.width));
            }
            check(residue === 0,
                `the Stage is the card's picture pixel for pixel after the guest stops ` +
                `(${residue} of ${panel.width * panel.height} differ` +
                `${residue ? `, in ${rows.size} rows` : ''})`);
        }
    }

    if (showConsole) {
        originalWarn('--- console ---');
        originalWarn(text.replace(/\r/g, ''));
        originalWarn('--- end console ---');
    }
    if (showCard) {        const shape = cardShape();
        originalWarn(`--- card ${panel.width}x${panel.height}, background ${shape.background}, ` +
            `${shape.bands} bands of ink ---`);
        originalWarn(cardText() || '(no font: images/mini-fb-font.bin is not there)');
        originalWarn('--- end card ---');
    }
    if (errors.length > 0) check(false, `${errors.length} runtime error(s): ${errors[0]}`);

    for (const what of passes) originalWarn(`ok   ${what}`);
    for (const what of failures) originalWarn(`FAIL ${what}`);
    originalWarn(failures.length === 0 ? 'PASS' : 'FAIL');
    // The verdict is set, not exited: `process.exit` does not wait for a
    // pipe's pending writes, and the list above is the whole point of the run.
    // A check that is only ever read through `| Select-Object` or a redirected
    // handle loses everything after the last flushed chunk -- which is exactly
    // the ok/FAIL lines -- and reports an exit code with no reason beside it.
    process.exitCode = failures.length === 0 ? 0 : 1;
}

main().catch((err) => {
    // `console.error` is redirected into `errors` while the project runs, so
    // that a VM's own warning is not mistaken for the project failing. A
    // harness failure must not go the same way: it is this file's bug and it
    // has to be visible, which is why it writes to the descriptor directly
    // rather than through the patched console.
    process.stderr.write(`HARNESS FAILURE: ${err && err.stack ? err.stack : err}\n`);
    process.exitCode = 1;
});
