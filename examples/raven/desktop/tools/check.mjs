// Run the built project in a real Scratch VM and see whether the machine runs.
//
//     node tools/check.mjs                 the whole check
//     node tools/check.mjs --console       print the guest's console afterwards
//     node tools/check.mjs --budget 300    seconds to give it
//
// `raven check` and `raven build` say the project compiles; nothing so far says
// the machine boots. This does. It loads `dist/desktop-arm-virt-linux.sb3` into the Scratch VM
// the project is going to run in, presses the green flag, steps the runtime as
// fast as it can, and reads the console the UART produced -- `console_trace` on
// the stage, which is a byte-for-byte copy of everything the guest has
// transmitted -- for what the machine is supposed to have said.
//
// It needs a checkout of the Scratch VM:
//
//     SCRATCH_VM_ROOT=path/to/scratch-vm
//
// and it means two different things by "a real Scratch VM", so it uses both.
// The vanilla VM interprets every block, which is the slowest way to run the
// machine and the one that cannot get Linux through its own early boot: an
// instruction is about four hundred blocks and the interpreter retires a few
// thousand a second, so a boot is hours. TurboWarp's VM compiles the blocks to
// JavaScript first, which is the same blocks with the same semantics and
// roughly an order of magnitude more instructions a second, and the kernel's
// boot is the part of the check that needs it. The vanilla VM stays the
// reference for what the blocks *say*: `tools/validate-sb3.js` and the block
// laws are its business, and the pen and UART checks below pass on either.
//
//     SCRATCH_VM_ROOT=ref/turbowarp-vm node tools/check.mjs --budget 900
//
// The renderer is attached and records rather than draws, because the monitor's
// output is pen lines and the check counts them: what the LCD controller scans
// out is the point of this project, so the pen is the thing being measured and
// not an implementation detail to be mocked away.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { writeScanoutPng, scanoutRgb } from './png.mjs';

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

// A runtime with a renderer asks the page whether it is hidden before it draws.
// There is no page here, and the answer only has to exist: this one says it is
// hidden, which is also the answer that skips the draw.
globalThis.document = { hidden: true };

/// Which VM to run in, and whether it can compile.
///
/// `SCRATCH_VM_ROOT` wins when it is set. Otherwise TurboWarp's checkout is
/// taken if there is one, because the machine is too slow for the vanilla
/// interpreter to finish the boot, and the vanilla VM the rest of the examples
/// use is the fallback. A VM that compiles is asked to: the flag is a property
/// of the runtime rather than of the project, so an unpatched VM is unaffected.
const candidates = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean);
const VM_ROOT = candidates.find((dir) => fs.existsSync(path.join(dir, 'src', 'virtual-machine.js')));
if (!VM_ROOT) {
    console.error(`no Scratch VM found; tried\n  ${candidates.join('\n  ')}`);
    process.exit(2);
}

const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// A renderer that records what the pen did instead of drawing it.
///
/// Only the calls this project makes are here. The fence is the identity,
/// because the monitor lays its own pixels out inside the stage and clipping is
/// the monitor's business; the check is about what it asked to draw.
function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    // Every line ever drawn. It is kept alongside the raster rather than
    // rebuilt into one, because a line is what a colour assertion is about and
    // the raster is what a picture assertion is about. It is trimmed in blocks
    // because a run makes millions of them and every one is an object.
    const lines = [];
    let drawn = 0;
    // Every pen colour the monitor has ever used. It is a set rather than a
    // re-read of `lines`, because `lines` is trimmed and the boot ROM's
    // eight-colour bring-up pattern is the *oldest* thing in it.
    const colours = new Set();
    // The Stage itself, kept as the pen draws rather than rebuilt from the line
    // history. `pen clear` empties it, which is what the monitor does on every
    // pass, so this is the *current* picture -- the one the pass that just
    // finished painted -- and not the accumulation the row-only redraw made.
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
    const record = (line) => {
        drawn += 1;
        const c = line.colour;
        if (c) colours.add((Math.round(c[0] * 255) << 16) | (Math.round(c[1] * 255) << 8) |
            Math.round(c[2] * 255));
        lines.push(line);
        if (lines.length > 2000000) lines.splice(0, lines.length - 1000000);
    };
    return {
        lines,
        drawn: () => drawn,
        colours,
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
        penClear() { this.clears++; pixels.fill(-1); },
        penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) {
            record({ x0, y0, x1, y1, pen: attrs.diameter,
                colour: attrs.color4f ? [...attrs.color4f] : null });
            stamp(attrs.diameter, attrs.color4f, x0, y0, x1, y1);
        },
        penPoint(_skin, attrs, x, y) {
            record({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter,
                colour: attrs.color4f ? [...attrs.color4f] : null });
            stamp(attrs.diameter, attrs.color4f, x, y, x, y);
        },
        draw() {}
    };
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 900) * 1000;
const showConsole = args.includes('--console');
const sb3 = args.find((a) => a.endsWith('.sb3')) || path.join(root, 'dist', 'desktop-arm-virt-linux.sb3');

