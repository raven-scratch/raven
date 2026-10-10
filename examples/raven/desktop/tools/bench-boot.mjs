// What each desktop project costs, on each runtime, to reach the three moments
// a reader actually waits for.
//
//     node tools/bench-boot.mjs                       every project, TurboWarp
//     node tools/bench-boot.mjs --vm vanilla          ... on Scratch's own VM
//     node tools/bench-boot.mjs --project arm          one of them
//     node tools/bench-boot.mjs --budget 900           seconds per run
//
// `bench.mjs` times the machine on a fixed slice; `watch-rv32.mjs --boot` times a
// RISC-V boot. Neither answers the question a reader of the README has, which is
// "how long do I have to sit here before something happens, and how long before
// it is a computer". So this times three moments and reports each one:
//
//   checkerboard   the boot ROM's bring-up pattern is on the Stage: the machine
//                  is alive and showing its own test picture, and nothing the
//                  guest did is visible yet
//   picture        the guest has taken the display over. On the ARM board the
//                  kernel's PL111 driver programs LCD_UPBASE away from the boot
//                  ROM's frame; on the RISC-V board `fbcon` starts writing the
//                  card. Either way the Stage stops being a test pattern
//   init           the guest handed over to `/init`, which for a Linux guest is
//                  the first moment there is a userspace at all. A bare-metal
//                  guest has no such moment and says so
//
// The three are *cumulative* seconds from the green flag, and the difference
// between two of them is the number that says where a boot's time goes.
//
// # Why the milestones are what they are
//
// Every one is read out of the machine's own state rather than inferred from a
// picture, because a picture is expensive to look at and this runs the same
// project hundreds of seconds at a time:
//
//   * `checkerboard` is the first frame in which the monitor drew anything at
//     all with a non-zero scanout base, which is the boot ROM's frame.
//   * `picture` is `clcd_ubas != 0xd00000` on the ARM board -- the exact
//     register the kernel reprograms, and the same test `check.mjs` asserts --
//     and on the RISC-V board the first frame in which `efb_words` holds
//     anything the guest wrote.
//   * `init` is the guest's own console text, which is the marker both checks
//     already wait for.
//
// # What the numbers mean, and what they do not
//
// The seconds are the *host's* as well as the machine's: the same check has been
// 25.9 s on an idle machine and 55.1 s beside a busy one. What reproduces is the
// guest instruction count at each milestone, so both are printed. A rate is
// printed too, because "instructions a second" is the number that compares two
// runtimes fairly when one of them is a hundred times slower than the other.

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
const budgetMs = Number(arg('--budget', 900)) * 1000;
const vmChoice = arg('--vm', 'turbo');
const only = arg('--project', null);
/// How long to run the vanilla VM's rate measurement. Long enough that the
/// interpreter retires a useful number of instructions, short enough that the
/// tool returns.
const rateWindowMs = Number(arg('--rate-window', 45)) * 1000;
/// The guest instructions a vanilla measurement runs to before stopping. It is
/// small because the interpreter is slow: this is a *rate* sample, not a boot.
const rateLimit = Number(arg('--rate-limit', 200000));

