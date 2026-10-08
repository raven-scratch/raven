// A diagnostic probe: boot the machine and say what the Stage actually holds.
//
//     node tools/probe.mjs --budget 600
//     node tools/probe.mjs --png dist/stage.png --budget 200
//
// This is not `check.mjs`. It makes no assertions. It prints the LCD
// controller's own registers, the monitor's counters, the kernel's log buffer
// and a coarse classified picture of the pen lines the runtime recorded, so a
// reader can tell a boot ROM checkerboard from a framebuffer console without a
// colour count standing in for the answer.
//
// It also writes the framebuffer the controller is scanning to a PNG, because
// the machine takes hours to reach the handover and the picture is the point:
// `dist/stage.png`, blown up three times with nearest neighbour so the console
// font is legible.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { writeScanoutPng } from './png.mjs';

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

const VM_ROOT = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean).find((dir) => fs.existsSync(path.join(dir, 'src', 'virtual-machine.js')));
if (!VM_ROOT) { console.error('no Scratch VM'); process.exit(2); }
const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    const lines = [];
    return {
        lines, clears: 0,
        setLayerGrouping() {}, setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; }, createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; }, createPenSkin() { return nextId++; },
        destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {}, updateTextSkin() {},
        getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [480, 360]; },
        createDrawable() { const id = nextId++; drawables.set(id, { position: [0, 0] }); return id; },
        destroyDrawable(id) { drawables.delete(id); }, updateDrawableSkinId() {},
        updateDrawablePosition(id, position) { const d = drawables.get(id); if (d) d.position = [position[0], position[1]]; },
        updateDrawableDirectionScale() {}, updateDrawableVisible() {}, updateDrawableEffect() {},
        setDrawableOrder() {}, getDrawableOrder() { return 0; },
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; }, drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; }, drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; }, isTouchingDrawables() { return false; },
        penClear() { this.clears++; lines.length = 0; }, penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) {
            lines.push({ x0, y0, x1, y1, pen: attrs.diameter, colour: attrs.color4f ? [...attrs.color4f] : null });
        },
        penPoint(_skin, attrs, x, y) {
            lines.push({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter, colour: attrs.color4f ? [...attrs.color4f] : null });
        },
        draw() {}
    };
}

const args = process.argv.slice(2);
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 300) * 1000;
const sb3 = args.find((a) => a.endsWith('.sb3')) || path.join(root, 'dist', 'desktop-arm-virt-linux.sb3');

const vm = new VirtualMachine();
const renderer = recordingRenderer();
vm.attachRenderer(renderer);
const originalWarn = console.warn;
const errors = [];
console.warn = () => {};
console.error = (...a) => {
    const line = a.map(String).join(' ');
    if (!/\b(Deprecation|ExperimentalWarning)\b/.test(line)) errors.push(line);
};

