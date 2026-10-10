// Where the machine's *time* goes, per procedure, on a real guest.
//
//     node tools/profile-blocks.mjs --budget 120
//     node tools/profile-blocks.mjs --budget 120 --until 80000000
//     node tools/profile-blocks.mjs --budget 120 --vm vanilla
//
// `probe-blocks.mjs --profile` counts *interpreted block executions* over a few
// frames with the compiler off, which answers "what does the picture cost in
// blocks". That is the display's question. It is not the boot's. A boot retires
// a hundred million guest instructions, and in TurboWarp's VM those instructions
// are not blocks at all -- the compiler turns each custom block into a JavaScript
// function -- so the block census cannot see where a boot's seconds go.
//
// This one measures the boot where it actually runs. TurboWarp's compiler names
// each compiled procedure after the custom block's proccode, so wrapping the
// compiler's own entry point gives a per-procedure attribution of everything the
// machine does: `cpu_step`, `cpu_execute`, `cpu_class_load_store`, `bus_read32`,
// `cpu_translate`, the monitor's rows, and the rest.
//
// Two numbers per procedure, because they are different questions:
//
//   self     the time spent inside it and nothing it called
//   total    the time inside it and everything below it
//
// and one more that is the machine's own: guest instructions retired per
// procedure entry. A procedure whose self time is large and whose entry count is
// small is expensive per call; one whose entry count is enormous is a hot path
// however cheap it is.
//
// The compiler is switched off at the end, over a few frames, and the
// interpreter's block counts for those frames are printed beside the times --
// that is the same census `probe-blocks.mjs --profile` makes, so the two
// instruments agree about what the picture costs.

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
const budgetArg = args.indexOf('--budget');
const budgetMs = (budgetArg >= 0 ? Number(args[budgetArg + 1]) : 300) * 1000;
const untilArg = args.indexOf('--until');
const until = untilArg >= 0 ? Number(args[untilArg + 1]) : null;
/// `--boot` stops the run the moment the guest's console says the kernel has
/// handed over to `/init`, which is the marker `check.mjs` waits for and the
/// only end-to-end number a change to the machine can be judged by. It is the
/// same marker without the typing, the keyboard check and the picture, so a
/// boot is a measurement rather than an acceptance run.
const boot = args.includes('--boot');
const bootMarker = 'Run /init as init process';
const sb3 = args.find((a) => a.endsWith('.sb3')) ||
    path.join(root, 'dist', 'desktop-arm-virt-linux.sb3');
const topArg = args.indexOf('--top');
const top = topArg >= 0 ? Number(args[topArg + 1]) : 40;
const vmArg = args.indexOf('--vm');
const vmChoice = vmArg >= 0 ? args[vmArg + 1] : 'turbo';

const candidates = {
    turbo: path.join(repo, 'ref', 'turbowarp-vm'),
    vanilla: path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
};
const VM_ROOT = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : candidates[vmChoice];
if (!VM_ROOT || !fs.existsSync(path.join(VM_ROOT, 'src', 'virtual-machine.js'))) {
    console.error(`no Scratch VM at ${VM_ROOT}`);
    process.exit(2);
}

const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

// A renderer that does nothing. The monitor's own cost is measured as the guest
// runs; rasterising its strokes here would put the host's rasteriser into the
// number instead of the machine's.
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

const vm = new VirtualMachine();
vm.attachRenderer(nullRenderer());
const originalWarn = console.warn;
console.warn = () => {};
console.error = () => {};
const data = fs.readFileSync(sb3);
await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));

const runtime = vm.runtime;
const turbo = !!(runtime.compilerOptions);
if (turbo) runtime.compilerOptions.enabled = true;

const stage = () => runtime.getTargetForStage();
const value = (t, name) => {
    const v = Object.values(t.variables).find((x) => x.name === name);
    return v ? v.value : undefined;
};
const consoleText = () => {
    const bytes = value(stage(), 'console_trace') || [];
    return Buffer.from(bytes.map((b) => Number(b) & 0xff)).toString('latin1');
};

// ---------------------------------------------------------------------------
// The instrument
// ---------------------------------------------------------------------------