/// The three projects this directory builds, and what each one's guest is.
///
/// `bare` is the one that has no `/init`: embeddedDOOM is not an operating
/// system and has no userspace to hand over to, so its third milestone is the
/// first frame it draws rather than a console line. Saying so is better than
/// timing a marker that will never arrive and reporting the budget.
const PROJECTS = [
    {
        key: 'arm',
        file: 'dist/desktop-arm-virt-linux.sb3',
        board: 'ARM Versatile-PB',
        guest: 'Linux 6.6 (framebuffer console on the PL110)',
        counter: 'cpu_instructions',
        // The kernel reprograms LCD_UPBASE away from the boot ROM's frame.
        //
        // The test needs the *boot ROM's* address first and a different one
        // after: `clcd_ubas` starts at zero, so "is not `0xd00000`" is already
        // true before the ROM has programmed anything, and asking it that way
        // reports the handover at the first frame. So the state machine is
        // two-step, and `handoverState` below carries the first step.
        handover: (v, seen) =>
            seen.romFrame && (Number(v('clcd_ubas')) >>> 0) !== 0x00d00000,
        handoverWhat: 'LCD_UPBASE moved off the boot ROM\u2019s 0xd00000',
        // The ROM's frame, which has to be seen before the handover means
        // anything.
        romFrame: (v) => (Number(v('clcd_ubas')) >>> 0) === 0x00d00000,
        init: 'Run /init as init process',
        initWhat: 'the kernel handed over to /init'
    },
    {
        key: 'rv32',
        file: 'dist/desktop-rv32-linux.sb3',
        board: 'RISC-V mini-rv32',
        guest: 'Linux 6.8 (framebuffer console on the card)',
        counter: 'rv_instructions',
        // The direct colour panel: the guest's `simplefb` writes the card's own
        // memory, so a pixel store is the first thing it drew.
        handover: (v) => Number(v('efb_writes')) > 0,
        handoverWhat: 'the guest wrote the card',
        init: 'Run /init as init process',
        initWhat: 'the kernel handed over to /init'
    },
    {
        key: 'doom',
        file: 'dist/desktop-rv32-doom.sb3',
        board: 'RISC-V mini-rv32',
        guest: 'embeddedDOOM, bare metal',
        counter: 'rv_instructions',
        // The indexed panel has a commit handshake, so Doom's first picture is
        // the first frame the card latched rather than a pixel store.
        handover: (v) => Number(v('efb_frames')) > 0,
        handoverWhat: 'Doom latched a frame onto the card',
        init: null,
        initWhat: 'a bare-metal guest has no /init'
    }
];

const candidates = {
    turbo: path.join(repo, 'ref', 'turbowarp-vm'),
    vanilla: path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
};
const VM_ROOT = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : candidates[vmChoice];
if (!VM_ROOT || !fs.existsSync(path.join(VM_ROOT, 'src', 'virtual-machine.js'))) {
    console.error(`no Scratch VM at ${VM_ROOT}; try --vm turbo or --vm vanilla`);
    process.exit(2);
}

const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// A renderer that does nothing. This tool times the machine, and a renderer
/// that rasterised the pen would put the host's own drawing into every number.
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

// A Note on the vanilla VM, and why this tool measures a rate rather than
// waiting for the milestones.
//
// Scratch's own VM interprets every block. The ARM board retires about 7,700
// guest instructions a second there, so the 113.7 million instructions a boot
// takes would be **four hours** -- and the RISC-V board's slice is 262,144
// instructions, which the interpreter runs in thirty-four seconds without
// yielding, so a `_step()` on that project does not return in any useful time at
// all. A tool that waited would not be a measurement; it would be an afternoon.
//
// So on the vanilla VM this measures the *rate* -- instructions retired against
// the wall clock, on a machine bounded by `machine_limit` so the run ends -- and
// multiplies it by the instruction counts the compiled run already produced. The
// estimate is honest about which half is measured and which is arithmetic, and
// the arithmetic is exact: the milestones are reached at the same guest
// instructions on both VMs, because the guest is deterministic and only the host
// differs.

