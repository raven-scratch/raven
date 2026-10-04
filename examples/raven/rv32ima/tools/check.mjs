// Run the built projects in a real Scratch VM and see whether the guests run.
//
//     node tools/check.mjs                 both images
//     node tools/check.mjs baremetal       one of them
//     node tools/check.mjs --screen        print the console afterwards
//     node tools/check.mjs --budget 300    seconds per image
//
// `raven check` and `raven build` say the projects compile; nothing so far says
// the guests run. This does. For each image it loads `dist/<name>.sb3` into the
// Scratch VM the project is going to run in, presses the green flag, steps the
// runtime as fast as it can, and then reads the terminal's own cell buffer --
// which *is* the screen, so the check sees what the reader sees -- for what
// that guest is supposed to have printed.
//
// For Linux it also drives the keyboard, because that is the half of the
// machine that no boot log exercises: `a` goes through a `when key pressed`
// hat, whose dropdown names it, and Backspace goes through the polled
// `key pressed?` whose key name is built because the dropdown has no word for
// it. The bare metal image never reads the console, so it has no such checks;
// what it has instead is the console itself, held still enough to drive by hand.
//
// It needs a checkout of the Scratch VM:
//
//     set SCRATCH_VM_ROOT=path/to/scratch-vm
//
// and takes about a minute, most of it Linux' own boot.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');

// The SVG sanitiser is not needed to execute blocks and some checkouts have no
// built copy of it, so it is stubbed the way `tools/validate-sb3.js` stubs it.
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

// A renderer is attached so that the pen's lines can be counted, and a runtime
// with a renderer asks the page whether it is hidden before it draws. There is
// no page here, and the answer only has to exist: this one says it is hidden,
// which is also the answer that skips the draw.
globalThis.document = { hidden: true };

const args = process.argv.slice(2);
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 240) * 1000;
const showScreen = args.includes('--screen');
const wanted = args.filter((a) => !a.startsWith('--') && !/^\d+$/.test(a));

const COLS = 64;
const ROWS = 20;

/// A renderer that records what the pen did instead of drawing it.
///
/// Only the calls this project makes are here, and the fence is the identity:
/// the terminal's grid is inside the pen's box by construction, which is the
/// layout's job and not something this check re-decides.
function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    const lines = [];
    return {
        lines,
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
        penClear() { this.clears++; lines.length = 0; },
        penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) { lines.push({ x0, y0, x1, y1, pen: attrs.diameter }); },
        penPoint(_skin, attrs, x, y) { lines.push({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter }); },
        draw() {}
    };
}