const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
const runtime = vm.runtime;
if (runtime.compilerOptions) runtime.compilerOptions.enabled = !args.includes('--no-compile');
const out = originalWarn;
out(`vm           ${runtime.compilerOptions ? (runtime.compilerOptions.enabled ? 'compiling blocks' : 'interpreting blocks') : 'vanilla (interpreting)'}`);
const stage = runtime.getTargetForStage();
const cell = (t, name) => {
    const v = Object.values(t.variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};
const machine = runtime.targets.find((t) => t.getName() === 'Machine');
const get = (n) => cell(stage, n) ?? (machine ? cell(machine, n) : undefined);
const hex = (x) => (Number(x) >>> 0).toString(16);

vm.greenFlag();
runtime.currentStepTime = 1000 / 30;
const started = Date.now();
let steps = 0;
while (Date.now() - started < budgetMs) { runtime._step(); steps++; }

const text = Buffer.from((get('console_trace') || []).map((b) => Number(b) & 0xff)).toString('latin1');
out(`steps        ${steps} in ${((Date.now() - started) / 1000).toFixed(1)} s`);
out(`guest        ${get('cpu_instructions')} instructions retired`);
out(`console      ${text.length} bytes`);
out('--- the kernel console lines that matter');
for (const needle of ['Linux version', 'Console: switching', 'colour frame buffer device',
    'Initialized pl111', 'fb0', 'no panel detected', 'detected: ', 'EPROBE_DEFER',
    'No bridge', 'vblank wait timed out', 'raven desktop', '~ #']) {
    out(`  ${text.includes(needle) ? 'YES' : ' no'}  ${needle}`);
}
if (args.includes('--console')) {
    out('--- console text ---');
    out(text);
    out('--- end console text ---');
}

out('--- LCD controller registers');
for (const r of ['clcd_tim0', 'clcd_tim1', 'clcd_tim2', 'clcd_tim3', 'clcd_ubas', 'clcd_lbas',
    'clcd_cntl', 'clcd_ienb', 'clcd_ris', 'clcd_scan',
    'sys_regs_word21', 'monitor_frames', 'monitor_reads', 'monitor_runs']) {
    const v = get(r);
    if (v === undefined) continue;
    out(`  ${r.padEnd(14)} ${typeof v === 'number' ? '0x' + hex(v) + ` (${v})` : JSON.stringify(v)}`);
}
const sysregs = get('sys_regs');
if (Array.isArray(sysregs)) out(`  sys_regs[21] SYS_CLCD = 0x${hex(sysregs[20])} (${sysregs[20]})`);

// ---- the kernel's log buffer, followed through `log_buf`
const ram = get('ram') || [];
const word = (va) => Number(ram[(va - 0xc0000000) >>> 2]) >>> 0;
const LOG_BUF_PTR = Number(process.env.LOG_BUF_PTR || 0xc06d9fd8);
const LOG_BUF = Number(process.env.LOG_BUF || 0xc0702a7c);
const pointed = word(LOG_BUF_PTR);
const buffer = (pointed >= 0xc0000000 && pointed < 0xc1000000) ? pointed : LOG_BUF;
out(`--- kernel log buffer  log_buf ${hex(LOG_BUF_PTR)} -> ${hex(pointed)} (${buffer === pointed ? 'followed' : 'using __log_buf'})`);
{
    const bytes = [];
    for (let pa = buffer - 0xc0000000; bytes.length < 32768; pa += 4) {
        const w = word(0xc0000000 + pa);
        bytes.push(w & 0xff, (w >>> 8) & 0xff, (w >>> 16) & 0xff, (w >>> 24) & 0xff);
    }
    let s = '';
    for (const b of bytes) s += (b === 0) ? '<0>' : String.fromCharCode(b);
    // print the tail of the ring, which is where the newest records are
    out(s.replace(/[^\x09\x0a\x0d\x20-\x7e]+/g, ' ').slice(-6000));
}

// ---- the Stage, classified
const lines = renderer.lines;
const key = (l) => (l.colour || []).slice(0, 3).map((v) => Math.round(v * 255)).join(',');
const byColour = new Map();
for (const l of lines) byColour.set(key(l), (byColour.get(key(l)) || 0) + 1);
const ranked = [...byColour.entries()].sort((a, b) => b[1] - a[1]);
out(`--- Stage  ${lines.length} pen lines, ${byColour.size} distinct pen colours`);
out('  the twenty most used pen colours (r,g,b -> lines)');
for (const [k, n] of ranked.slice(0, 20)) {
    out(`    ${k.padEnd(14)} ${n}  ${(100 * n / (lines.length || 1)).toFixed(2)}%`);
}

// The boot ROM's eight saturated colours, as `clcd_rgb565` maps them with the
// PLD mode the firmware programs (mode 2), as 0xRRGGBB.
const BOOT = ['0,0,0', '248,0,0', '0,252,0', '0,0,248', '248,252,0', '0,252,248', '248,0,248', '248,252,248'];
const bootSet = new Set(BOOT);
let bootLines = 0;
for (const l of lines) if (bootSet.has(key(l))) bootLines++;
out(`  lines whose pen colour is one of the boot ROM's eight: ${bootLines} ` +
    `(${(100 * bootLines / (lines.length || 1)).toFixed(2)}%)`);

// A coarse picture: 80 by 30 cells, each cell's pen colour the last line that
// covers its centre. The Stage is 480 by 360, so a cell is 6 by 12 stage px.
{
    const CW = 80, CH = 30;
    const grid = Array.from({ length: CH }, () => new Array(CW).fill(null));
    for (const l of lines) {
        const y = l.y0;
        const cy = Math.floor((180 - y) / 12);
        if (cy < 0 || cy >= CH) continue;
        const lo = Math.min(l.x0, l.x1), hi = Math.max(l.x0, l.x1);
        const c0 = Math.max(0, Math.floor((lo + 240) / 6));
        const c1 = Math.min(CW - 1, Math.floor((hi + 240) / 6));
        for (let c = c0; c <= c1; c++) grid[cy][c] = key(l);
    }
    // Which of the boot colours, as a single character; anything else is '?'.
    const sym = new Map(BOOT.map((k, i) => [k, '.KRGBYCMW'[i]]));
    out('--- coarse Stage picture (80x30, boot colours as .KRGBYCMW, other as ?)');
    for (let r = 0; r < CH; r++) out('  ' + grid[r].map((k) => (k === null ? ' ' : (sym.get(k) ?? '?'))).join(''));
}

// What the controller is actually scanning, read straight out of SDRAM at the
// address the kernel programmed into LCD_UPBASE. This is the source of the
// picture above, and the two have to agree if the monitor is reading the
// framebuffer the kernel is writing.
{
    const ubas = Number(get('clcd_ubas'));
    const width = (Math.floor(Number(get('clcd_tim0')) / 4) % 64 + 1) * 16;
    const height = Number(get('clcd_tim1')) % 1024 + 1;
    const stride = width * 2;
    // A halfword read: the word at the address, and the half the address picks.
    // Reading the low half always would report the even pixel twice.
    const pixel = (x, y) => {
        const addr = ubas + y * stride + x * 2;
        return (Number(ram[addr >>> 2]) >>> ((addr & 2) * 8)) & 0xffff;
    };
    out(`--- the scanout framebuffer  ubas 0x${ubas.toString(16)} ${width}x${height} stride ${stride}`);
    const distinct = new Set();
    let nonBlack = 0;
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const p = pixel(x, y);
            distinct.add(p);
            if (p !== 0) nonBlack++;
        }
    }
    out(`  ${distinct.size} distinct 16-bit pixel values, ${nonBlack} of ${width * height} pixels are not 0x0000 ` +
        `(${(100 * nonBlack / (width * height)).toFixed(2)}% ink)`);
    out(`  the sixteen most common pixel values`);
    {
        const hist = new Map();
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            const p = pixel(x, y);
            hist.set(p, (hist.get(p) || 0) + 1);
        }
        const rankedPx = [...hist.entries()].sort((a, b) => b[1] - a[1]);
        for (const [p, n] of rankedPx.slice(0, 16)) {
            out(`    0x${p.toString(16).padStart(4, '0')}  ${String(n).padStart(6)}  ` +
                `${(100 * n / (width * height)).toFixed(3)}%  r${(p >> 11) & 31} g${(p >> 5) & 63} b${p & 31}`);
        }
    }
    // A character-ish picture: one character per panel pixel of every other
    // column and every other row, inked or not.
    out('  the framebuffer, one character per 4x4 block, # for any ink');
    for (let y = 0; y < height; y += 4) {
        let line = '  ';
        for (let x = 0; x < width; x += 4) {
            let ink = false;
            for (let dy = 0; dy < 4 && y + dy < height; dy++) {
                for (let dx = 0; dx < 4 && x + dx < width; dx++) if (pixel(x + dx, y + dy) !== 0) ink = true;
            }
            line += ink ? '#' : '.';
        }
        out(line);
    }
    out('  the first sixteen pixels of rows 0, 8, 16, 120');
    for (const y of [0, 8, 16, 120]) {
        const row = [];
        for (let x = 0; x < 16; x++) row.push(pixel(x, y).toString(16).padStart(4, '0'));
        out(`    row ${String(y).padStart(3)}  ${row.join(' ')}`);
    }
    // Pixel exact, one character a pixel, so a glyph is a glyph and a
    // checkerboard is a checkerboard. The ramp is by how far the pixel is from
    // black, which is all a reader needs to see which of the two it is.
    const ramp = ' .:-=+*#%@';
    const window = (x0, y0, w, h, label) => {
        out(`  pixels ${label}  x ${x0}..${x0 + w - 1}, y ${y0}..${y0 + h - 1}, one character a pixel`);
        for (let y = y0; y < y0 + h; y++) {
            let line = '    ';
            for (let x = x0; x < x0 + w; x++) {
                const p = pixel(x, y);
                const lum = ((p >> 11) & 31) * 2 + ((p >> 5) & 63) / 2 + (p & 31) * 2;
                line += ramp[Math.min(ramp.length - 1, Math.max(0, Math.round(lum * (ramp.length - 1) / 140)))];
            }
            out(line);
        }
    };
    window(0, 0, 160, 32, 'top left');
    window(0, 80, 160, 24, 'first text rows');
    window(0, 216, 160, 24, 'bottom rows');

    // The controller's colour routing against the routing the framebuffer's own
    // values imply. The Linux logo's beak and feet are the only strongly
    // coloured thing in this memory and 0xf5e1 is RGB565 for orange, so which
    // colour that lands on the Stage as says which way round the model's PLD
    // table has SYS_CLCD's two sixteen-bit modes.
    {
        const orange = 0xf5e1;
        let pixels = 0;
        for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
            if (pixel(x, y) === orange) pixels++;
        }
        const drawn = (r, g, b) => {
            let n = 0;
            for (const [k, count] of byColour) {
                const [a, c, d] = k.split(',').map(Number);
                if (Math.abs(a - r) <= 1 && Math.abs(c - g) <= 1 && Math.abs(d - b) <= 1) n += count;
            }
            return n;
        };
        out(`  the one strongly coloured value in the scanout: 0x${orange.toString(16)} x ${pixels} pixels`);
        out(`    as RGB565, which is what it is:     240,188,8   ->  ${drawn(240, 188, 8)} pen lines`);
        out(`    as the model's PLD mode 3 renders:    8,188,240   ->  ${drawn(8, 188, 240)} pen lines`);
    }
}