/// Measure a build's guest-instruction rate on this VM, over a bounded window.
///
/// The bound is the machine's own `machine_limit`, which is the only counter
/// here that belongs to the emulated machine: without it a single slice on the
/// RISC-V board runs for half a minute and the harness never gets the frame
/// back. `limit` is set to a small number so the run ends promptly, and what is
/// measured is the rate at which the interpreter retires instructions, not a
/// milestone -- those come from the arithmetic above.
async function measureRate(spec, limit) {
    const file = path.join(root, spec.file);
    if (!fs.existsSync(file)) return null;

    const vm = new VirtualMachine();
    vm.attachRenderer(nullRenderer());
    const warnings = console.warn;
    console.warn = () => {};
    const errors = console.error;
    console.error = () => {};
    const data = fs.readFileSync(file);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
    console.warn = warnings;
    console.error = errors;

    const runtime = vm.runtime;
    if (runtime.compilerOptions) runtime.compilerOptions.enabled = false;
    const stage = runtime.getTargetForStage();
    const value = (name) => {
        const v = Object.values(stage.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    const machine = runtime.targets.find((t) => t.getName() === 'Machine');
    const setMachine = (name, v) => {
        const x = machine && Object.values(machine.variables).find((y) => y.name === name);
        if (x) x.value = v;
    };

    vm.greenFlag();
    runtime.currentStepTime = 1000 / 30;
    // Two different bounds, because the two boards are bounded differently.
    //
    // `machine_slice` is the number of instructions a slice may retire and both
    // boards have it, so it is the one that always applies. The ARM board also
    // has `machine_limit`, which stops the whole machine at an exact guest
    // instruction -- used when it is there, because a rate measured between two
    // exact counts is the rate and not an average of whatever the clock did.
    //
    // The RISC-V board has no `machine_limit`: its slice is bounded by cycles
    // and by wall clock and by nothing else, which is why a bare `_step()` on
    // that project runs for half a minute. A small slice is what makes it
    // return.
    setMachine('machine_slice', 2048);
    setMachine('machine_slice_us', 0);
    setMachine('machine_limit', limit);
    setMachine('machine_budget', 100000);

    const instructions = () => Number(value(spec.counter)) || 0;
    /// Whether the project's RAM list has stopped growing at Scratch's limit.
    ///
    /// This is why a vanilla rate of zero is a *finding* rather than a bug in
    /// this tool. The RISC-V board's `hart_load_ram` fills `rv_ram` with
    /// `add to list` in a loop, and Scratch refuses to grow a list past 200,000
    /// items -- a limit `src/mem/ram.rav` documents and the ARM board avoids by
    /// writing its SDRAM into the project as a literal. On Scratch's own VM the
    /// RISC-V boards therefore stop at the reset vector for ever: the guest
    /// image never finishes loading. TurboWarp's VM has no such limit.
    const ramStuck = () => {
        const ram = value('rv_ram');
        return Array.isArray(ram) && ram.length >= 200000;
    };
    const started = Date.now();
    let steps = 0;
    // The stop conditions are all three, because no single one is enough:
    //   * the wall clock, so a project that never reaches the limit returns;
    //   * the guest instruction count, which is what is being measured;
    //   * a step ceiling, so a machine whose counter is named differently -- or
    //     is zero because it never started -- cannot spin for ever.
    const maxSteps = 4000;
    while (
        Date.now() - started < rateWindowMs &&
        instructions() < limit &&
        steps < maxSteps
    ) {
        runtime._step();
        steps += 1;
    }
    const secs = (Date.now() - started) / 1000;
    const done = instructions();
    return {
        secs,
        steps,
        instructions: done,
        rate: done / Math.max(0.001, secs),
        ramStuck: ramStuck()
    };
}

/// One project, one runtime, three milestones.
async function runProject(spec) {
    const file = path.join(root, spec.file);
    if (!fs.existsSync(file)) {
        return { ...spec, missing: true };
    }

    const vm = new VirtualMachine();
    vm.attachRenderer(nullRenderer());
    const warnings = console.warn;
    console.warn = () => {};
    console.error = () => {};
    const data = fs.readFileSync(file);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

    const runtime = vm.runtime;
    const turbo = !!(runtime.compilerOptions);
    if (turbo) runtime.compilerOptions.enabled = !args.includes('--no-compile');

    const stage = runtime.getTargetForStage();
    const value = (name) => {
        const v = Object.values(stage.variables).find((x) => x.name === name);
        return v ? v.value : undefined;
    };
    const consoleText = () => {
        const bytes = value('console_trace') || [];
        return Buffer.from(bytes.map((b) => Number(b) & 0xff)).toString('latin1');
    };
    /// How many guest instructions have retired. Both boards count them -- the
    /// ARM board as `cpu_instructions` and the RISC-V board as `rv_instructions`
    /// -- and it is the one number here that belongs to the emulated machine
    /// rather than to the host, so it is what reproduces between runs.
    const instructions = () => {
        const n = value(spec.counter);
        return n === undefined ? 0 : Number(n);
    };

    const started = Date.now();
    const marks = { checkerboard: null, picture: null, init: null };
    const instrAt = { checkerboard: null, picture: null, init: null };
    let steps = 0;
    let frames = 0;
    let sawInk = false;
    let sawHandover = false;
    let sawInit = false;
    /// The state a two-step handover test needs. The ARM board's is the only
    /// one: its controller's base starts at zero, so the handover can only be
    /// recognised once the boot ROM's own frame has been seen.
    const seen = { romFrame: false };

    vm.greenFlag();
    runtime.currentStepTime = 1000 / 30;

    while (Date.now() - started < budgetMs) {
        runtime._step();
        steps += 1;

        // The monitor counts the frames it has drawn; the first one that drew
        // anything is the checkerboard. `monitor_frames` is a pass count, so it
        // is one per frame once the monitor's loop is running.
        if (!sawInk) {
            const drawn = Number(value('monitor_frames')) || 0;
            if (drawn > 0) {
                sawInk = true;
                marks.checkerboard = (Date.now() - started) / 1000;
                instrAt.checkerboard = instructions();
            }
        }
        if (!sawHandover) {
            // A board whose handover test needs the previous state first says so
            // with `romFrame`; the ARM board's controller base is zero until the
            // ROM programs it, so "not the ROM's frame" would otherwise be true
            // from the first instruction.
            if (spec.romFrame && spec.romFrame(value)) seen.romFrame = true;
            if (spec.handover(value, seen)) {
                sawHandover = true;
                marks.picture = (Date.now() - started) / 1000;
                instrAt.picture = instructions();
            }
        }
        if (!sawInit && spec.init !== null) {
            if (consoleText().includes(spec.init)) {
                sawInit = true;
                marks.init = (Date.now() - started) / 1000;
                instrAt.init = instructions();
            }
        }
        frames += 1;

        // A bare-metal guest is done when it has drawn: there is no later
        // milestone to wait for, so the run stops rather than spending the
        // whole budget proving nothing else happens.
        if (spec.init === null && sawHandover) break;
        if (sawInit) break;
    }

    const secs = (Date.now() - started) / 1000;
    const done = instructions();
    console.warn = warnings;
    return {
        ...spec,
        vm: path.relative(repo, VM_ROOT),
        turbo,
        secs,
        steps,
        done,
        marks,
        instrAt,
        rate: done / Math.max(0.001, secs),
        reachedInit: sawInit,
        reachedHandover: sawHandover
    };
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const chosen = PROJECTS.filter((p) => only === null || p.key === only);
const results = [];
for (const spec of chosen) {
    process.stderr.write(`  running ${spec.key} ...\n`);
    if (vmChoice === 'vanilla') {
        // The vanilla VM cannot be waited on: see the note above `measureRate`.
        // What is measured is the rate, and the milestone seconds are that rate
        // applied to the instruction counts the compiled run produces.
        const rate = await measureRate(spec, rateLimit);
        results.push({ ...spec, turbo: false, vanilla: true, rateSample: rate });
    } else {
        results.push(await runProject(spec));
    }
}

const fmt = (x) => (x === null || x === undefined ? '--' : x.toFixed(1));
/// A duration in the unit a reader needs: seconds up to a minute, then minutes
/// and hours. Four hours of boot is a real answer and `14400.0` is not readable.
const duration = (s) => {
    if (s === null || s === undefined) return '--';
    if (s < 90) return `${s.toFixed(1)} s`;
    if (s < 5400) return `${(s / 60).toFixed(1)} min`;
    return `${(s / 3600).toFixed(2)} h`;
};
const vanilla = results.some((r) => r.vanilla);

console.log('');
console.log(`vm            ${path.relative(repo, VM_ROOT)}` +
    `${vanilla ? ' (interpreting every block)' : ' (compiling blocks to JavaScript)'}`);

if (!vanilla) {
    console.log(`budget        ${(budgetMs / 1000).toFixed(0)} s per project`);
    console.log('');
    console.log('seconds from the green flag, and the guest instructions retired by then');
    console.log('');
    console.log(`  ${'project'.padEnd(7)} ${'checkerboard'.padStart(16)} ${'picture'.padStart(16)} ` +
        `${'init'.padStart(16)}  ${'instructions/s'.padStart(15)}`);
    for (const r of results) {
        if (r.missing) {
            console.log(`  ${r.key.padEnd(7)} ${'(no artifact; run tools/build.mjs)'.padStart(16)}`);
            continue;
        }
        const cell = (mark, instr) => mark === null ? '--' : `${fmt(mark)}s @ ${instr}`;
        console.log(`  ${r.key.padEnd(7)} ${cell(r.marks.checkerboard, r.instrAt.checkerboard).padStart(16)} ` +
            `${cell(r.marks.picture, r.instrAt.picture).padStart(16)} ` +
            `${cell(r.marks.init, r.instrAt.init).padStart(16)}  ${Math.round(r.rate).toString().padStart(15)}`);
    }
    console.log('');
    for (const r of results) {
        if (r.missing) continue;
        console.log(`${r.key}  ${r.board}, ${r.guest}`);
        if (r.marks.checkerboard !== null) {
            console.log(`     checkerboard  ${fmt(r.marks.checkerboard)} s: ` +
                `the boot ROM's bring-up pattern is on the Stage`);
        } else {
            console.log(`     checkerboard  not reached in ${(budgetMs / 1000).toFixed(0)} s`);
        }
        if (r.reachedHandover) {
            console.log(`     picture       ${fmt(r.marks.picture)} s, ` +
                `${((r.marks.picture - r.marks.checkerboard) || 0).toFixed(1)} s after the pattern: ` +
                `${r.handoverWhat} (${r.instrAt.picture} instructions)`);
        } else {
            console.log(`     picture       not reached in ${(budgetMs / 1000).toFixed(0)} s` +
                ` (${r.done} instructions retired)`);
        }
        if (r.init === null) {
            console.log(`     init          ${r.initWhat}`);
        } else if (r.reachedInit) {
            console.log(`     init          ${fmt(r.marks.init)} s, ` +
                `${((r.marks.init - r.marks.picture) || 0).toFixed(1)} s after the picture: ` +
                `${r.initWhat} (${r.instrAt.init} instructions)`);
        } else {
            console.log(`     init          not reached in ${(budgetMs / 1000).toFixed(0)} s` +
                ` (${r.done} instructions retired)`);
        }
        console.log(`     total         ${r.secs.toFixed(1)} s, ${r.steps} runtime steps, ` +
            `${r.done} instructions, ${Math.round(r.rate)} a second`);
    }
    console.log('');
} else {
    // ---- the vanilla VM: a measured rate, and arithmetic from it ----------
    //
    // The instruction counts are *not* re-derived here: they come from the
    // compiled run, which reached the same milestones at the same guest
    // instructions. The two VMs run the same guest; only the host differs, so
    // the instruction counts transfer and the seconds do not.
    const compiledAt = path.join(root, 'dist', 'bench-boot.json');
    let counts = null;
    if (fs.existsSync(compiledAt)) {
        try {
            const previous = JSON.parse(fs.readFileSync(compiledAt, 'utf8'));
            if (!previous.vm.includes('scratch-vm') || previous.vm.includes('turbowarp')) {
                counts = previous;
            }
        } catch { /* a malformed file is simply not used */ }
    }

    console.log(`rate window   ${(rateWindowMs / 1000).toFixed(0)} s each, bounded by ` +
        `machine_limit = ${rateLimit} guest instructions`);
    console.log('');
    console.log('The vanilla VM interprets every block, so a boot is hours and the');
    console.log('milestones cannot be waited for. What follows is a measured rate');
    console.log('applied to the guest instruction counts from the compiled run.');
    console.log('');
    console.log(`  ${'project'.padEnd(7)} ${'rate (instr/s)'.padStart(14)} ` +
        `${'checkerboard'.padStart(14)} ${'picture'.padStart(14)} ${'init'.padStart(14)}`);
    for (const r of results) {
        const rate = r.rateSample ? r.rateSample.rate : 0;
        const known = counts && counts.projects.find((p) => p.key === r.key);
        const estimate = (instr) => (rate > 0 && instr ? instr / rate : null);
        const at = (name) => (known && known.instructionsAtMark ? known.instructionsAtMark[name] : null);
        console.log(`  ${r.key.padEnd(7)} ${Math.round(rate).toString().padStart(14)} ` +
            `${duration(estimate(at('checkerboard'))).padStart(14)} ` +
            `${duration(estimate(at('picture'))).padStart(14)} ` +
            `${duration(estimate(at('init'))).padStart(14)}`);
    }
    console.log('');
    for (const r of results) {
        const rate = r.rateSample ? r.rateSample.rate : 0;
        console.log(`${r.key}  ${r.board}, ${r.guest}`);
        if (r.rateSample && r.rateSample.ramStuck) {
            console.log(`     rate          NOT MEASURABLE: the guest image never finished loading`);
            console.log(`                   \`rv_ram\` stopped at Scratch's 200,000-item list limit, so the`);
            console.log(`                   hart never left the reset vector. This board grows its RAM with`);
            console.log(`                   \`add to list\`; the ARM board writes its SDRAM into the project`);
            console.log(`                   as a literal and is not affected. TurboWarp has no such limit.`);
            console.log(`                   Boot time on Scratch's own VM: never.`);
            continue;
        }
        console.log(`     rate          ${Math.round(rate)} guest instructions/s ` +
            `(${r.rateSample ? r.rateSample.instructions : 0} in ` +
            `${r.rateSample ? r.rateSample.secs.toFixed(1) : '0'} s, ` +
            `${r.rateSample ? r.rateSample.steps : 0} steps)`);
        if (!counts) {
            console.log('     estimate      no compiled run in dist/bench-boot.json to take the');
            console.log('                   instruction counts from; run --vm turbo first');
            continue;
        }
        const known = counts.projects.find((p) => p.key === r.key);
        if (!known) {
            console.log('     estimate      the compiled run has no entry for this project');
            continue;
        }
        const at = (name) => (known.instructionsAtMark ? known.instructionsAtMark[name] : null);
        const est = (name) => {
            const n = at(name);
            return n && rate > 0 ? n / rate : null;
        };
        console.log(`     checkerboard  ${duration(est('checkerboard'))} ` +
            `(at ${at('checkerboard') ?? '--'} instructions)`);
        console.log(`     picture       ${duration(est('picture'))} ` +
            `(at ${at('picture') ?? '--'} instructions)`);
        if (r.init === null) {
            console.log(`     init          ${r.initWhat}`);
        } else {
            console.log(`     init          ${duration(est('init'))} ` +
                `(at ${at('init') ?? '--'} instructions)`);
        }
    }
    console.log('');
    console.log('counts from   the compiled run in dist/bench-boot.json' +
        `${counts ? ` (${counts.vm})` : ''}`);
    console.log('');
}

// A machine-readable copy, so a later run can be diffed against this one rather
// than read by eye, and so the vanilla run can take its instruction counts from
// the compiled one. Written beside the artifacts and not into them.
//
// A vanilla run does **not** overwrite a compiled run's file: the file's whole
// purpose on the vanilla side is to hold the compiled run's instruction counts,
// and clobbering it would destroy the numbers the estimate is made of.
const jsonAt = path.join(root, 'dist', 'bench-boot.json');
fs.mkdirSync(path.dirname(jsonAt), { recursive: true });
const payload = {
    vm: path.relative(repo, VM_ROOT),
    mode: vanilla ? 'vanilla' : 'turbo',
    budgetSeconds: budgetMs / 1000,
    projects: results.map((r) => ({
        key: r.key,
        board: r.board,
        guest: r.guest,
        missing: !!r.missing,
        seconds: vanilla ? null : r.secs,
        instructions: vanilla ? null : r.done,
        instructionsPerSecond: vanilla
            ? Math.round(r.rateSample ? r.rateSample.rate : 0)
            : Math.round(r.rate),
        steps: vanilla ? (r.rateSample ? r.rateSample.steps : 0) : r.steps,
        marks: vanilla ? null : r.marks,
        instructionsAtMark: vanilla ? null : r.instrAt
    }))
};
if (vanilla) {
    const vanillaAt = path.join(root, 'dist', 'bench-boot-vanilla.json');
    fs.writeFileSync(vanillaAt, JSON.stringify(payload, null, 2) + '\n');
    console.log(`written       ${path.relative(process.cwd(), vanillaAt)}`);
} else {
    fs.writeFileSync(jsonAt, JSON.stringify(payload, null, 2) + '\n');
    console.log(`written       ${path.relative(process.cwd(), jsonAt)}`);
}