/// Where a compiled procedure lives, and why this is the right seam.
///
/// TurboWarp does *not* emit one JavaScript function per custom block. `compile`
/// turns each script into a generator function, and a custom block's body
/// becomes a *second* generator in `result.procedures[proccode]`; `tryCompile`
/// then calls each of those factories once per thread and keeps what they
/// return on `thread.procedures[proccode]` (`thread.js` lines 511-516).
///
/// What is on `thread.procedures[proccode]` is therefore the *factory*, not the
/// generator: `Function fun34_cpu_step` rather than a live generator object. The
/// generator a call actually runs is made by invoking the factory -- which the
/// generated code does at every call site, `thread.procedures["Wcpu_step"](...)`
/// -- so the factory is what has to be wrapped. Wrapping the returned generator
/// instead finds nothing to wrap, which is exactly what the first version of
/// this file did and what its zero rows said.
///
/// Two shapes arrive here, and the wrapper has to handle both:
///
///   * a *plain* function for a procedure that never yields -- the body runs
///     inside the call, so the call is the thing to time;
///   * a *generator function* for one that does, whose body runs when the caller
///     iterates what came back. The generated call site cannot tell them apart
///     and does not try, so the wrapper times the call and then wraps the
///     generator's own `next` as well, or the yielding half of the machine is
///     invisible. An earlier version of this file timed only the generators and
///     reported zero for everything.
///
/// The time attributed to a procedure is therefore its call plus its body, which
/// is the honest cost of a Scratch procedure call in this VM and the number a
/// change to the machine has to move.
const stats = new Map();
const stat = (name) => {
    let s = stats.get(name);
    if (!s) { s = { name, self: 0, calls: 0 }; stats.set(name, s); }
    return s;
};

/// A proccode as a reader can use it. The compiler prefixes a proccode with the
/// shape of what it returns -- `W` for a procedure with no result, `Z` for one
/// whose call site uses it as a generator -- so the prefix is stripped and the
/// parameter shape kept.
function prettyName(proccode) {
    return proccode.replace(/^[A-Z]/, '');
}

const instrumented = new WeakSet();
let wrappedCount = 0;

/// The call graph, as the run builds it: for each callee, how many times each
/// caller entered it. The wrap knows the callee by name and the caller from
/// `currentNode`, which is the same chain the timing uses -- so the stack costs
/// nothing to maintain and cannot disagree with the times.
/// `edges` maps callee name -> Map(caller name -> calls).
const edges = new Map();
let currentNode = { name: '(top)' };
const recordEdge = (callee) => {
    const caller = currentNode.name;
    let perCaller = edges.get(callee);
    if (!perCaller) { perCaller = new Map(); edges.set(callee, perCaller); }
    perCaller.set(caller, (perCaller.get(caller) || 0) + 1);
};

/// Wrap every compiled procedure on one thread, by wrapping the factory.
///
/// There are two shapes, and both are on the same map:
///
///   * a *plain* function (`fun34_cpu_step`) for a procedure that never yields
///     -- the body runs inside the call and there is no generator to step, so
///     the call itself is the thing to time;
///   * a *generator function* (`gen7_run_loop`) for one that does, whose body
///     runs when the caller iterates it.
///
/// The generated call site cannot tell them apart and does not try: it writes
/// `thread.procedures["Zrun_loop"](...)` for the second and lets `yield*` drive
/// it. So the wrapper has to time the call and, when what came back is a
/// generator, time the generator's own steps as well -- otherwise the yielding
/// half of the machine is invisible, which is exactly what an earlier version of
/// this file measured.
function wrapThread(thread) {
    if (!thread || !thread.procedures) return 0;
    let n = 0;
    for (const proccode of Object.keys(thread.procedures)) {
        const factory = thread.procedures[proccode];
        if (typeof factory !== 'function' || instrumented.has(factory)) continue;
        instrumented.add(factory);
        const s = stat(prettyName(proccode));
        const wrappedFactory = function (...rest) {
            const previous = currentNode;
            // The edge is recorded at *call* time, while the caller is still on
            // top of the stack: reading it after the restore would attribute
            // every call to itself.
            recordEdge(s.name);
            const t0 = process.hrtime.bigint();
            currentNode = s;
            let result;
            try {
                result = factory.apply(this, rest);
            } finally {
                s.self += Number(process.hrtime.bigint() - t0);
                s.calls += 1;
                currentNode = previous;
            }
            if (!result || typeof result.next !== 'function' || result.__timed) {
                return result;
            }
            // A generator: the caller drives it, so its steps are timed too, and
            // the stack stays on this frame while it is being driven.
            result.__timed = true;
            const originalNext = result.next.bind(result);
            const originalThrow = result.throw && result.throw.bind(result);
            result.next = function (...nextArgs) {
                const prev = currentNode;
                currentNode = s;
                const t = process.hrtime.bigint();
                try {
                    return originalNext(...nextArgs);
                } finally {
                    s.self += Number(process.hrtime.bigint() - t);
                    currentNode = prev;
                }
            };
            if (originalThrow) {
                result.throw = function (...throwArgs) {
                    const prev = currentNode;
                    currentNode = s;
                    const t = process.hrtime.bigint();
                    try {
                        return originalThrow(...throwArgs);
                    } finally {
                        s.self += Number(process.hrtime.bigint() - t);
                        currentNode = prev;
                    }
                };
            }
            return result;
        };
        Object.defineProperty(wrappedFactory, 'name', { value: factory.name });
        thread.procedures[proccode] = wrappedFactory;
        n += 1;
    }
    wrappedCount += n;
    return n;
}