// What the boot ROM left at its own fixed framebuffer, for the comparison the
// task asks for: if the Stage were still the checkerboard this address would
// still hold the pattern and the monitor would be reading it.
{
    out('--- SDRAM at the boot ROM framebuffer 0xD00000, first pixel of each 32-pixel block');
    for (let band = 0; band < 8; band++) {
        const y = band * 32;
        const base = 0xd00000 + y * 640;
        const row = [];
        for (let c = 0; c < 10; c++) {
            const px = Number(ram[(base + c * 32 * 2) >>> 2]) & 0xffff;
            row.push(px.toString(16).padStart(4, '0'));
        }
        out(`  row ${String(y).padStart(3)}  ${row.join(' ')}`);
    }
}
out(`runtime errors: ${errors.length}${errors.length ? ' ' + errors[0] : ''}`);

// ---- the picture, as a file
//
// The framebuffer the controller is scanning, at three times size so the
// console font is legible, so that a reader who cannot leave the machine
// running for hours still gets to see what it draws.
{
    const pngArg = args.indexOf('--png');
    const file = pngArg >= 0
        ? path.resolve(args[pngArg + 1])
        : path.join(root, 'dist', 'stage.png');
    try {
        out(`--- picture  ${writeScanoutPng(file, get, ram, 3)}`);
    } catch (err) {
        out(`--- picture  none: ${err.message}`);
    }
}