/// What each image is supposed to say, and where it says it.
const IMAGES = [
    {
        name: 'baremetal',
        sb3: 'dist/rv32baremetal.sb3',
        keyboard: false,
        cursor: true,
        colours: true,
        erase: true,
        expect: [
            ['the greeting ran', (t) => /Hello world from RV32 land\./.test(t)],
            ['the assembly function ran', (t) => /I'm an assembly function\./.test(t)],
            ['main is where the image says', (t) => /main is at:/.test(t) && /80000044/.test(t)],
            ['the processor measured itself', (t) => /Processor effective speed:/.test(t)],
            ['the machine powered off', (t) => /Poweroff/.test(t)]
        ]
    },
    {
        name: 'linux',
        sb3: 'dist/rv32linux.sb3',
        keyboard: true,
        expect: [
            ['the console came up', (t) => /console \[ttyS0\] enabled/.test(t)],
            ['the kernel mounted a rootfs', (t) => /Run \/init as init process/.test(t)],
            ['the guest booted', (t) => /Welcome to Buildroot/.test(t)],
            ['a login is waiting', (t) => /buildroot login:/.test(t)]
        ]
    },
    {
        // The reference's kernel with the reference rootfs's own login replaced
        // by a root shell, and three programs installed in it. Nothing in the
        // image starts a program for you, so the check does what a person does:
        // it waits for the prompt and then types. Every expectation is about
        // what the typed command printed.
        name: 'mini',
        sb3: 'dist/rv32mini.sb3',
        keyboard: false,
        terminal: true,
        typing: [
            {
                send: 'screenfetch',
                expect: /Shell:/,
                name: 'screenfetch drew its labels',
                // The labels alone are printed by every version of this image, so
                // they cannot tell a fresh build from a stale one. The logo can:
                // these rows are the jgs Tux, top of the art and its two credit
                // rows, and no other art this image has carried has them.
                also: [
                    ['screenfetch drew the jgs Tux', /a8888b/],
                    ['the logo keeps its credit rows', /jgs/],
                    ['the logo keeps its second credit row', /a:f/]
                ],
                seconds: 120
            },
            { send: "duktape -e 'console.log(12345)'", expect: /^12345$/m, name: 'duktape evaluated a program', seconds: 60 },
            { send: 'duktape /root/fizzbuzz.js', expect: /FizzBuzz/, name: 'duktape ran a script file', seconds: 60 },
            { send: 'coremark', expect: /Iterations\/Sec/, name: 'coremark measured the machine', seconds: 180 }
        ],
        expect: []
    },
    {
        // The image the Scratch project itself carries, and the one this example
        // was first built around. It is a different kernel and a different
        // rootfs from `linux_image` -- 4,263,001 bytes against 2,945,224 -- and
        // it signs on to a root shell rather than to a login.
        name: 'scratch',
        sb3: 'dist/rv32scratch.sb3',
        keyboard: true,
        expect: [
            ['the console came up', (t) => /console \[ttyS0\] enabled/.test(t)],
            ['the kernel mounted a rootfs', (t) => /Run \/init as init process/.test(t)],
            ['the guest booted', (t) => /Welcome to Linux On Scratch!/.test(t)],
            ['a shell is waiting', (t) => /~ #/.test(t)]
        ]
    }
];

const chosen = wanted.length > 0 ? IMAGES.filter((i) => wanted.includes(i.name)) : IMAGES;
if (chosen.length === 0) {
    console.error(`usage: node tools/check.mjs [${IMAGES.map((i) => i.name).join('|')}]`);
    process.exit(2);
}

const vmRoot = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : path.resolve(repo, '..', 'scratch-vm');
const { default: VirtualMachine } = await import('file://' + path.join(vmRoot, 'src/virtual-machine.js').replace(/\\/g, '/'));

let failures = 0;

for (const image of chosen) {
    const sb3 = path.join(root, image.sb3);
    if (!fs.existsSync(sb3)) {
        console.error(`${sb3} is not built: node tools/build.mjs ${image.name}`);
        process.exit(2);
    }

    const errors = [];
    const vm = new VirtualMachine();
    // The VM reports a runtime error to `console.error`. It also reports a
    // costume or a sound it could not load, which is this environment rather
    // than the project: `loadProject` in Node has no asset storage to load them
    // from, and the check draws no costume and plays no sound.
    console.error = (...a) => {
        const line = a.map(String).join(' ');
        if (/No storage module present/.test(line) || /DeprecationWarning/.test(line)) return;
        if (/error/i.test(line)) errors.push(line);
    };
    console.warn = () => {};

    const data = fs.readFileSync(sb3);
    const renderer = recordingRenderer();
    vm.attachRenderer(renderer);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

    const runtime = vm.runtime;
    const target = (name) => runtime.targets.find((t) => t.getName() === name);
    const bind = (t, name) => Object.values(t.variables).find((v) => v.name === name);
    const value = (t, name) => {
        const v = bind(t, name);
        return v ? v.value : undefined;
    };
    const screen = () => {
        const cells = value(target('Terminal'), 'glyphs');
        const rows = [];
        for (let r = 0; r < ROWS; r++) {
            let line = '';
            for (let c = 0; c < COLS; c++) {
                const code = cells[c * ROWS + r];
                line += code > 31 && code < 127 ? String.fromCharCode(code) : ' ';
            }
            rows.push(line.replace(/\s+$/, ''));
        }
        return rows;
    };
    const text = () => screen().join('\n');

    vm.greenFlag();
    const started = Date.now();
    let steps = 0;
    const done = () => image.expect.every(([, f]) => f(text()));
    while (Date.now() - started < budgetMs && !done()) {
        runtime._step();
        steps++;
    }
    const rows = screen();
    const shown = rows.join('\n');

    // The keyboard, both ways. What the keyboard sent is the machine's console
    // input, and that is run 1 of the stage's `_gheap`: a run is a handle of
    // (base, length, capacity) followed by its items, which is how raven lays
    // one out. Reading it is how the check sees the byte without waiting for a
    // login prompt to echo it -- the guest does read the console, but through a
    // polled timer in its 8250 driver, and one character takes thousands of
    // frames of this machine's speed.
    const checks = image.expect.map(([name, f]) => [name, f(shown)]);
    let sent = '';
    if (image.keyboard) {
        const consoleInput = () => {
            const heap = value(runtime.getTargetForStage(), '_gheap');
            return heap.slice(heap[0] - 1, heap[0] - 1 + heap[1]);
        };
        const send = (key) => {
            runtime.ioDevices.keyboard.postData({ key, isDown: true });
            for (let i = 0; i < 3; i++) runtime._step();
            runtime.ioDevices.keyboard.postData({ key, isDown: false });
            for (let i = 0; i < 3; i++) runtime._step();
        };
        const hatFrom = consoleInput().length;
        send('a');
        const fromHat = consoleInput().slice(hatFrom).filter((v) => v === 97);
        const pollFrom = consoleInput().length;
        send('Backspace');
        const fromPoll = consoleInput().slice(pollFrom).filter((v) => v === 127);
        runtime.ioDevices.keyboard.postData({ key: 'Backspace', isDown: true });
        for (let i = 0; i < 3; i++) runtime._step();
        const polling = value(target('Input'), 'held')[0] === 1;
        runtime.ioDevices.keyboard.postData({ key: 'Backspace', isDown: false });
        for (let i = 0; i < 3; i++) runtime._step();
        sent = `hat sent ${JSON.stringify(fromHat)}, poll sent ${JSON.stringify(fromPoll)}`;
        checks.push(['a key hat reached the machine', fromHat.length === 1]);
        checks.push(['the polled key reached the machine', fromPoll.length === 1]);
        checks.push(['the poll asked for the key', polling]);
    }

    // Typing, which is the only way anything runs in the `mini` image: nothing in
    // it starts a program for you, so the check does what a person does. One key
    // at a time, the way the section above does it, and one command at a time --
    // each waits for the prompt, types what it was given, and then waits for the
    // command's own output, because a keystroke's echo takes a fraction of a
    // second of guest time and coremark takes seconds of it.
    if (image.typing) {
        const press = (key) => {
            runtime.ioDevices.keyboard.postData({ key, isDown: true });
            for (let i = 0; i < 4; i++) { runtime._step(); steps++; }
            runtime.ioDevices.keyboard.postData({ key, isDown: false });
            for (let i = 0; i < 4; i++) { runtime._step(); steps++; }
        };
        const lastLine = () => text().split('\n').filter((line) => line.trim()).pop() ?? '';
        const settle = (test, seconds) => {
            const until = Date.now() + seconds * 1000;
            while (Date.now() < until && !test()) { runtime._step(); steps++; }
            return test();
        };
        // The prompt, before anything is typed. An image that stops at a login is
        // not an image you can type at, so it is the first thing asserted.
        const prompted = settle(() => /[#$]\s*$/.test(lastLine()), 120);
        checks.push(['it booted to a root prompt with no login', prompted]);
        for (const typed of image.typing) {
            for (const character of typed.send) press(character);
            press('Enter');
            checks.push([typed.name, settle(() => typed.expect.test(text()), typed.seconds ?? 60)]);
            // What the same screen has to say as well, read before the next
            // command is typed: the labels a command prints are the same labels
            // in every version of this image, so a check made of them alone
            // passes against a *stale* `dist/rv32mini.sb3` built from an older
            // image. Anything here has to be something only this image produces
            // -- a row of the logo, say -- or it is not worth asserting.
            for (const [name, expect] of typed.also ?? []) {
                // Waited for, not read once: the logo is printed a row at a
                // time and the primary expectation above fires as soon as a row
                // near the *top* of the block is out, so a row below it is not
                // on the screen yet. Reading it once here is how a check calls a
                // correct logo missing.
                const ok = settle(() => expect.test(text()), typed.seconds ?? 60);
                checks.push([name, ok]);
                // A picture of where the art actually went, because "the logo
                // row is not on the screen" is worth nothing without knowing
                // whether it scrolled off, was never printed, or is there under
                // another name.
                if (!ok) {
                    console.log(`          ${name}: ${expect} not found. The screen was:`);
                    for (const row of text().split('\n')) {
                        if (row.trim()) console.log(`          |${row}|`);
                    }
                }
            }
        }
    }

    const riscv = target('RISCV');
    checks.push(['the machine executed', value(riscv, 'instruction_n') > 1000]);
    checks.push(['RAM reaches the device tree', value(riscv, 'ram').length >= 67107136 + 1536]);
    checks.push(['nothing threw', errors.length === 0]);

    // The cursor, which is the font's own U+2588 FULL BLOCK. The check runs
    // after the bare metal image has powered off, so the machine is idle and
    // the guest writes nothing: the screen is emptied, the blink is held off
    // (which is the cursor held *on*), and whatever the pen draws next is the
    // cursor and only the cursor. A block drawn as one fat pen line would be
    // one line; a filled glyph is dozens, and it is a cell wide and most of a
    // cell tall, which is the other thing a rectangle of pen lines got wrong.
    let cursorLines = 0;
    let cursorBox = null;
    if (image.cursor) {
        const cells = value(target('Terminal'), 'glyphs');
        for (let i = 0; i < cells.length; i++) cells[i] = 0;
        bind(target('Terminal'), 'current_blinking').value = 0;
        for (let i = 0; i < 2; i++) runtime._step();
        renderer.lines.length = 0;
        runtime._step();
        cursorLines = renderer.lines.length;
        const xs = renderer.lines.flatMap((l) => [l.x0, l.x1]);
        const ys = renderer.lines.flatMap((l) => [l.y0, l.y1]);
        cursorBox = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
        checks.push(['the cursor is drawn as a glyph', cursorLines > 20]);
        checks.push(['the cursor covers a cell',
            cursorBox[1] - cursorBox[0] >= 6 && cursorBox[1] - cursorBox[0] <= 8 &&
            cursorBox[3] - cursorBox[2] >= 14 && cursorBox[3] - cursorBox[2] <= 17]);
    }

    // The console's colours and its erase sequences, driven straight at the
    // terminal rather than through the guest, because this is the half of the
    // escape handling no boot log exercises: the machine is idle, so the
    // machine's *console output* run is written to by hand -- run 2 of the
    // stage's `_gheap` -- and what the terminal did with it is read back.
    if (image.colours || image.erase || image.terminal) {
        const heap = value(runtime.getTargetForStage(), '_gheap');
        const base = heap[3];
        const oneStep = () => runtime._step();
        const feed = (codes) => {
            codes.forEach((code, i) => { heap[base - 1 + i] = code; });
            heap[4] = codes.length;
            oneStep();
        };
        // The console's output is a run in the arena, and a run holds a fixed
        // number of bytes: writing a longer sequence into it by hand walks off
        // the end of the run and the terminal reads a truncated escape that
        // never terminates. A guest writing one byte at a time never has to
        // think about it; a check injecting a whole sequence does, so a long
        // one goes in as several writes -- which is legal, because the escape
        // state is the terminal's and survives between them.
        const runBytes = heap[5];
        const feedLong = (codes) => {
            for (let i = 0; i < codes.length; i += 5) feed(codes.slice(i, i + 5));
        };
        const terminal = target('Terminal');
        const fg = () => value(terminal, 'current_fg');
        // What ink a cell was actually drawn in, which is the palette reaching
        // the screen rather than the terminal's variable.
        const cellInk = (col, row) => {
            const inks = value(terminal, 'fg');
            return inks[col * ROWS + row];
        };
        const A = (text) => [...text].map((c) => c.charCodeAt(0));
        if (image.colours) {
            const escape = (...params) => [27, 91, ...params, 109];
            // The palette's last entry, which is the default: the reference's
            // palette is white on black with one green, and the terminal holds it
            // in a run of the arena rather than in a Scratch list.
            const DEFAULT = '#FFFFFF';
            feed(escape(51, 50));               // ESC [ 32 m
            const green = fg();
            feed(escape(48));                   // ESC [ 0 m
            const afterZero = fg();
            feed(escape(51, 50));
            feed(escape());                     // ESC [ m
            const afterBare = fg();
            feed(escape(51, 50));
            feed(escape(51, 57));               // ESC [ 39 m
            const afterDefault = fg();
            checks.push(['SGR 32 is the palette green', green === '#00AA00']);
            checks.push(['SGR 0 returns to the default ink', afterZero === DEFAULT]);
            checks.push(['SGR m returns to the default ink', afterBare === DEFAULT]);
            checks.push(['SGR 39 returns to the default ink', afterDefault === DEFAULT]);
            // The bare form and the explicit zero are the same reset, and the
            // bare one is the one that keeps getting lost: an absent parameter
            // is the empty string, and Scratch's `=` will not call that the
            // number zero unless the code asks whether it is empty.
            checks.push(['a bare ESC [ m and an ESC [ 0 m agree', afterBare === afterZero]);
            // The palette is the sixteen a TUI needs, and the check that it is
            // is a cell's own colour: the ink a cell is drawn in is what the
            // cell holds, so a green cell has to say green.
            feed(escape(51, 50));
            feed([...A('Z')]);
            bind(terminal, 'cursor_x').value = 0;
            bind(terminal, 'cursor_y').value = 0;
            feed([...A('Z')]);
            const cell = cellInk(0, 0);
            checks.push(['a cell is drawn in the ink it was written with', cell === '#00AA00']);
            feed(escape(51, 52));               // ESC [ 34 m, blue
            bind(terminal, 'cursor_x').value = 1;
            bind(terminal, 'cursor_y').value = 0;
            feed([...A('Z')]);
            checks.push(['SGR 34 reaches the cell', cellInk(1, 0) === '#0000AA']);

            // The extended colours, which were the unasserted half of the SGR
            // handler: `38 ; 5 ; n` picks one of the sixteen the palette names,
            // one of the 6 by 6 by 6 cube, or one of the twenty-four greys, and
            // `38 ; 2 ; r ; g ; b` names a colour outright. All of them end as
            // the `#rrggbb` a cell already holds, so each is read back off a
            // cell of its own -- what the cell says is what the screen shows.
            const SGR_38_5 = [51, 56, 59, 53, 59];
            const SGR_48_5 = [52, 56, 59, 53, 59];
            const SGR_38_2 = [51, 56, 59, 50, 59];
            const codes = (n) => [...String(n)].map((c) => c.charCodeAt(0));
            const inCell = (sequence, col, want) => {
                const before = fg();
                feedLong(sequence);
                const ink = fg();
                // What the parser was left holding, because "the ink did not
                // move" has two very different causes: a sequence that was
                // understood and is the colour it already was, and a sequence
                // that was never read at all.
                const args = value(terminal, 'escape_args').join('|');
                const escapeState = value(terminal, 'is_escape');
                bind(terminal, 'cursor_x').value = col;
                bind(terminal, 'cursor_y').value = 0;
                feed([...A('W')]);
                const cell = cellInk(col, 0);
                return { ink, cell, want, args, escapeState,
                    ok: ink === want && cell === want };
            };
            const extended = [
                ['38;5;0 is the palette black', escape(...SGR_38_5, ...codes(0)), 2, '#000000'],
                ['38;5;2 is the palette green', escape(...SGR_38_5, ...codes(2)), 3, '#00AA00'],
                ['38;5;8 is the bright black', escape(...SGR_38_5, ...codes(8)), 4, '#555555'],
                ['38;5;17 is the cube\'s first step off black', escape(...SGR_38_5, ...codes(17)), 5, '#00005F'],
                ['38;5;196 is the cube\'s red corner', escape(...SGR_38_5, ...codes(196)), 6, '#FF0000'],
                ['38;5;232 is the first grey', escape(...SGR_38_5, ...codes(232)), 7, '#080808'],
                ['38;5;255 is the last grey', escape(...SGR_38_5, ...codes(255)), 8, '#EEEEEE'],
                ['38;2;18;52;86 is a colour of its own',
                    escape(...SGR_38_2, ...codes(18), 59, ...codes(52), 59, ...codes(86)), 9, '#123456']
            ];
            // What each one did, not only whether it was right: a report that
            // says "FAILED" and nothing else is the thing that lets a broken
            // escape path look like a broken check.
            const extendedOk = extended.map(([name, sequence, col, want]) => {
                const seen = inCell(sequence, col, want);
                checks.push([name, seen.ok]);
                return `${want}:${seen.ink}/${seen.cell}` +
                    `${seen.ok ? '' : ` FAILED (args "${seen.args}", is_escape ${seen.escapeState})`}`;
            });
            // `48 ; ...` is the background and this terminal has none -- the page
            // *is* the backdrop -- so its parameters are consumed and dropped.
            // The check is that dropping them really does leave the ink alone.
            feed(escape(51, 50));               // green ink first
            const beforeBare48 = fg();
            const background = inCell(escape(...SGR_48_5, ...codes(196)), 10, '#00AA00');
            const bgIgnored = background.ok && fg() === beforeBare48;
            checks.push(['48;5;n is consumed and leaves the ink alone', bgIgnored]);
            console.log(`colours   green ${green}, after 0 ${afterZero}, ` +
                `after bare m ${afterBare}, after 39 ${afterDefault}, ` +
                `cell inks ${cell} then ${cellInk(1, 0)}`);
            console.log(`colours   38;5;n and 38;2;r;g;b wanted/current_fg/cell: ` +
                `${extendedOk.join(', ')}; run holds ${runBytes} bytes`);
            console.log(`colours   48;5;n ${bgIgnored ? 'ignored' : 'NOT ignored'} ` +
                `(${background.ink} as ink, then fg ${fg()})`);
        }
        // Erasing. The parameterless `ESC [ J` is what a shell's line editor
        // sends with a backspace -- it erases from the cursor to the end of the
        // screen, which is usually nothing at all -- and it is the parameter
        // *and* the byte stream behind it that were both wrong. The text either
        // side of the sequence goes into the same write as the sequence, which
        // is how a shell sends it, so a clear that empties the console's own
        // output is a clear that loses the text. The cursor starts at the top
        // left and the one row written is read back as characters.
        // What a full-screen program opens with, which is the byte stream the
        // guest actually sent when `vi` was typed at its prompt: `ESC [ ? 1049 h`
        // -- a screen of its own to draw on -- and, to learn how big that screen
        // is, `ESC [ 999 ; 999 H` to push the cursor out of the way and then
        // `ESC [ 6 n` to ask where it ended up. That last one is a *question*,
        // and the answer belongs on the guest's input; the console once put it
        // in its own output queue, which is a place the guest never reads.
        if (image.terminal) {
            const rowOf = (r) => {
                const glyphs = value(terminal, 'glyphs');
                let line = '';
                for (let c = 0; c < COLS; c++) {
                    const code = glyphs[c * ROWS + r];
                    line += code > 31 && code < 127 ? String.fromCharCode(code) : ' ';
                }
                return line.replace(/\s+$/, '');
            };
            const wipe = () => {
                const glyphs = value(terminal, 'glyphs');
                for (let i = 0; i < glyphs.length; i++) glyphs[i] = 0;
                bind(terminal, 'cursor_x').value = 0;
                bind(terminal, 'cursor_y').value = 0;
                oneStep();
            };
            const inputText = () => heap.slice(heap[0] - 1, heap[0] - 1 + heap[1])
                .map((v) => String.fromCharCode(v)).join('');

            wipe();
            bind(terminal, 'cursor_x').value = 3;
            feed([27, 91, 54, 110]);                    // ESC [ 6 n
            const reply = inputText().includes('\u001b[1;4R');
            checks.push(['the cursor report is answered on the guest\'s input', reply]);

            wipe();
            feed([...A('ABC')]);
            const page = rowOf(0);
            feed([27, 91, 63, 49, 48, 52, 57, 104]);    // ESC [ ? 1049 h
            const blank = rowOf(0);
            bind(terminal, 'cursor_x').value = 0;
            feed([...A('XY')]);
            const own = rowOf(0);
            feed([27, 91, 63, 49, 48, 52, 57, 108]);    // ESC [ ? 1049 l
            const back = rowOf(0);
            checks.push(['the alternate screen saves the page and gives it back',
                page === 'ABC' && blank === '' && own === 'XY' && back === 'ABC']);

            feed([27, 91, 63, 50, 53, 108]);            // ESC [ ? 25 l
            const away = value(terminal, 'cursor_hidden');
            feed([27, 91, 63, 50, 53, 104]);            // ESC [ ? 25 h
            checks.push(['the cursor can be put away and brought back',
                away === 1 && value(terminal, 'cursor_hidden') === 0]);
            console.log(`terminal  cursor report on the guest's input: ${reply}; ` +
                `page ${JSON.stringify(page)} -> blank ${JSON.stringify(blank)} -> ` +
                `own ${JSON.stringify(own)} -> back ${JSON.stringify(back)}; ` +
                `cursor hidden ${away} then ${value(terminal, 'cursor_hidden')}`);
        }
        if (image.erase) {
            // A clear replaces the list's array rather than emptying it in
            // place, so the cells are read and written through the binding every
            // time rather than held in a variable of our own.
            const atRow = (r) => {
                const glyphs = value(terminal, 'glyphs');
                let line = '';
                for (let c = 0; c < COLS; c++) {
                    const code = glyphs[c * ROWS + r];
                    line += code > 31 && code < 127 ? String.fromCharCode(code) : ' ';
                }
                return line.replace(/\s+$/, '');
            };
            const row = () => atRow(0);
            const from = (codes) => {
                const glyphs = value(terminal, 'glyphs');
                for (let i = 0; i < glyphs.length; i++) glyphs[i] = 0;
                bind(terminal, 'cursor_x').value = 0;
                bind(terminal, 'cursor_y').value = 0;
                oneStep();
                feed(codes);
                return row();
            };
            // One write: three characters, the erase, one more character. `J`
            // does not move the cursor, so the character after a whole-screen
            // erase lands wherever the three before it left off pointing --
            // which is what the port's copy of the reference's cursor position
            // is for, and what the check reads the screen back with.
            const toEnd = from([...A('ABC'), 27, 91, 74, ...A('D')]);
            const whole = from([...A('ABC'), 27, 91, 50, 74, ...A('D')]);
            checks.push(['ESC [ J erases to the end and keeps what follows it', toEnd === 'ABCD']);
            checks.push(['ESC [ 2 J clears the screen and keeps what follows it', whole.trim() === 'D']);

            // Erasing, which the buffer cannot show, because the erase is the
            // *pen*. It is not a stroke of any shape: the pen is cleared and the
            // page is drawn out again from the buffer, so after `ESC [ J` has
            // taken the characters away the paper has nothing on it at all --
            // and what is left is the cursor, drawn on top of the page after it.
            // A partial erase would leave something: a black rectangle for every
            // cell, or worse, whatever the old glyph had drawn outside the cell's
            // own box. So: one clear, and one cursor's worth of ink.
            from([...A('ABC')]);
            const clears = renderer.clears;
            bind(terminal, 'current_blinking').value = 0;
            bind(terminal, 'cursor_x').value = 0;
            bind(terminal, 'cursor_y').value = 0;
            feed([27, 91, 74]);
            const left = renderer.lines.length;
            checks.push(['erasing the page clears the pen and draws it again',
                renderer.clears === clears + 1 && left === cursorLines]);
            console.log(`erase     ESC[J then D -> ${JSON.stringify(toEnd)}, ` +
                `ESC[2J then D -> ${JSON.stringify(whole)}, ` +
                `${renderer.clears - clears} clear, ${left} strokes left on the paper ` +
                `(the cursor is ${cursorLines})`);

            // `ESC [ K`, erase in line, which is what a line editor uses when it
            // rewrites a row: the row keeps whatever the new line covers and
            // loses the tail of the old one. Mode 2 takes the whole row, and the
            // row either side of it is what proves it stopped there.
            from([...A('ABCDEF')]);
            bind(terminal, 'cursor_x').value = 2;
            bind(terminal, 'cursor_y').value = 1;
            feed([...A('GH')]);                  // row 1 becomes "  GH"
            bind(terminal, 'cursor_x').value = 3;
            feed([27, 91, 75]);                  // ESC [ K from column 3, row 1
            const kept = atRow(1);
            bind(terminal, 'cursor_x').value = 0;
            feed([27, 91, 50, 75]);              // ESC [ 2 K, the whole row
            const gone = atRow(1);
            checks.push(['ESC [ K erases the rest of its own row',
                kept === '  G' && gone === '' && atRow(0) === 'ABCDEF']);
            console.log(`erase     ESC[K kept ${JSON.stringify(kept)}, ` +
                `ESC[2K left ${JSON.stringify(gone)}, row 0 still ` +
                `${JSON.stringify(atRow(0))}`);
        }
    }

    console.log(`=== ${image.name}  (${path.relative(process.cwd(), sb3)})`);
    console.log(`executed  ${value(riscv, 'instruction_n')} instructions in ${steps} frames, ` +
        `${((Date.now() - started) / 1000).toFixed(0)} s`);
    console.log(`RAM       ${value(riscv, 'ram').length} bytes`);
    if (sent) console.log(`keyboard  ${sent}`);
    if (cursorBox) {
        console.log(`cursor    ${cursorLines} pen lines, x ${cursorBox[0]}..${cursorBox[1]}, ` +
            `y ${cursorBox[2]}..${cursorBox[3]}`);
    }
    if (showScreen) {
        // Read at the end, not at the top: an image whose expectations are all
        // about what a typed command printed has its screen *after* the typing,
        // and the terminal keeps the last twenty lines of it.
        console.log('--- screen');
        screen().forEach((row, i) => { if (row) console.log(String(i).padStart(2) + ' |' + row + '|'); });
    }
    for (const [name, ok] of checks) {
        console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`);
        if (!ok) failures++;
    }
    if (failures > 0 && !showScreen) {
        console.log('--- screen at the end');
        rows.forEach((row, i) => { if (row) console.log(String(i).padStart(2) + ' |' + row + '|'); });
    }
    console.log('');
}

if (failures > 0) {
    console.log(`${failures} check(s) failed`);
    process.exit(1);
}
console.log('PASS');