/// Wrap every compiled procedure on one thread, by wrapping the factory.
/// A thread only exists once the green flag has started it, and the Machine's
/// is the one whose slice dominates the boot.

const wrappedThreads = new WeakSet();
const wrapAllThreads = () => {
    let wrapped = 0;
    for (const thread of runtime.threads) {
        if (wrappedThreads.has(thread)) continue;
        const n = wrapThread(thread);
        if (n > 0) { wrappedThreads.add(thread); wrapped += n; }
    }
    if (wrapped > 0 && process.env.PROFILE_DEBUG) {
        originalWarn(`debug         wrapped ${wrapped} factory(ies); ` +
            runtime.threads.map((t) => `${t.target.getName()}:` +
                Object.keys(t.procedures || {}).length).join(', '));
    }
};

vm.greenFlag();
runtime.currentStepTime = 1000 / 30;
if (process.env.PROFILE_DEBUG) {
    originalWarn(`debug         greenFlag left ${runtime.threads.length} thread(s): ` +
        runtime.threads.map((t) => `${t.target.getName()}:` +
            (t.procedures ? Object.keys(t.procedures).length : 'none')).join(', '));
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const started = Date.now();
let steps = 0;
let booted = false;
const startInstr = Number(value(stage(), 'cpu_instructions')) || 0;
while (Date.now() - started < budgetMs) {
    runtime._step();
    steps += 1;
    // Every frame, because a hat that fires later starts a thread that did not
    // exist when the last one was walked.
    wrapAllThreads();
    if (until !== null) {
        const n = Number(value(stage(), 'cpu_instructions')) || 0;
        if (n >= until) break;
    } else if (consoleText().includes(bootMarker)) {
        booted = true;
        break;
    }
}
const secs = (Date.now() - started) / 1000;
const done = (Number(value(stage(), 'cpu_instructions')) || 0) - startInstr;

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

const totalSelf = [...stats.values()].reduce((a, s) => a + s.self, 0);
const rows = [...stats.values()].filter((s) => s.self > 0).sort((a, b) => b.self - a.self);

/// Exclusive time, which is the number that names a lag source.
///
/// Inclusive time says what a procedure would cost if it stopped calling
/// anything, and that makes every caller expensive: `cpu_step` is charged for
/// the whole instruction. Exclusive time takes each callee's own inclusive time
/// away from its caller's, so what is left is the code that is *only* in that
/// procedure -- and the sum of the exclusive times over the whole tree is the
/// run. A procedure with a large exclusive time is where the seconds are; one
/// that only looks large inclusively is a dispatcher.
///
/// The subtraction is done per call site rather than per procedure, because a
/// procedure's callees are known only from the call graph the run built. An
/// approximate version is enough to rank by: each procedure's inclusive total is
/// reduced by the inclusive totals of everything it called, counted once per
/// distinct caller/callee edge and scaled by the callee's calls per caller call.
const exclusive = new Map();
for (const s of stats.values()) exclusive.set(s.name, s.self);

/// Attribute each callee's inclusive total up its incoming edges, in the
/// proportion of the calls that came along each one. A procedure that is only
/// ever called from one place is removed from that caller exactly once; one
/// called in a loop is removed once per call, which is what makes the caller's
/// exclusive time its own code rather than its callee's.
for (const [calleeName, perCaller] of edges) {
    const callee = stats.get(calleeName);
    if (!callee) continue;
    const totalEdgeCalls = [...perCaller.values()].reduce((a, b) => a + b, 0);
    if (totalEdgeCalls === 0) continue;
    for (const [callerName, n] of perCaller) {
        if (!exclusive.has(callerName)) continue;
        exclusive.set(callerName,
            exclusive.get(callerName) - callee.self * (n / totalEdgeCalls));
    }
}

originalWarn(`file          ${path.relative(process.cwd(), sb3)}`);
originalWarn(`vm            ${path.relative(repo, VM_ROOT)}${turbo ? ' (compiling)' : ' (interpreting)'}`);
originalWarn(`run           ${secs.toFixed(1)} s, ${steps} runtime steps, ` +
    `${booted ? 'reached /init' : `stopped at ${done} guest instructions`}`);
if (boot) {
    originalWarn(booted
        ? `BOOT          ${secs.toFixed(1)} s and ${done} guest instructions to ` +
          `"${bootMarker}"`
        : `BOOT          did not reach "${bootMarker}" in ${(budgetMs / 1000).toFixed(0)} s`);
}
originalWarn(`guest         ${done} instructions retired ` +
    `(${Math.round(done / secs)} a second)`);
originalWarn(`wrapped       ${wrappedCount} compiled procedure generator(s), ` +
    `${rows.length} of them ran`);
originalWarn('');
originalWarn('--- compiled procedures, by exclusive time');
originalWarn('    exclusive: the callee\'s time is taken out of the caller\'s, so these');
originalWarn('    are the seconds that are in this procedure and nowhere below it.');
originalWarn(`  ${'excl ms'.padStart(10)}  ${'% run'.padStart(7)}  ${'incl ms'.padStart(10)}  ` +
    `${'calls'.padStart(14)}  ${'ns/call'.padStart(11)}  procedure`);
const byExclusive = [...exclusive.entries()]
    .map(([name, ex]) => ({ name, ex, s: stat(name) }))
    .filter((r) => r.ex > 0)
    .sort((a, b) => b.ex - a.ex);
for (const r of byExclusive.slice(0, top)) {
    originalWarn(`  ${(r.ex / 1e6).toFixed(1).padStart(10)}  ` +
        `${(r.ex / (secs * 1e9) * 100).toFixed(1).padStart(6)}%  ` +
        `${(r.s.self / 1e6).toFixed(1).padStart(10)}  ` +
        `${String(r.s.calls).padStart(14)}  ${(r.s.self / Math.max(1, r.s.calls)).toFixed(0).padStart(11)}  ${r.name}`);
}
originalWarn('');
originalWarn(`instrumented  ${(totalSelf / 1e6).toFixed(1)} ms of inclusive time across ` +
    `${rows.length} procedure(s)`);
originalWarn(`run           ${(secs * 1000).toFixed(1)} ms of wall clock, of which ` +
    `cpu_step accounts for ${((stat('cpu_step').self) / 1e6).toFixed(1)} ms ` +
    `(${(stat('cpu_step').self / (secs * 1e9) * 100).toFixed(1)}%) and ` +
    `monitor_draw for ${((stat('monitor_draw').self) / 1e6).toFixed(1)} ms ` +
    `(${(stat('monitor_draw').self / (secs * 1e9) * 100).toFixed(1)}%)`);
originalWarn('');
originalWarn('--- compiled procedures, by inclusive time');
originalWarn(`  ${'incl ms'.padStart(10)}  ${'% run'.padStart(7)}  ${'calls'.padStart(14)}  ` +
    `${'ns/call'.padStart(11)}  procedure`);
for (const s of rows.slice(0, top)) {
    const ms = s.self / 1e6;
    originalWarn(`  ${ms.toFixed(1).padStart(10)}  ${(ms / (secs * 1000) * 100).toFixed(1).padStart(6)}%  ` +
        `${String(s.calls).padStart(14)}  ${(s.self / Math.max(1, s.calls)).toFixed(0).padStart(11)}  ${s.name}`);
}

// The interpreted census, for the display half of the question.
const framesAt = Number(value(stage(), 'monitor_frames'));
const runsAt = Number(value(stage(), 'monitor_runs'));
const readsAt = Number(value(stage(), 'monitor_reads'));
const counts = new Map();
const real = runtime.getOpcodeFunction.bind(runtime);
let counting = false;
runtime.getOpcodeFunction = function (opcode) {
    const fn = real(opcode);
    if (typeof fn !== 'function') return fn;
    return function (...rest) {
        if (counting) counts.set(opcode, (counts.get(opcode) || 0) + 1);
        return fn.apply(this, rest);
    };
};
if (turbo) {
    runtime.compilerOptions.enabled = false;
    for (const thread of runtime.threads) thread.isCompiled = false;
}
const profileSteps = Number(process.env.PROFILE_STEPS || 3);
counting = true;
for (let i = 0; i < profileSteps; i++) runtime._step();
counting = false;
const passes = Number(value(stage(), 'monitor_frames')) - framesAt;
const runs = Number(value(stage(), 'monitor_runs')) - runsAt;
const reads = Number(value(stage(), 'monitor_reads')) - readsAt;
const total = [...counts.values()].reduce((a, b) => a + b, 0);
originalWarn('');
originalWarn(`--- interpreted blocks over ${profileSteps} frame(s), ${passes} whole-panel pass(es)`);
originalWarn(`  ${String(total).padStart(12)} blocks, ${String(runs).padStart(10)} runs, ` +
    `${String(reads).padStart(12)} pixels read`);
for (const [opcode, n] of [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20)) {
    originalWarn(`  ${String(n).padStart(12)}  ${String(Math.round(n / Math.max(1, passes))).padStart(10)}/pass  ${opcode}`);
}