const failures = [];
const passes = [];
const check = (ok, what) => (ok ? passes : failures).push(what);

async function main() {
    if (!fs.existsSync(sb3)) {
        console.error(`no project at ${sb3}; run node tools/build.mjs first`);
        process.exit(2);
    }

    const vm = new VirtualMachine();
    const renderer = recordingRenderer();
    vm.attachRenderer(renderer);

    const originalWarn = console.warn;
    const errors = [];
    console.warn = () => {};
    // Node writes its own deprecation and experimental notices to stderr from
    // inside the VM's dependencies, and those are not the runtime complaining
    // about the project: only a message that is not one of those is kept.
    console.error = (...a) => {
        const line = a.map(String).join(' ');
        if (!/\b(Deprecation|Experimental)Warning\b/.test(line)) errors.push(line);
    };

    const data = fs.readFileSync(sb3);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

    const runtime = vm.runtime;
    // TurboWarp's runtime carries the compiler's options and its VM defaults
    // them on; an unpatched VM has no such object here and runs the blocks the
    // way Scratch does, which is the slow path and the one the budget has to
    // cover. Setting `enabled` is what the sequencer reads when it decides
    // whether a thread is worth compiling, so it has to happen before the green
    // flag starts any.
    const turbo = !!(runtime.compilerOptions);
    if (turbo) runtime.compilerOptions.enabled = !args.includes('--no-compile');
    const target = (name) => runtime.targets.find((t) => t.getName() === name);
    const value = (t, name) => {
        const v = Object.values(t.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    const stage = () => runtime.getTargetForStage();

    /// Everything the guest has transmitted, as text. The UART's console is a
    /// byte stream, so this is what a serial terminal on the board's connector
    /// would have shown.
    const consoleText = () => {
        const bytes = value(stage(), 'console_trace') || [];
        return Buffer.from(bytes.map((b) => Number(b) & 0xff)).toString('latin1');
    };

    vm.greenFlag();
    // The sequencer gives each step a wall-clock budget of 75% of
    // `currentStepTime`, and that is null until `runtime.start()` sets it -- so
    // without this line `stepThreads` runs zero threads and every check passes
    // against a machine that never executed an instruction. Setting it here
    // rather than calling `start()` keeps the stepping synchronous: one
    // `_step()` is one 30 Hz frame, which is exactly what the machine's own
    // frame loop yields on.
    runtime.currentStepTime = 1000 / 30;

    const started = Date.now();
    let steps = 0;
    while (Date.now() - started < budgetMs) {
        runtime._step();
        steps++;
    }
    const text = consoleText();

    // ---- the keyboard: a real keypress reaches the PL011's receiver -------
    //
    // The machine's keyboard is the *reader's* keyboard. `src/sprites/input.rav`
    // is the ARM board's only source of console input: a hat per key the key
    // dropdown names, and `when any key pressed` followed by `key pressed?` for
    // the punctuation, both writing through `dev::console::type_byte` into the
    // PL011's receive FIFO. What proves it is not a register read back -- that
    // only says the model queued something -- but the *guest's own echo*: the
    // shell `init` execs is interactive on `/dev/console`, which this command
    // line makes `ttyAMA0`, so a key typed into the port comes back out of the
    // transmitter as the character the shell read.
    //
    // The keys are posted the way a person presses them -- down, one runtime
    // step, up -- and they are posted into the VM's keyboard device and nothing
    // else: no pointer, no sprite, no picture of a key.
    let typedBack = null;
    const promptSeen = () => /[#$] ?$/.test(consoleText());
    for (let i = 0; i < 3600 && !promptSeen(); i++) { runtime._step(); steps++; }
    if (promptSeen()) {
        /// One key, and then a wait for the console to come back before the
        /// next: a person types at the speed the shell echoes, and the PL011's
        /// own FIFO is sixteen bytes deep, so a check that filled it faster
        /// than the guest reads would be testing the driver's overrun handling
        /// instead of the keyboard.
        const typeKey = (key) => {
            const was = consoleText().length;
            runtime.ioDevices.keyboard.postData({ key, isDown: true });
            runtime._step(); steps++;
            runtime.ioDevices.keyboard.postData({ key, isDown: false });
            runtime._step(); steps++;
            for (let i = 0; i < 600 && consoleText().length === was; i++) { runtime._step(); steps++; }
        };
        const since = consoleText().length;
        for (const key of 'echo raven-keys') typeKey(key);
        typeKey('Enter');
        // Two `raven-keys` and not one: the first is the tty echoing the line
        // back as it was typed, which only proves the bytes arrived; the second
        // is `echo` printing its own argument, which proves the shell read the
        // line and ran it.
        const said = () => consoleText().slice(since);
        const twice = () => (said().match(/raven-keys/g) || []).length >= 2;
        for (let i = 0; i < 1800 && !twice(); i++) { runtime._step(); steps++; }
        typedBack = said();
    }
    originalWarn(`keys          ${typedBack === null ? 'no shell prompt to type at' :
        `typed "echo raven-keys" with real keypresses; the port answered ${JSON.stringify(typedBack)}`}`);
    check(typedBack !== null && /echo raven-keys/.test(typedBack) &&
        (typedBack.match(/raven-keys/g) || []).length >= 2,
        'real keypresses reach the guest and the shell runs what was typed: the port echoed ' +
        '"echo raven-keys" and then printed its own output for it');

    // The picture, as a file. The assertion below reads the framebuffer the
    // controller is scanning to decide whether the console took the Stage over,
    // and a boolean is not something anyone can look at -- the machine takes
    // hours to get here and this is the whole of what it produced. So the same
    // memory goes out as a PNG beside the project, three times size, whatever
    // the assertions then say.
    let picture;
    try {
        picture = writeScanoutPng(path.join(root, 'dist', 'stage.png'),
            (name) => value(stage(), name), value(stage(), 'ram') || [], 3);
    } catch (err) {
        picture = `none: ${err.message}`;
    }

    originalWarn(`file          ${path.relative(process.cwd(), sb3)}`);
    originalWarn(`vm            ${path.relative(repo, VM_ROOT)}` +
        `${turbo && !args.includes('--no-compile') ? ' (compiling blocks to JavaScript)' : ' (interpreting blocks)'}`);
    originalWarn(`steps         ${steps} runtime steps in ${((Date.now() - started) / 1000).toFixed(1)} s`);
    originalWarn(`console       ${text.length} bytes`);
    originalWarn(`guest         ${value(stage(), 'cpu_instructions')} instructions retired`);
    originalWarn(`pen           ${renderer.drawn()} lines drawn over the whole run ` +
        `(${renderer.clears} whole-panel erases)`);
    originalWarn(`monitor       ${value(stage(), 'monitor_frames')} frames, ` +
        `${value(stage(), 'monitor_reads')} pixels read, ` +
        `${value(stage(), 'monitor_runs')} runs drawn`);
    originalWarn(`picture       ${picture}`);

    // The header has always offered this flag and it used to do nothing, which
    // made "the guest's console is the thing being read" a claim the reader had
    // to take on faith. The two lines that decide the display question --
    // `Console: switching to colour frame buffer device` and `[drm] Initialized
    // pl111` -- are only in here, so print it when asked.
    if (showConsole) {
        originalWarn('--- console ---');
        originalWarn(text);
        originalWarn('--- end console ---');
    }

    check(text.includes('Hi\n'), 'the boot ROM wrote its three bytes to UART0');

    // The dots are the bring-up test: each one is the timer reaching the
    // processor through the interrupt controller and the handler returning.
    const dots = (text.match(/\./g) || []).length;
    check(dots >= 2,
        `the timer reached the processor through the interrupt controller (${dots} ticks)`);

    check(renderer.drawn() > 500,
        `the monitor drew the controller's scanout (${renderer.drawn()} pen lines)`);
    // The colours of the pen's whole history, not of the picture it is holding
    // now: `pen clear` empties the raster on every pass, so the current raster
    // is the *console* -- one background and a little ink -- and the bring-up
    // pattern that proves the controller's own scanout reached the Stage is
    // older than that and lives in the history.
    check(renderer.colours.size >= 6,
        `the pattern in the framebuffer reached the Stage (${renderer.colours.size} pen colours)`);

    // What the Stage actually holds, read out of the raster the pen left. The
    // monitor erases the whole pen layer on every pass and repaints the whole
    // panel, so the raster is the picture of the pass that just finished -- one
    // whole scanout and not a patchwork of passes, which is what the row-only
    // redraw made of it.
    //
    // This is the assertion a colour count cannot make, and the reason is the
    // shape of the two pictures rather than their palette. The boot ROM's
    // bring-up pattern is a regular grid of thirty-two pixel blocks of eight
    // saturated colours, so every pixel is ink: two of those eight colours are
    // dark (black and blue, a quarter of the area), five are lit (three
    // quarters less red, which lands between), and none of it is background. A
    // framebuffer console is the other way round -- one background colour under
    // most of the area with a thin minority of glyph pixels -- so "most of the
    // Stage is background" separates the two pictures while a colour count,
    // which both of them satisfy, does not.
    //
    // What it proves: the pixels the monitor put on the Stage by the end of the
    // run are predominantly a dark background, which the boot ROM's
    // checkerboard is not, so the console took the display over. What it does
    // not prove: that the ink is legible text, that the text is this kernel's,
    // or that a browser draws what the renderer recorded -- this is the
    // monitor's own output and not a screenshot. It is also a statement about
    // the *end* of the run: a budget too short to reach the handover leaves the
    // pattern on the Stage and fails here, correctly.
    const stageFace = (() => {
        const W = 480, H = 360;
        let dark = 0, lit = 0;
        for (let i = 0; i < W * H; i++) {
            const c = renderer.pixels[i];
            if (c < 0) continue;
            const ink = Math.round(255 * (0.299 * ((c >>> 16) & 0xff) +
                0.587 * ((c >>> 8) & 0xff) + 0.114 * (c & 0xff)));
            if (ink < 48) dark++;
            if (ink >= 96) lit++;
        }
        return { dark: dark / (W * H), lit: lit / (W * H) };
    })();
    check(stageFace.dark >= 0.5 && stageFace.lit <= 0.4,
        `the Stage is the kernel's framebuffer console, not the boot ROM's checkerboard ` +
        `(${(stageFace.dark * 100).toFixed(1)}% background and ${(stageFace.lit * 100).toFixed(1)}% lit; ` +
        `the pattern is at most 25.0% background and 62.5% lit)`);

    // The handover, as the controller's own register reports it. The boot ROM
    // points the scanout at its fixed 0xD00000; the kernel's pl111 driver
    // programs LCD_UPBASE with its own allocation, and the monitor reads that
    // register rather than a constant. This is not a rendering assertion -- it
    // says the controller was reprogrammed and the address is not the firmware's
    // -- but it is the mechanism the assertion above depends on, so a
    // regression in the register path names itself here.
    const ubas = Number(value(stage(), 'clcd_ubas'));
    check(ubas !== 0x00d00000,
        `the kernel moved the scanout off the boot ROM's framebuffer (LCD_UPBASE 0x${ubas.toString(16)})`);

    // The kernel and the initramfs came out of the flash chip and were handed
    // the machine. Everything past here is Linux's own console, which is the
    // same UART the dots came out of -- so a check that reads the console is
    // reading the kernel.
    check(/Linux version/.test(text), 'the kernel booted');
    check(text.includes('raven desktop'), 'the initramfs ran /init');

    // ---- the Stage against the scanout: measured, and not asserted --------
    //
    // The RISC-V check asserts the invariant -- after the monitor has drawn,
    // every pixel of the Stage's panel region is the card's -- because there the
    // panel *is* the Stage: 480 by 360 of card onto 480 by 360 of Stage. This
    // board resamples, and that is the difference between the two checks.
    //
    // A 320 by 240 panel on a 480 by 360 Stage makes every stroke 1.5 stage
    // pixels tall, so panel rows overlap, later rows are drawn over earlier
    // ones, and no stage pixel *is* a panel pixel. The sample below takes the
    // stage row a panel row's own stroke starts on (`floor(1.5 * row - 0.25)`,
    // which no later row overwrites) and the stage column its run starts at,
    // and even so a console reports about 12.7% of the sampled pixels differing
    // -- not zero and not small. A per-pixel equality test between two rasters
    // of different sizes cannot come out at zero however right the monitor is,
    // and what the number is really measuring is the pen's round cap and the
    // row overlap, neither of which this sample models. Turning it into an
    // assertion would be a guess in the direction that makes the check green,
    // so it stays a number.
    //
    // What can be said about residue without a mapping is said by construction
    // instead: every pass erases the whole pen layer and repaints the whole
    // panel, so there is no scheme left by which an older picture survives a
    // pass. See `src/monitor/lcd.rav`.
    //
    // The CPU is halted first, so the framebuffer cannot move between the pen's
    // last scan and the pixels being read, and the monitor is given a hundred
    // and twenty passes to repaint the panel from the frozen controller.
    {
        const halted = Object.values(stage().variables).find((x) => x.name === 'cpu_halted');
        if (halted) halted.value = 1;
        for (let i = 0; i < 120; i++) runtime._step();
        try {
            const shot = scanoutRgb((name) => value(stage(), name), value(stage(), 'ram') || []);
            const scaleX = 480 / shot.width, scaleY = 360 / shot.height;
            let worse = 0, better = 0;
            for (let y = 0; y < shot.height; y++) {
                const sy = Math.max(0, Math.floor(scaleY * y - 0.25));
                for (let x = 0; x < shot.width; x++) {
                    const sx = Math.floor((x + 0.5) * scaleX);
                    if (sx < 0 || sy < 0 || sx >= 480 || sy >= 360) continue;
                    const stageC = renderer.pixels[sy * 480 + sx];
                    const at = (y * shot.width + x) * 3;
                    const same = stageC >= 0 &&
                        ((stageC >>> 16) & 0xff) === shot.rgb[at] &&
                        ((stageC >>> 8) & 0xff) === shot.rgb[at + 1] &&
                        (stageC & 0xff) === shot.rgb[at + 2];
                    if (same) better++; else worse++;
                }
            }
            originalWarn(`scanout       ${shot.width}x${shot.height} against the Stage's ` +
                `480x360 raster: ${worse} of ${better + worse} sampled pixels differ ` +
                `(${(worse / (better + worse) * 100).toFixed(1)}%) -- measured, not asserted; ` +
                `a 320x240 panel on a 480x360 Stage resamples, so no sample can be exact`);
        } catch (err) {
            originalWarn(`scanout       could not be read back (${err.message})`);
        }
    }

    if (errors.length > 0) check(false, `${errors.length} runtime error(s): ${errors[0]}`);

    for (const what of passes) originalWarn(`ok   ${what}`);
    for (const what of failures) originalWarn(`FAIL ${what}`);
    originalWarn(failures.length === 0 ? 'PASS' : 'FAIL');
    // The verdict is set, not exited: `process.exit` does not wait for a pipe's
    // pending writes, and the list above is the whole point of the run.
    process.exitCode = failures.length === 0 ? 0 : 1;
}

main().catch((err) => {
    // `console.error` is redirected into `errors` while the project runs, so
    // that a VM's own warning is not mistaken for the project failing. A
    // harness failure must not go the same way: it is this file's bug and it
    // has to be visible, which is why it writes to the descriptor directly
    // rather than through the patched console.
    process.stderr.write(`HARNESS FAILURE: ${err && err.stack ? err.stack : err}\n`);
    process.exit(1);
});
