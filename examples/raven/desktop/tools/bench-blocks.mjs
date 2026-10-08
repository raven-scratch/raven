#!/usr/bin/env node
// What one Scratch 3 block costs, in a real VM, one block category at a time.
//
//     node examples/raven/desktop/tools/bench-blocks.mjs
//     node examples/raven/desktop/tools/bench-blocks.mjs --reps 3 --json out.json
//     node examples/raven/desktop/tools/bench-blocks.mjs --only 'list|pen'
//
// ## Method
//
// **The VM.** TurboWarp's compiled `scratch-vm`, resolved the way every other
// tool in this directory resolves it: `$SCRATCH_VM_ROOT`, then
// `ref/turbowarp-vm`, then `ref/scratch-vm/node_modules/scratch-vm`, taking the
// first that has `src/virtual-machine.js`. `ref/turbowarp-vm` is present and is
// version 2.1.46. The bootstrap is `tools/check-rv32.mjs`'s: the
// `@scratch/scratch-svg-renderer` module stub through `Module._load`,
// `globalThis.document = {hidden: true}`, and a renderer implementing the
// renderer interface.
//
// **The compiler is ON.** `runtime.compilerOptions.enabled` is TurboWarp's
// default and this tool never turns it off, because that is the mode the
// desktop project's checks run in and therefore the mode that matters. Four
// consequences are reported rather than hidden:
//
//   * A block TurboWarp compiles natively is a handful of JavaScript
//     operations; a block it does not is routed through the *compatibility
//     layer*, which builds a BlockUtility object per call. Both are real costs
//     of the same program and both are in the table; `src/compiler/jsgen.js`
//     and `src/compiler/enums.js` say which is which. Motion, pen, list,
//     variable, every operator, control, procedures and `sensing_timer` are
//     native in this version.
//   * A non-warp `repeat` yields once an iteration -- `jsgen.js:1036`
//     `yieldNotWarp` writes a `yield` into the compiled loop -- while inside a
//     warp script `jsgen.js:1046` `yieldStuckOrNotWarp` only yields if
//     `isStuck()`, and `compilerOptions.warpTimer` is false by default. That
//     difference is a row of the table (`yield_frame_overhead`), measured by
//     timing the same empty loop both ways.
//   * A literal that is a numeric *string* is cast at compile time.
//     `operator_add` types its operands `InputType.NUMBER`
//     (`src/compiler/irgen.js:311`) and a constant folds to a number, so
//     `"12" + 3` compiles to `12 + 3`. Coercion is only paid where the
//     operand's type is not known at compile time -- a variable, a parameter,
//     a list item -- which is what the `_item_` rows measure: the same compiled
//     expression, over a list item that holds a number and over one that holds
//     a numeric string.
//   * TurboWarp's own IR optimizer (`src/compiler/iroptimizer.js`) tracks
//     variable types around a loop and deletes casts it can prove unnecessary,
//     so a variable that only ever holds numbers is a number operand and not a
//     coercion.
//
// **What is timed, and why the loop body looks the way it does.**
// `vm.greenFlag()` followed by `runtime._step()` until the stage variable
// `bench_done` is 1, wall clock, with `performance.now()`. The loop overhead is
// measured the same way -- a `repeat` whose body is empty -- and subtracted.
// Two things about the body are deliberate, because a micro-benchmark of a
// two-operation JavaScript expression measures V8's optimiser instead of the
// block if you are careless:
//
//   * Every body **increments a tick variable first** (`change bench_i by 1`).
//     Without it an expression built only from variables and constants is
//     loop-invariant, and V8 hoists it out of the loop: measuring it gives
//     zero. With it, anything the construct computes depends on a value that
//     changes every iteration.
//   * Every reporter is measured as **the operand of an accumulator**
//     (`change bench_tmp by <expr>`), not as the value of a `set`. A reporter
//     with no consumer is dead code the compiler drops; and a `set` whose
//     result nothing reads is a store V8 sinks out of the loop, because the
//     value is overwritten before anything observes it. `change ... by` *reads*
//     the variable it writes, so its store is carried from one iteration to the
//     next and cannot be removed. That is why `set_var` measures near zero here
//     and the accumulator row does not: the measurement is being honest about
//     what a hot compiled loop does to a store nobody reads, and it is why
//     every reporter row is taken through the accumulator and subtracts it.
//
// Both fixes were forced by measurement, not assumed: with a constant store and
// no tick the same construct came back at 0.5 ns/op, which is the empty loop's
// own cost, and with the tick and the accumulator it comes back at the
// 1-30 ns/op the rows below show.
//
// **The project.** A synthetic one, built as a plain JavaScript object and
// handed straight to `vm.loadProject(object)`. TurboWarp's
// `virtual-machine.js:458` JSON-stringifies an object input and validates it as
// a project, and the deserializer asks the zip for *assets* only: a project
// with no costumes, no sounds and one `pen` extension needs no zip at all. (A
// costume without a zip fails -- the costume loader asks for an asset no zip
// can supply -- so the sprites here carry no costumes.) Each benchmark loads
// its own project into one long-lived VM: one sprite, a `whenflagclicked` hat
// that runs only the construct under test inside a `repeat bench_n`, then
// `bench_done = 1`. The block JSON is written in the shapes the built desktop
// project uses, read out of it rather than guessed.
//
// **Repetitions and noise.** Every benchmark is run `--reps` times (5 by
// default) and two readings are reported: the **median**, which is what the
// machine charged on average, and the **minimum**, the fastest run, which is the
// one that survives a machine another process is also using. The **noise figure
// is `(max - min) / median`, as a percentage**, over the same set of runs. The
// median is the headline; the minimum is the column to compare rows by when the
// noise column is large. Note what that means at this scale: the cheap rows are
// a few JavaScript operations inside a loop V8 has optimised, so a loaded
// machine can put 30 per cent of spread on a row, and differences below about
// one nanosecond are not resolvable here. A row whose net is smaller than its
// own spread -- or is negative, which no block can be -- is marked `~` and
// should be read as "about zero".
//
// **Iterations per measurement** are chosen per benchmark, not guessed: a pilot
// run of `pilot` iterations is timed, then `N` is scaled so the run takes about
// `--target-ms` (200 ms by default), clamped to that benchmark's own floor and
// ceiling. Sizing every row to the same wall-clock time also holds V8 at the
// same optimisation tier from row to row, which a fixed iteration count would
// not. A few benchmarks cap `N` by nature rather than by time: `add to list`
// would otherwise grow the list to hundreds of millions of items, and `item # of`
// scans the whole list.
//
// **The unit.** `bare_block` is one `motion_changexby 1`, and it is the "1" of
// the `rel(bare)` column. It is a movement block and not a variable write
// because a movement block also moves a drawable and reports the move to the
// runtime, which is where most of its hundred-odd nanoseconds go; the unit is
// therefore "one block that does something", not "one JavaScript assignment".
// `set_var` and `acc_base` are reported beside it, and the raw JSON carries a
// `relToSetVar` for each row.
//
// **What a subtracted row is worth.** A row's uncertainty is its own spread
// plus the spread of everything taken off it, and the table says so: a row
// whose net is smaller than that accumulated uncertainty is marked `~` and
// should be read as "about zero", not as a small number. This matters more than
// it looks, because `bare_block` itself carries tens of per cent of spread on a
// loaded machine, so a row that subtracts it inherits all of it. The whole run
// takes minutes, and the machine's speed drifts over that time; a `_num` row
// and its `_str` twin are measured next to each other for that reason, and the
// runs are sized to the same wall-clock time so that V8 is in the same tier for
// each.
//
// The pen is measured with a renderer whose `penLine`/`penPoint`/`penClear`
// count calls and touch no pixels: the pen rows are therefore what the *VM*
// charges for a pen block, with the rasteriser's own cost deliberately
// excluded, and the desktop project's real rasteriser sits on top of them.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');

// ---------------------------------------------------------------------------
// The VM, bootstrapped the way check-rv32.mjs does it
// ---------------------------------------------------------------------------

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
const vmVersion = (() => {
    try {
        return JSON.parse(fs.readFileSync(path.join(VM_ROOT, 'package.json'), 'utf8')).version;
    } catch (e) {
        return 'unknown';
    }
})();
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

/// Everything the VM reports as an error, so a benchmark that silently fell
/// back to the interpreter -- `thread.js:502` logs `cannot compile script` --
/// cannot pass for a compiled one.
const vmErrors = [];
try {
    const log = require_(path.join(VM_ROOT, 'src', 'util', 'log.js'));
    const realError = log.error;
    log.error = (...a) => {
        vmErrors.push(a.map((x) => (x && x.stack) || String(x)).join(' '));
        if (typeof realError === 'function') realError.apply(log, a);
    };
} catch (e) { /* a VM without that module can still benchmark */ }

// The table goes to stdout, so the VM's chatter goes nowhere and its console
// errors are collected instead of printed.
const out = console.log.bind(console);
const progress = (line) => process.stderr.write(line + '\n');
console.warn = () => {};
console.error = (...a) => { vmErrors.push(a.map(String).join(' ')); };

/// A renderer that implements the interface and does nothing. The pen is the
/// one place where "do nothing" changes what is being measured, and it is on
/// purpose: `penLine`/`penPoint`/`penClear` here count calls and touch no
/// pixels, so the pen rows are the VM's own charge for a pen block -- block,
/// extension, state, and the call into the renderer -- with the rasteriser's
/// cost deliberately excluded.
function noopRenderer() {
    let nextId = 1;
    const pen = { lines: 0, points: 0, clears: 0, stamps: 0 };
    const box = { left: 0, right: 0, top: 0, bottom: 0 };
    return {
        pen,
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
        createDrawable() { return nextId++; },
        destroyDrawable() {},
        updateDrawableSkinId() {},
        updateDrawablePosition() {},
        updateDrawableDirectionScale() {},
        updateDrawableVisible() {},
        updateDrawableEffect() {},
        setDrawableOrder() {},
        getDrawableOrder() { return 0; },
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return box; },
        getBoundsForBubble() { return box; },
        pick() { return -1; },
        drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; },
        drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; },
        isTouchingDrawables() { return false; },
        penClear() { pen.clears++; },
        penStamp() { pen.stamps++; },
        penLine() { pen.lines++; },
        penPoint() { pen.points++; },
        draw() {}
    };
}

// ---------------------------------------------------------------------------
// Building a project out of blocks
// ---------------------------------------------------------------------------
//
// Scratch input primitives: `[1, [4, "5"]]` is a number, `[1, [10, "a"]]` a
// string, `[1, [9, "#ff0000"]]` a colour, `[3, id, [4, ""]]` a reporter in a
// value slot, `[2, id]` a reporter in a boolean slot, and a substack is `[2,
// id]` with no shadow. These are the shapes the built desktop project uses.

const NUM = (v) => [1, [4, String(v)]];
const TEXT = (v) => [1, [10, String(v)]];
const COLOR = (v) => [1, [9, String(v)]];

class Builder {
    constructor() {
        this.blocks = {};
        this.next = 0;
        this.topLevel = [];
        this.vars = new Map();
        this.lists = new Map();
        this.procs = new Map();
        this.extensions = new Set(['pen']);
    }

    id() { return 'k' + (++this.next); }
    varId(name) { return 'vid_' + name; }
    listId(name) { return 'lid_' + name; }

    var(name, value) { this.vars.set(name, value); return this; }
    list(name, values) { this.lists.set(name, values); return this; }

    block(opcode, { fields = {}, inputs = {}, mutation = null, shadow = false, topLevel = false } = {}) {
        const id = this.id();
        this.blocks[id] = { opcode, next: null, parent: null, inputs, fields, shadow, topLevel };
        if (mutation) this.blocks[id].mutation = mutation;
        return id;
    }
    reporter(opcode, fields = {}, inputs = {}) { return this.block(opcode, { fields, inputs }); }
    statement(opcode, fields = {}, inputs = {}) { return this.block(opcode, { fields, inputs }); }

    inValue(id) { return [3, id, [4, '']]; }
    inBool(id) { return [2, id]; }
    substack(first) { return [2, first === undefined || first === null ? null : first]; }

    chain(ids) {
        const list = (ids || []).filter((x) => x !== null && x !== undefined);
        for (let i = 0; i < list.length; i++) {
            const b = this.blocks[list[i]];
            b.next = i + 1 < list.length ? list[i + 1] : null;
            if (i > 0) b.parent = list[i - 1];
        }
        return list.length ? list[0] : null;
    }

    hat(opcode) { return this.block(opcode, { topLevel: true }); }

    // -- variables ----------------------------------------------------------
    varGet(name) { return this.reporter('data_variable', { VARIABLE: [name, this.varId(name)] }); }
    varValue(name) { return this.inValue(this.varGet(name)); }
    varSet(name, valueInput) {
        return this.statement('data_setvariableto', { VARIABLE: [name, this.varId(name)] }, { VALUE: valueInput });
    }
    varChange(name, valueInput) {
        return this.statement('data_changevariableby', { VARIABLE: [name, this.varId(name)] }, { VALUE: valueInput });
    }

    // -- lists --------------------------------------------------------------
    listFields(name) { return { LIST: [name, this.listId(name)] }; }
    listGet(name, indexInput) { return this.reporter('data_itemoflist', this.listFields(name), { INDEX: indexInput }); }
    listReplace(name, indexInput, itemInput) {
        return this.statement('data_replaceitemoflist', this.listFields(name), { INDEX: indexInput, ITEM: itemInput });
    }
    listAdd(name, itemInput) {
        return this.statement('data_addtolist', this.listFields(name), { ITEM: itemInput });
    }
    listLength(name) { return this.reporter('data_lengthoflist', this.listFields(name)); }
    listIndexOf(name, itemInput) { return this.reporter('data_itemnumoflist', this.listFields(name), { ITEM: itemInput }); }
    /// `(bench_i % m) + offset` as a reporter id: an index that never sits still,
    /// so a read through it cannot be hoisted out of the loop.
    cyclicIndex(m, offset) {
        return this.add(this.inValue(this.mod(this.inValue(this.varGet('bench_i')), NUM(m))), NUM(offset));
    }
    /// `item indexId of list` as a value input.
    itemInput(name, indexId) { return this.inValue(this.listGet(name, this.inValue(indexId))); }

    // -- operators ----------------------------------------------------------
    op(opcode, inputs) { return this.reporter(opcode, {}, inputs); }
    add(a, b) { return this.op('operator_add', { NUM1: a, NUM2: b }); }
    sub(a, b) { return this.op('operator_subtract', { NUM1: a, NUM2: b }); }
    mul(a, b) { return this.op('operator_multiply', { NUM1: a, NUM2: b }); }
    div(a, b) { return this.op('operator_divide', { NUM1: a, NUM2: b }); }
    mod(a, b) { return this.op('operator_mod', { NUM1: a, NUM2: b }); }
    round(a) { return this.op('operator_round', { NUM: a }); }
    mathop(operator, a) { return this.reporter('operator_mathop', { OPERATOR: [operator] }, { NUM: a }); }
    equals(a, b) { return this.op('operator_equals', { OPERAND1: a, OPERAND2: b }); }
    less(a, b) { return this.op('operator_lt', { OPERAND1: a, OPERAND2: b }); }
    greater(a, b) { return this.op('operator_gt', { OPERAND1: a, OPERAND2: b }); }
    and(a, b) { return this.op('operator_and', { OPERAND1: a, OPERAND2: b }); }
    or(a, b) { return this.op('operator_or', { OPERAND1: a, OPERAND2: b }); }
    not(a) { return this.op('operator_not', { OPERAND: a }); }
    join(a, b) { return this.op('operator_join', { STRING1: a, STRING2: b }); }
    letterOf(letter, string) { return this.op('operator_letter_of', { LETTER: letter, STRING: string }); }

    // -- control ------------------------------------------------------------
    repeat(times, body) {
        return this.statement('control_repeat', {}, { TIMES: times, SUBSTACK: this.substack(body) });
    }
    repeatUntil(condition, body) {
        return this.statement('control_repeat_until', {}, { CONDITION: condition, SUBSTACK: this.substack(body) });
    }
    ifBlock(condition, body) {
        return this.statement('control_if', {}, { CONDITION: condition, SUBSTACK: this.substack(body) });
    }
    ifElse(condition, whenTrue, whenFalse) {
        return this.statement('control_if_else', {}, {
            CONDITION: condition, SUBSTACK: this.substack(whenTrue), SUBSTACK2: this.substack(whenFalse)
        });
    }

    // -- procedures ---------------------------------------------------------
    procedure(name, { params = [], warp = false, body = [] } = {}) {
        const proccode = params.length ? `${name} ${params.map(() => '%s').join(' ')}` : name;
        const argIds = params.map((_, i) => `arg_${name}_${i}`);
        const defId = this.id();
        const protoId = this.id();
        this.blocks[protoId] = {
            opcode: 'procedures_prototype', next: null, parent: defId, inputs: {}, fields: {}, shadow: true, topLevel: false,
            mutation: {
                tagName: 'mutation', children: [], proccode,
                argumentids: JSON.stringify(argIds),
                argumentnames: JSON.stringify(params),
                argumentdefaults: JSON.stringify(params.map(() => '')),
                warp: String(warp)
            }
        };
        this.blocks[defId] = {
            opcode: 'procedures_definition', next: this.chain(body), parent: null,
            inputs: { custom_block: [1, protoId] }, fields: {}, shadow: false, topLevel: true
        };
        this.topLevel.push(defId);
        const info = { proccode, argIds, params, warp };
        this.procs.set(name, info);
        return info;
    }
    call(name, args = []) {
        const p = this.procs.get(name);
        const inputs = {};
        p.argIds.forEach((id, i) => { inputs[id] = args[i]; });
        return this.block('procedures_call', {
            inputs,
            mutation: {
                tagName: 'mutation', children: [], proccode: p.proccode,
                argumentids: JSON.stringify(p.argIds), warp: String(p.warp)
            }
        });
    }
    argReporter(name) { return this.reporter('argument_reporter_string_number', { VALUE: [name] }); }

    // -- motion and pen -----------------------------------------------------
    changeX(dx) { return this.statement('motion_changexby', {}, { DX: dx }); }
    changeY(dy) { return this.statement('motion_changeyby', {}, { DY: dy }); }
    setX(x) { return this.statement('motion_setx', {}, { X: x }); }
    goToXY(x, y) { return this.statement('motion_gotoxy', {}, { X: x, Y: y }); }
    penClear() { return this.statement('pen_clear'); }
    penDown() { return this.statement('pen_penDown'); }
    penUp() { return this.statement('pen_penUp'); }
    penSize(size) { return this.statement('pen_setPenSizeTo', {}, { SIZE: size }); }
    penColor(color) { return this.statement('pen_setPenColorToColor', {}, { COLOR: color }); }

    // -- sensing ------------------------------------------------------------
    timer() { return this.reporter('sensing_timer'); }

    project() {
        const stage = {
            isStage: true, name: 'Stage', variables: {}, lists: {}, broadcasts: {}, blocks: {}, comments: {},
            currentCostume: 0, costumes: [], sounds: [], volume: 100, layerOrder: 0, tempo: 60,
            videoTransparency: 50, videoState: 'on', textToSpeechLanguage: null
        };
        for (const [name, value] of this.vars) stage.variables[this.varId(name)] = [name, value];
        for (const [name, value] of this.lists) stage.lists[this.listId(name)] = [name, value];
        return {
            targets: [stage, {
                isStage: false, name: 'Bench', variables: {}, lists: {}, broadcasts: {}, blocks: this.blocks,
                comments: {}, currentCostume: 0, costumes: [], sounds: [], volume: 100, layerOrder: 1,
                visible: true, x: 0, y: 0, size: 100, direction: 90, draggable: false, rotationStyle: 'all around'
            }],
            monitors: [], extensions: [...this.extensions], meta: { semver: '3.0.0', vm: '0.2.0', agent: '' }
        };
    }
}

// ---------------------------------------------------------------------------
// The benchmarks
// ---------------------------------------------------------------------------
//
//   name     the row's name
//   note     what was measured, in words
//   loop     'warp' -- the loop is inside a warp procedure, so it compiles to a
//            plain JavaScript for loop; 'yield' -- the loop is non-warp, and
//            yields once an iteration. The calibration subtracted is the empty
//            loop of the same kind.
//   minus    benchmarks whose net cost is subtracted from this row
//   size     iteration sizing: pilot / targetMs / min / max
//   setup(B) extra variables or lists this benchmark needs
//   build(B) defines procedures and returns { body, before }
//
// The shared names, so the `minus` lists below read:
//   tick        change bench_i by 1
//   acc_base    change bench_tmp by bench_i   (tick subtracted) -- the consumer
//               every reporter row is measured through
//   set_var     set bench_tmp to bench_i      (tick subtracted)
//   bare_block  change x by 1                 (tick subtracted)
//   expr_index_low   the index arithmetic (bench_i % 100) + 1
//   list_read_low    that index read out of list_big
//   op_join     join(bench_i, "")

const BENCHES = [];
const bench = (spec) => { BENCHES.push(spec); return spec; };

const TICK = 'tick';
const ACC = 'acc_base';
const SET_VAR = 'set_var';
const BARE = 'bare_block';
const JOIN = 'op_join';
const IDX_LOW = 'expr_index_low';
const IDX_HIGH = 'expr_index_high';
const READ_LOW = 'list_read_low';

// ---- the loop overhead and the two reference points ----------------------
bench({
    name: 'repeat_empty',
    category: 'calibration',
    note: 'one iteration of an empty warp repeat: the loop machinery every warp row pays',
    loop: 'warp',
    build: () => ({ body: [] })
});
bench({
    name: 'yield_repeat_empty',
    category: 'calibration',
    note: 'one iteration of an empty non-warp repeat: the loop machinery plus the yield (nothing asks for a redraw, so the sequencer resumes it at once)',
    loop: 'yield',
    size: { pilot: 20000, targetMs: 100, min: 20000, max: 500000 },
    build: () => ({ body: [] })
});
bench({
    name: TICK,
    category: 'calibration',
    note: 'one data_changevariableby: the tick every body starts with',
    loop: 'warp',
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1))] })
});
bench({
    name: BARE,
    category: 'unit',
    note: 'one motion_changexby 1 -- the unit of rel(bare)',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.changeX(NUM(1))] })
});
bench({
    name: ACC,
    category: 'unit',
    note: 'data_changevariableby to an accumulator that reads itself: the consumer every reporter row goes through',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.varValue('bench_i'))] })
});
bench({
    name: SET_VAR,
    category: 'unit',
    note: 'one data_setvariableto of a loop-variant value',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.varSet('bench_tmp', B.varValue('bench_i'))] })
});

// ---- procedures ----------------------------------------------------------
bench({
    name: 'proc_call_0',
    category: 'procedure',
    note: 'a warp procedure call whose body is one change x by 1, the callee block included; `proc_call_0_overhead` is this less the bare block',
    loop: 'warp',
    minus: [TICK],
    build: (B) => {
        B.procedure('bench_zero', { warp: true, body: [B.changeX(NUM(1))] });
        return { body: [B.varChange('bench_i', NUM(1)), B.call('bench_zero')] };
    }
});
bench({
    name: 'proc_call_0_plain',
    category: 'procedure',
    note: 'the same call to a non-warp procedure from a non-warp loop: the callee moves the sprite, so every call asks for a redraw and ends the runtime step',
    loop: 'yield',
    minus: [TICK],
    size: { pilot: 20000, targetMs: 100, min: 20000, max: 500000 },
    build: (B) => {
        B.procedure('bench_plain0', { warp: false, body: [B.changeX(NUM(1))] });
        return { body: [B.varChange('bench_i', NUM(1)), B.call('bench_plain0')] };
    }
});
bench({
    name: 'proc_call_2',
    category: 'procedure',
    note: 'the same call with two numeric parameters whose body reads both and adds them, the callee block included',
    loop: 'warp',
    minus: [TICK],
    build: (B) => {
        B.procedure('bench_two', {
            params: ['p_a', 'p_b'], warp: true,
            body: [B.changeX(B.inValue(B.add(B.inValue(B.argReporter('p_a')), B.inValue(B.argReporter('p_b')))))]
        });
        return { body: [B.varChange('bench_i', NUM(1)), B.call('bench_two', [NUM(7), NUM(9)])] };
    }
});

// ---- loops ---------------------------------------------------------------
bench({
    name: 'repeat_until_counter',
    category: 'control',
    note: 'repeat until bench_i > bench_n, incrementing bench_i (the compare and the increment are subtracted; the body block is still in it)',
    loop: 'warp',
    minus: [TICK, 'cmp_gt_num'],
    build: (B) => ({
        before: [B.varSet('bench_i', NUM(0))],
        body: [B.repeatUntil(
            B.inBool(B.greater(B.varValue('bench_i'), B.varValue('bench_n'))),
            B.chain([B.changeX(NUM(1)), B.varChange('bench_i', NUM(1))])
        )]
    })
});

// ---- lists ---------------------------------------------------------------
// The index sweeps a band rather than sitting still: a constant index would let
// V8 hoist the read out of the loop, and a band is what the monitor does anyway.
bench({
    name: IDX_LOW,
    category: 'list',
    note: 'the index arithmetic (bench_i % 100) + 1 alone',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.cyclicIndex(100, 1)))]
    })
});
bench({
    name: IDX_HIGH,
    category: 'list',
    note: 'the index arithmetic (bench_i % 100) + 4901 alone',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.cyclicIndex(100, 4901)))]
    })
});
bench({
    name: READ_LOW,
    category: 'list',
    note: 'data_itemoflist at indices 1..100 of a 10000-item list',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.itemInput('list_big', B.cyclicIndex(100, 1)))]
    })
});
bench({
    name: 'list_read_high',
    category: 'list',
    note: 'data_itemoflist at indices 4901..5000 of the same list',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.itemInput('list_big', B.cyclicIndex(100, 4901)))]
    })
});
bench({
    name: 'list_write_low',
    category: 'list',
    note: 'data_replaceitemoflist at indices 1..100',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.listReplace('list_big', B.inValue(B.cyclicIndex(100, 1)), NUM(1))]
    })
});
bench({
    name: 'list_write_high',
    category: 'list',
    note: 'data_replaceitemoflist at indices 4901..5000',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.listReplace('list_big', B.inValue(B.cyclicIndex(100, 4901)), NUM(1))]
    })
});
bench({
    name: 'list_add',
    category: 'list',
    note: 'data_addtolist on a list emptied before each run (growth capped at N = 200000)',
    loop: 'warp',
    size: { pilot: 20000, targetMs: 200, min: 50000, max: 200000 },
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.listAdd('rv_reg', NUM(1))] })
});
bench({
    name: 'list_length',
    category: 'list',
    note: 'data_lengthoflist of a list that grows each iteration, so the length is not hoisted',
    loop: 'warp',
    minus: [TICK, 'list_add', ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.listAdd('rv_reg', NUM(1)),
            B.varChange('bench_tmp', B.inValue(B.listLength('rv_reg')))]
    })
});
bench({
    name: 'list_index_of',
    category: 'list',
    note: 'data_itemnumoflist for an item not in the list: a full 10000-item scan',
    loop: 'warp',
    minus: [TICK, ACC],
    size: { pilot: 50, targetMs: 200, min: 50, max: 4000 },
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)),
            B.varChange('bench_tmp', B.inValue(B.listIndexOf('list_big', B.varValue('bench_i'))))]
    })
});

// ---- arithmetic on numbers ----------------------------------------------
bench({
    name: 'op_add_num',
    category: 'operator',
    note: 'operator_add of a number variable and a number literal (bench_i + 3)',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)),
            B.varChange('bench_tmp', B.inValue(B.add(B.varValue('bench_i'), NUM(3))))]
    })
});
bench({
    name: 'op_add_lit_str',
    category: 'operator',
    note: 'operator_add of a number variable and a numeric-string literal (bench_i + "3"), which the compiler folds',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)),
            B.varChange('bench_tmp', B.inValue(B.add(B.varValue('bench_i'), TEXT('3'))))]
    })
});
for (const [name, fn, literal, opcode] of [
    ['op_sub_num', 'sub', 3, 'operator_subtract'],
    ['op_mul_num', 'mul', 3, 'operator_multiply'],
    ['op_div_num', 'div', 3, 'operator_divide'],
    ['op_mod_num', 'mod', 5, 'operator_mod']
]) {
    bench({
        name,
        category: 'operator',
        note: `${opcode} of a number variable and a literal`,
        loop: 'warp',
        minus: [TICK, ACC],
        build: (B) => ({
            body: [B.varChange('bench_i', NUM(1)),
                B.varChange('bench_tmp', B.inValue(B[fn](B.varValue('bench_i'), NUM(literal))))]
        })
    });
}
bench({
    name: 'op_round_num',
    category: 'operator',
    note: 'operator_round of a number variable',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.round(B.varValue('bench_i'))))]
    })
});
bench({
    name: 'op_floor_num',
    category: 'operator',
    note: 'operator_mathop floor of a number variable',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.mathop('floor', B.varValue('bench_i'))))]
    })
});

// ---- arithmetic where the operand's type is not known at compile time -----
// Two lists of a hundred items with the same shape, one holding numbers and one
// holding the same values as numeric strings. The compiled expression is
// identical -- a list item is typed ANY, so the cast is emitted either way --
// and only the runtime type differs, which is exactly Scratch's coercion.
bench({
    name: 'op_add_item_num',
    category: 'coercion',
    note: 'operator_add of a list item holding a number and 3',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.add(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(3))))]
    })
});
bench({
    name: 'op_add_item_str',
    category: 'coercion',
    note: 'operator_add of a list item holding a numeric string and 3 -- the same compiled code, coerced at run time',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.add(B.itemInput('str100', B.cyclicIndex(100, 1)), NUM(3))))]
    })
});
bench({
    name: 'op_mod_item_num',
    category: 'coercion',
    note: 'operator_mod of a list item holding a number and 5',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.mod(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(5))))]
    })
});
bench({
    name: 'op_mod_item_str',
    category: 'coercion',
    note: 'operator_mod of a list item holding a numeric string and 5',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.mod(B.itemInput('str100', B.cyclicIndex(100, 1)), NUM(5))))]
    })
});
bench({
    name: 'op_floor_item_num',
    category: 'coercion',
    note: 'operator_mathop floor of a list item holding a number',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.mathop('floor', B.itemInput('num100', B.cyclicIndex(100, 1)))))]
    })
});
bench({
    name: 'op_floor_item_str',
    category: 'coercion',
    note: 'operator_mathop floor of a list item holding a numeric string',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.mathop('floor', B.itemInput('str100', B.cyclicIndex(100, 1)))))]
    })
});

// ---- strings -------------------------------------------------------------
bench({
    name: JOIN,
    category: 'operator',
    note: 'operator_join of a number variable and an empty string, which makes a fresh string each iteration',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)),
            B.varChange('bench_tmp', B.inValue(B.join(B.varValue('bench_i'), TEXT(''))))]
    })
});
bench({
    name: 'op_letter_of',
    category: 'operator',
    note: 'operator_letter_of letter 1 of a list item holding a string (the read is subtracted)',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.letterOf(NUM(1), B.itemInput('str100', B.cyclicIndex(100, 1)))))]
    })
});

// ---- comparison and boolean ---------------------------------------------
// Each comparison reads its operand from a list, exactly as the coercion rows
// do, so that a `_num` row and a `_str` row differ only in the runtime type of
// the value: a list item is typed ANY, so the compiler emits the general
// comparison in both. `_str` against `_num` is therefore the string comparison
// -- Scratch lowercases both sides -- and nothing else.
bench({
    name: 'cmp_equals_num',
    category: 'operator',
    note: 'operator_equals of a list item holding a number and 50',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.equals(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))))]
    })
});
bench({
    name: 'cmp_equals_str',
    category: 'coercion',
    note: 'operator_equals of a list item holding a numeric string and "50" (lowercased, as Scratch does)',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.equals(B.itemInput('str100', B.cyclicIndex(100, 1)), TEXT('50'))))]
    })
});
bench({
    name: 'cmp_lt_num',
    category: 'operator',
    note: 'operator_lt of a list item holding a number and 50',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))))]
    })
});
bench({
    name: 'cmp_gt_num',
    category: 'operator',
    note: 'operator_gt of a list item holding a number and 50',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.greater(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))))]
    })
});
bench({
    name: 'cmp_lt_str',
    category: 'coercion',
    note: 'operator_lt of a list item holding a numeric string and "50"',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(
            B.less(B.itemInput('str100', B.cyclicIndex(100, 1)), TEXT('50'))))]
    })
});
bench({
    name: 'bool_and',
    category: 'operator',
    note: 'operator_and of two comparisons over the same list at indices with different moduli (the reads and both comparisons are subtracted)',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW, READ_LOW, 'cmp_lt_num', 'cmp_gt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.and(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))),
            B.inBool(B.greater(B.itemInput('num100', B.cyclicIndex(101, 2)), NUM(50))))))]
    })
});
bench({
    name: 'bool_or',
    category: 'operator',
    note: 'operator_or of two comparisons over the same list at indices with different moduli (the reads and both comparisons are subtracted)',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW, READ_LOW, 'cmp_lt_num', 'cmp_gt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.or(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))),
            B.inBool(B.greater(B.itemInput('num100', B.cyclicIndex(101, 2)), NUM(50))))))]
    })
});
bench({
    name: 'bool_not',
    category: 'operator',
    note: 'operator_not of a comparison over the same list (the read and the comparison are subtracted)',
    loop: 'warp',
    minus: [TICK, ACC, READ_LOW, 'cmp_lt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.not(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))))))]
    })
});

// ---- sensing -------------------------------------------------------------
bench({
    name: 'sensing_timer',
    category: 'sensing',
    note: 'sensing_timer as the value of a set: a call into the VM clock, not a variable',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.timer()))]
    })
});

// ---- the monitor's own expression shapes ---------------------------------
// `efb_words[base + k + 1] % 65536` and `floor(efb_words[...] / 65536) % 65536`
// is what the desktop monitor costs per scanned pixel. The `% 5000` keeps the
// index inside the 10000-item list while still varying every iteration.
bench({
    name: 'expr_efb_index',
    category: 'monitor',
    note: 'the monitor\'s index expression (bench_i % 5000) + bench_k + 1 on its own',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.add(
            B.inValue(B.add(B.inValue(B.mod(B.inValue(B.varGet('bench_i')), NUM(5000))), B.varValue('bench_k'))),
            NUM(1))))]
    })
});
bench({
    name: 'expr_efb_mod',
    category: 'monitor',
    note: 'efb_words[(bench_i % 5000) + bench_k + 1] mod 65536, one pixel\'s low half',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => {
        const index = B.add(B.inValue(B.add(B.inValue(B.mod(B.inValue(B.varGet('bench_i')), NUM(5000))), B.varValue('bench_k'))), NUM(1));
        return {
            body: [B.varChange('bench_i', NUM(1)),
                B.varChange('bench_tmp', B.inValue(B.mod(B.itemInput('efb_words', index), NUM(65536))))]
        };
    }
});
bench({
    name: 'expr_efb_floor',
    category: 'monitor',
    note: 'floor(efb_words[(bench_i % 5000) + bench_k + 1] / 65536) mod 65536, one pixel\'s high half',
    loop: 'warp',
    minus: [TICK, ACC],
    build: (B) => {
        const index = B.add(B.inValue(B.add(B.inValue(B.mod(B.inValue(B.varGet('bench_i')), NUM(5000))), B.varValue('bench_k'))), NUM(1));
        const shifted = B.inValue(B.mathop('floor', B.inValue(B.div(B.itemInput('efb_words', index), NUM(65536)))));
        return {
            body: [B.varChange('bench_i', NUM(1)), B.varChange('bench_tmp', B.inValue(B.mod(shifted, NUM(65536))))]
        };
    }
});

// ---- control -------------------------------------------------------------
// The condition is the same list-item comparison the `cmp_` rows measure, so
// the comparison can be taken off and what is left is the branch itself.
bench({
    name: 'if_true_empty',
    category: 'control',
    note: 'control_if with a true condition and an empty body (the read and the comparison are subtracted)',
    loop: 'warp',
    minus: [TICK, READ_LOW, 'cmp_lt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.ifBlock(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))), null)]
    })
});
bench({
    name: 'if_true_body',
    category: 'control',
    note: 'control_if taking its one-block body (the read, the comparison and the body block are subtracted)',
    loop: 'warp',
    minus: [TICK, READ_LOW, 'cmp_lt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.ifBlock(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))), B.changeX(NUM(1)))]
    })
});
bench({
    name: 'if_else_true',
    category: 'control',
    note: 'control_if_else taking the if branch (the read, the comparison and the body block are subtracted)',
    loop: 'warp',
    minus: [TICK, READ_LOW, 'cmp_lt_num'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.ifElse(
            B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))),
            B.changeX(NUM(1)), B.changeY(NUM(1)))]
    })
});
bench({
    name: 'if_else_false',
    category: 'control',
    note: 'control_if_else taking the else branch (the read, the comparison, the not and the body block are subtracted)',
    loop: 'warp',
    minus: [TICK, READ_LOW, 'cmp_lt_num', 'bool_not'],
    build: (B) => ({
        body: [B.varChange('bench_i', NUM(1)), B.ifElse(
            B.inBool(B.not(B.inBool(B.less(B.itemInput('num100', B.cyclicIndex(100, 1)), NUM(50))))),
            B.changeX(NUM(1)), B.changeY(NUM(1)))]
    })
});

// ---- pen and motion ------------------------------------------------------
bench({
    name: 'pen_clear',
    category: 'pen',
    note: 'pen_clear (the renderer counts the call and clears no raster)',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.penClear()] })
});
bench({
    name: 'pen_down',
    category: 'pen',
    note: 'pen_penDown (draws a point at the current position)',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.penDown()] })
});
bench({
    name: 'pen_up',
    category: 'pen',
    note: 'pen_penUp',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.penUp()] })
});
bench({
    name: 'pen_size_set',
    category: 'pen',
    note: 'pen_setPenSizeTo',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.penSize(NUM(2))] })
});
bench({
    name: 'pen_color_set',
    category: 'pen',
    note: 'pen_setPenColorToColor of a colour literal',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.penColor(COLOR('#ff0000'))] })
});
bench({
    name: 'motion_gotoxy',
    category: 'motion',
    note: 'motion_gotoxy of two literals',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.goToXY(NUM(10), NUM(10))] })
});
bench({
    name: 'motion_setx',
    category: 'motion',
    note: 'motion_setx of a literal',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({ body: [B.varChange('bench_i', NUM(1)), B.setX(NUM(10))] })
});
bench({
    name: 'pen_run5',
    category: 'pen',
    note: 'one monitor run as a unit: setPenColorToColor + goToXY + penDown + setX + penUp',
    loop: 'warp',
    minus: [TICK],
    build: (B) => ({
        body: [
            B.varChange('bench_i', NUM(1)),
            B.penColor(COLOR('#ff0000')),
            B.goToXY(NUM(10), NUM(10)),
            B.penDown(),
            B.setX(NUM(20)),
            B.penUp()
        ]
    })
});

// ---------------------------------------------------------------------------
// Derived rows: linear combinations of the rows above
// ---------------------------------------------------------------------------
const DERIVED = [
    { name: 'yield_frame_overhead', category: 'yield', note: 'a non-warp loop iteration less a warp one, with nothing asking for a redraw: the loop yield itself', terms: [['yield_repeat_empty', 1], ['repeat_empty', -1]] },
    { name: 'yield_call_delta', category: 'procedure', note: 'the same call non-warp as warp: what one redraw a call costs, because the loop pays a whole runtime step for it', terms: [['proc_call_0_plain', 1], ['proc_call_0', -1]] },
    { name: 'proc_call_2_delta', category: 'procedure', note: 'the two-parameter call less the no-parameter call: two parameter reads, one add, two arguments passed', terms: [['proc_call_2', 1], ['proc_call_0', -1]] },
    { name: 'proc_call_0_overhead', category: 'procedure', note: 'the warp call less the bare block it calls: the call itself', terms: [['proc_call_0', 1], [BARE, -1]] },
    { name: 'until_minus_repeat', category: 'control', note: 'a repeat until iteration less a repeat iteration, the body block being the same', terms: [['repeat_until_counter', 1], ['repeat_empty', -1], [BARE, -1]] },
    { name: 'if_body_vs_empty', category: 'control', note: 'the same if with and without a one-block body, measured next to each other: the body marginal to the branch', terms: [['if_true_body', 1], ['if_true_empty', -1]] },
    { name: 'if_else_vs_if', category: 'control', note: 'control_if_else taking its if branch less control_if taking the same body: the else branch\'s own overhead', terms: [['if_else_true', 1], ['if_true_body', -1]] },
    { name: 'if_else_false_vs_true', category: 'control', note: 'control_if_else taking the else branch less the same block taking the if branch', terms: [['if_else_false', 1], ['if_else_true', -1]] },
    { name: 'list_read_low_pure', category: 'list', note: 'the low read less its index arithmetic', terms: [[READ_LOW, 1], [IDX_LOW, -1]] },
    { name: 'list_read_high_pure', category: 'list', note: 'the high read less its index arithmetic', terms: [['list_read_high', 1], [IDX_HIGH, -1]] },
    { name: 'list_write_low_pure', category: 'list', note: 'the low write less its index arithmetic', terms: [['list_write_low', 1], [IDX_LOW, -1]] },
    { name: 'list_write_high_pure', category: 'list', note: 'the high write less its index arithmetic', terms: [['list_write_high', 1], [IDX_HIGH, -1]] },
    { name: 'list_read_high_delta', category: 'list', note: 'index 5000 read less index 50 read: zero means the read is O(1)', terms: [['list_read_high', 1], [READ_LOW, -1]] },
    { name: 'list_write_high_delta', category: 'list', note: 'index 5000 write less index 50 write: zero means the write is O(1)', terms: [['list_write_high', 1], ['list_write_low', -1]] },
    { name: 'list_add_vs_replace', category: 'list', note: 'add to list less replace item at a low index', terms: [['list_add', 1], ['list_write_low', -1]] },
    { name: 'op_add_coercion', category: 'coercion', note: 'the add of a numeric string less the add of a number: the coercion alone', terms: [['op_add_item_str', 1], ['op_add_item_num', -1]] },
    { name: 'op_mod_coercion', category: 'coercion', note: 'the modulo of a numeric string less the modulo of a number', terms: [['op_mod_item_str', 1], ['op_mod_item_num', -1]] },
    { name: 'op_floor_coercion', category: 'coercion', note: 'floor of a numeric string less floor of a number', terms: [['op_floor_item_str', 1], ['op_floor_item_num', -1]] },
    { name: 'expr_efb_mod_read', category: 'monitor', note: 'the low-half pixel expression less its index arithmetic: the read and the modulo', terms: [['expr_efb_mod', 1], ['expr_efb_index', -1]] },
    { name: 'expr_efb_floor_read', category: 'monitor', note: 'the high-half pixel expression less its index arithmetic', terms: [['expr_efb_floor', 1], ['expr_efb_index', -1]] },
    { name: 'pen_run5_vs_parts', category: 'pen', note: 'one run less its five blocks measured separately: positive because only in a run is the pen down while the sprite moves, so only a run pays for the penLine', terms: [['pen_run5', 1], ['pen_color_set', -1], ['motion_gotoxy', -1], ['pen_down', -1], ['motion_setx', -1], ['pen_up', -1]] }
];

// ---------------------------------------------------------------------------
// Sizing, running, reporting
// ---------------------------------------------------------------------------

const DEFAULT_SIZE = { pilot: 100000, targetMs: 200, min: 50000, max: 400000000 };
const RUN_TIMEOUT_MS = 60000;
const MAX_STEPS = 50000000;

function parseArgs(argv) {
    // `--json` is how the raw numbers are asked for, and it is not the default:
    // `dist/` holds the built projects and what a check writes and nothing else,
    // and a benchmark's output is neither until someone asks for it.
    const args = { reps: 5, json: null, only: null, targetMs: null };
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (a === '--reps') args.reps = Number(argv[++i]);
        else if (a === '--json') args.json = argv[++i];
        else if (a === '--only') args.only = new RegExp(argv[++i]);
        else if (a === '--target-ms') args.targetMs = Number(argv[++i]);
        else if (a === '--help' || a === '-h') args.help = true;
        else throw new Error(`unknown argument ${a}`);
    }
    return args;
}

function buildProject(spec) {
    const B = new Builder();
    B.var('bench_n', 100000);
    B.var('bench_done', 0);
    B.var('bench_tmp', 0);
    B.var('bench_sink', 0);
    B.var('bench_i', 0);
    B.var('bench_k', 0);
    B.list('list_big', Array.from({ length: 10000 }, (_, i) => (i * 7919) % 65536));
    B.list('efb_words', Array.from({ length: 10000 }, (_, i) => (i * 2654435761) % 4294967296));
    B.list('num100', Array.from({ length: 100 }, (_, i) => i + 1));
    B.list('str100', Array.from({ length: 100 }, (_, i) => String(i + 1)));
    B.list('rv_reg', []);
    if (spec.setup) spec.setup(B);

    const made = spec.build(B) || {};
    const body = made.body || [];
    const before = made.before || [];
    const loop = B.repeat(B.inValue(B.varGet('bench_n')), B.chain(body));
    const warp = spec.loop !== 'yield';

    // The hat runs the construct and then says so. In the warp case the loop
    // lives in a warp procedure, so the loop is a tight JavaScript for and the
    // frame boundary is paid once; in the yield case the loop is the script.
    // `bench_sink` is read after the loop so the accumulator the rows store
    // into is not itself dead code.
    let script;
    if (warp) {
        B.procedure('bench_loop', { warp: true, body: [...before, loop] });
        script = [B.call('bench_loop')];
    } else {
        script = [...before, loop];
    }
    const sink = B.varSet('bench_sink', B.varValue('bench_tmp'));
    const done = B.varSet('bench_done', NUM(1));
    const first = B.chain([...script, sink, done]);
    const hat = B.hat('event_whenflagclicked');
    B.blocks[hat].next = first;
    B.blocks[first].parent = hat;
    B.topLevel.push(hat);
    return B.project();
}

const median = (xs) => {
    const s = [...xs].sort((a, b) => a - b);
    const m = s.length >> 1;
    return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

/// Which benchmarks have to run for the requested ones: the selection plus
/// everything in its `minus` closure plus the calibrations.
function selection(args) {
    const byName = new Map(BENCHES.map((b) => [b.name, b]));
    const want = new Set();
    if (!args.only) {
        for (const b of BENCHES) want.add(b.name);
    } else {
        const addWithDeps = (name) => {
            if (want.has(name)) return;
            want.add(name);
            const b = byName.get(name);
            for (const m of (b && b.minus) || []) addWithDeps(m);
        };
        for (const b of BENCHES) if (args.only.test(b.name)) addWithDeps(b.name);
        for (const b of BENCHES) if (b.category === 'calibration') addWithDeps(b.name);
    }
    return BENCHES.filter((b) => want.has(b.name));
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
        out('node bench-blocks.mjs [--reps 5] [--json path] [--only regex] [--target-ms 200]');
        return;
    }

    const vm = new VirtualMachine();
    const renderer = noopRenderer();
    vm.attachRenderer(renderer);

    const results = new Map();
    const order = [];

    for (const spec of selection(args)) {
        const size = { ...DEFAULT_SIZE, ...(spec.size || {}) };
        if (args.targetMs) size.targetMs = args.targetMs;

        const project = buildProject(spec);
        try {
            await vm.loadProject(project);
        } catch (e) {
            results.set(spec.name, { name: spec.name, category: spec.category, note: spec.note, error: `loadProject: ${e && e.message || e}` });
            order.push(spec.name);
            continue;
        }
        vm.runtime.compilerOptions.enabled = true;
        vm.runtime.currentStepTime = 1000 / 30;

        const stage = vm.runtime.getTargetForStage();
        // A target keeps its variables and its lists in one map; a list's value
        // is the array itself.
        const nVar = stage.variables['vid_bench_n'];
        const doneVar = stage.variables['vid_bench_done'];
        const rvReg = stage.variables['lid_rv_reg'] && stage.variables['lid_rv_reg'].value;
        const listBig = stage.variables['lid_list_big'] && stage.variables['lid_list_big'].value;
        const listEfb = stage.variables['lid_efb_words'] && stage.variables['lid_efb_words'].value;
        const bigCopy = listBig ? [...listBig] : null;
        const efbCopy = listEfb ? [...listEfb] : null;

        if (!nVar || !doneVar) {
            results.set(spec.name, { name: spec.name, category: spec.category, note: spec.note, error: 'the stage lost bench_n or bench_done' });
            order.push(spec.name);
            continue;
        }

        const runOnce = (n) => {
            nVar.value = n;
            doneVar.value = 0;
            // The lists a construct grows are put back between runs; an array is
            // emptied in place, because a compiled list block holds the array.
            if (rvReg) rvReg.length = 0;
            if (bigCopy) { listBig.length = 0; listBig.push(...bigCopy); }
            if (efbCopy) { listEfb.length = 0; listEfb.push(...efbCopy); }
            const rt = vm.runtime;
            const t0 = performance.now();
            vm.greenFlag();
            let steps = 0;
            const deadline = Date.now() + RUN_TIMEOUT_MS;
            while (Number(doneVar.value) !== 1) {
                rt._step();
                steps++;
                if (steps > MAX_STEPS) break;
                if ((steps & 255) === 0 && Date.now() > deadline) break;
            }
            return { ms: performance.now() - t0, steps, done: Number(doneVar.value) === 1 };
        };

        const fail = (why) => {
            results.set(spec.name, { name: spec.name, category: spec.category, note: spec.note, error: why });
            order.push(spec.name);
        };

        let pilot;
        try {
            // The first run of a freshly compiled script is at a colder
            // optimisation tier than the ones after it, so the pilot that sizes
            // N is the second run, not the first.
            runOnce(size.pilot);
            pilot = runOnce(size.pilot);
        } catch (e) {
            fail(`pilot: ${e && e.stack || e}`);
            continue;
        }
        if (!pilot.done) { fail(`did not finish within ${RUN_TIMEOUT_MS} ms at N=${size.pilot}`); continue; }
        let n = Math.max(size.min, Math.min(size.max,
            Math.round((size.pilot * size.targetMs) / Math.max(1e-6, pilot.ms))));

        // Size once more off the first real run, because a warm loop can be
        // several times faster than the pilot that sized it.
        const runs = [];
        let first;
        try {
            first = runOnce(n);
        } catch (e) {
            fail(`run: ${e && e.stack || e}`);
            continue;
        }
        if (first.done) {
            const better = Math.max(size.min, Math.min(size.max,
                Math.round((n * size.targetMs) / Math.max(1e-6, first.ms))));
            if (better > n * 2 || better < n / 2) n = better;
            else runs.push(first);
        }
        for (let i = runs.length; i < args.reps; i++) {
            const r = runOnce(n);
            if (!r.done) break;
            runs.push(r);
        }
        if (runs.length === 0) { fail('no completed run'); continue; }

        const ms = runs.map((r) => r.ms);
        const steps = runs.map((r) => r.steps);
        const med = median(ms);
        const entry = {
            name: spec.name,
            category: spec.category,
            note: spec.note,
            loop: spec.loop === 'yield' ? 'yield' : 'warp',
            minus: spec.minus || [],
            n,
            reps: runs.length,
            medianMs: med,
            minMs: Math.min(...ms),
            maxMs: Math.max(...ms),
            medianSteps: median(steps),
            spreadPct: med > 0 ? ((Math.max(...ms) - Math.min(...ms)) / med) * 100 : 0,
            nsPerOp: (med / n) * 1e6,
            nsPerOpMin: (Math.min(...ms) / n) * 1e6
        };
        results.set(spec.name, entry);
        order.push(spec.name);
        progress(`  ${spec.name.padEnd(20)} N=${String(n).padEnd(11)} ${med.toFixed(2).padStart(9)} ms  ${entry.nsPerOp.toFixed(1).padStart(9)} ns/op  ${String(median(steps)).padStart(7)} steps`);
    }

    // ---- the loop overhead, then whatever each row declares ---------------
    //
    // Everything below is computed twice: once on the median of each row's runs,
    // which is the statistic to quote, and once on the *minimum*, which is the
    // one that survives a machine another process is also using. On a quiet
    // machine the two agree; on a loaded one the minimum is the honest reading
    // of what the block costs and the median is what the machine charged for it
    // at the time, and every row is reported both ways rather than pretending
    // one of them is the whole answer.
    const analyze = (key) => {
        const calib = (loop) => {
            const c = results.get(loop === 'yield' ? 'yield_repeat_empty' : 'repeat_empty');
            return c && !c.error ? c[key] : 0;
        };
        const cache = new Map();
        const net = (name) => {
            if (cache.has(name)) return cache.get(name);
            const e = results.get(name);
            if (!e || e.error) return null;
            cache.set(name, 0); // cycle guard
            // A calibration row is the loop overhead itself; it has nothing to
            // subtract but itself.
            let v = e.category === 'calibration' ? e[key] : e[key] - calib(e.loop);
            for (const m of e.minus || []) {
                const x = net(m);
                if (x !== null) v -= x;
            }
            cache.set(name, v);
            return v;
        };
        for (const name of order) net(name);
        const derived = [];
        for (const d of DERIVED) {
            const values = d.terms.map(([name]) => net(name));
            if (values.some((v) => v === null)) continue;
            derived.push({
                name: d.name, category: d.category, note: d.note,
                value: d.terms.reduce((acc, [, sign], i) => acc + sign * values[i], 0),
                spreadPct: Math.max(...d.terms.map(([name]) => {
                    const e = results.get(name);
                    return e && !e.error ? e.spreadPct : 0;
                })),
                terms: d.terms.map(([name, sign]) => ({ name, sign }))
            });
        }
        return { key, calib, net, derived };
    };
    const med = analyze('nsPerOp');
    const best = analyze('nsPerOpMin');

    // ---- the unit and the ratios -------------------------------------------
    const unit = med.net(BARE) || 1;
    const setVarNet = med.net(SET_VAR);
    // A unit that measures below the resolution of the harness cannot be a unit:
    // the column is dropped rather than filled with a division by noise.
    const unit2 = setVarNet !== null && Math.abs(setVarNet) >= 0.5 ? setVarNet : null;
    const ratioOf = (a, b) => {
        const x = med.net(a), y = med.net(b);
        if (x === null || y === null || y === 0) return null;
        return x / y;
    };
    const ratios = {
        add: [ratioOf('op_add_item_str', 'op_add_item_num'), 'op_add_item_num', 'op_add_item_str'],
        mod: [ratioOf('op_mod_item_str', 'op_mod_item_num'), 'op_mod_item_num', 'op_mod_item_str'],
        floor: [ratioOf('op_floor_item_str', 'op_floor_item_num'), 'op_floor_item_num', 'op_floor_item_str'],
        equals: [ratioOf('cmp_equals_str', 'cmp_equals_num'), 'cmp_equals_num', 'cmp_equals_str'],
        lt: [ratioOf('cmp_lt_str', 'cmp_lt_num'), 'cmp_lt_num', 'cmp_lt_str']
    };

    // ---- the table ---------------------------------------------------------
    // The uncertainty of a subtracted row is the uncertainty of its own runs
    // plus the uncertainties of everything taken off it: a difference of two
    // noisy numbers is noisier than either, and saying so is the whole point of
    // the noise column.
    const spreadOf = (name) => {
        const e = results.get(name);
        return e && !e.error ? e.spreadPct / 100 : 0;
    };
    const noiseCache = new Map();
    const noiseOf = (name) => {
        if (noiseCache.has(name)) return noiseCache.get(name);
        const e = results.get(name);
        if (!e || e.error) return 0;
        noiseCache.set(name, 0);
        let n = Math.abs(e.nsPerOp) * spreadOf(name);
        if (e.category !== 'calibration') {
            n += med.calib(e.loop) * spreadOf(e.loop === 'yield' ? 'yield_repeat_empty' : 'repeat_empty');
        }
        for (const m of e.minus || []) n += noiseOf(m);
        noiseCache.set(name, n);
        return n;
    };
    const rows = [];
    for (const name of order) {
        const e = results.get(name);
        if (!e) continue;
        if (e.error) { rows.push({ name, category: e.category, note: e.note, error: e.error }); continue; }
        const nsPerOp = med.net(name);
        const noiseNs = noiseOf(name);
        // A row whose net is smaller than its own uncertainty -- or is negative,
        // which no block can be -- is a row this machine cannot resolve. It is
        // reported as measured and marked.
        rows.push({
            name, category: e.category, note: e.note, derived: false,
            nsPerOp, nsPerOpMin: best.net(name), noisePct: e.spreadPct, noiseNs,
            unresolved: nsPerOp < 0 || Math.abs(nsPerOp) <= noiseNs,
            steps: e.medianSteps, n: e.n, reps: e.reps, raw: e
        });
    }
    for (const d of med.derived) {
        const dBest = best.derived.find((x) => x.name === d.name);
        const noiseNs = d.terms.reduce((acc, t) => acc + noiseOf(t.name), 0);
        rows.push({
            name: d.name, category: d.category, note: d.note, derived: true,
            nsPerOp: d.value, nsPerOpMin: dBest ? dBest.value : null,
            noisePct: d.spreadPct, noiseNs,
            unresolved: d.value < 0 || Math.abs(d.value) <= noiseNs,
            steps: null, n: null, reps: null, terms: d.terms
        });
    }
    rows.sort((a, b) => (b.error ? -Infinity : b.nsPerOp) - (a.error ? -Infinity : a.nsPerOp));

    const pad = (s, w) => String(s).padEnd(w);
    const num = (x, w, d = 2) => String(Number.isFinite(x) ? x.toFixed(d) : 'n/a').padStart(w);

    out('');
    out(`bench-blocks: ${rows.length} rows, ${args.reps} repetitions each, TurboWarp compiler ON`);
    out(`vm            ${VM_ROOT}  (scratch-vm ${vmVersion})`);
    out(`unit          one motion_changexby = ${unit.toFixed(2)} ns/op` +
        (unit2 === null ? '; one data_setvariableto measured below the noise floor, so it is not a unit' : `; one data_setvariableto = ${unit2.toFixed(2)} ns/op`));
    out(`consumer      one data_changevariableby accumulator = ${med.net(ACC) === null ? 'n/a' : med.net(ACC).toFixed(2)} ns/op`);
    out('noise         (max - min) / median over the repetitions, in per cent');
    out('');
    out('name                        category     ms/op    ns/op(med)  ns/op(min)  rel(bare)  noise%  steps/run');
    out('-'.repeat(98));
    for (const r of rows) {
        if (r.error) {
            out(`${pad(r.name + ' !', 27)} ${pad(r.category, 11)}  ${r.error}`);
            continue;
        }
        out([
            pad(r.name + (r.derived ? ' *' : '') + (r.unresolved ? ' ~' : ''), 27),
            pad(r.category, 11),
            num(r.nsPerOp / 1e6, 9, 3),
            num(r.nsPerOp, 11, 2),
            num(r.nsPerOpMin === null ? NaN : r.nsPerOpMin, 11, 2),
            num(r.nsPerOp / unit, 9, 3),
            num(r.noisePct, 7, 1),
            pad(r.steps === null ? '-' : String(r.steps), 10)
        ].join(' '));
    }
    out('-'.repeat(98));
    out('* derived by subtraction from the rows above; every other row already has');
    out('  the empty-loop calibration and its declared `minus` rows taken off.');
    out('~ the net is smaller than its own spread, or is negative, so the value is at');
    out('  this machine\'s resolution: read it as "about zero", not as a small number.');
    out('  ns/op(min) is the fastest run of the repetitions: on a machine another');
    out('  process is also using, it is the reading of what the block costs, and the');
    out('  median is the reading of what the machine charged for it.');
    out('');
    out('coercion: the same compiled expression over a list item holding a number and');
    out('over one holding the same value as a numeric string. The numeric rows land on');
    out('the harness\'s floor, so the difference is the reading and the ratio is only');
    out('given when the number row is above that floor.');
    for (const [what, [ratio, numName, strName]] of Object.entries(ratios)) {
        const a = med.net(strName);
        const b = med.net(numName);
        const delta = a === null || b === null ? null : a - b;
        const shown = b !== null && b >= 0.5 && ratio !== null ? ratio.toFixed(3) : 'n/a (the number row is at the floor)';
        out(`  ${pad(what, 8)} number ${num(b === null ? NaN : b, 7, 2)} ns   string ${num(a === null ? NaN : a, 7, 2)} ns   ` +
            `difference ${num(delta === null ? NaN : delta, 7, 2)} ns   ratio ${shown}`);
    }
    if (vmErrors.length) {
        out('');
        out(`${vmErrors.length} error line(s) the VM reported:`);
        for (const e of vmErrors.slice(0, 8)) out('  ' + e.split('\n')[0]);
    }

    // ---- the JSON ----------------------------------------------------------
    const payload = {
        tool: 'bench-blocks.mjs',
        method: {
            vmRoot: VM_ROOT,
            vmVersion,
            compiler: 'TurboWarp compiled: runtime.compilerOptions.enabled = true and never disabled',
            renderer: 'no-op renderer whose penLine/penPoint/penClear count calls and draw nothing, so the rasteriser is excluded',
            project: 'synthetic, built as a plain object and passed to vm.loadProject(object); one sprite, no costumes, one pen extension',
            timing: 'performance.now() around vm.greenFlag() plus runtime._step() until the stage variable bench_done is 1',
            loopOverhead: 'subtracted: an empty warp repeat for warp rows, an empty non-warp repeat for yield rows',
            liveness: 'every body starts with `change bench_i by 1` (so nothing is loop-invariant) and every reporter is the value of `set bench_tmp to ...`, read after the loop (so no store is dead)',
            iterations: `per benchmark: a pilot run sizes N so one run takes about ${args.targetMs || DEFAULT_SIZE.targetMs} ms, clamped to that benchmark's min and max`,
            repetitions: args.reps,
            statistic: 'median of the repetitions; noise = (max - min) / median, as a percentage',
            unit: 'bare_block = one motion_changexby 1; rel_setvar = one data_setvariableto'
        },
        unit: { name: BARE, nsPerOp: unit, setVarNsPerOp: setVarNet, consumerNsPerOp: med.net(ACC) },
        ratiosStringVsNumber: Object.fromEntries(Object.entries(ratios).map(([k, [ratio, numName, strName]]) => [k, {
            ratio, numberNsPerOp: med.net(numName), stringNsPerOp: med.net(strName),
            numberRow: numName, stringRow: strName
        }])),
        rows: rows.map((r) => (r.error ? { name: r.name, category: r.category, note: r.note, error: r.error } : {
            name: r.name, category: r.category, note: r.note, derived: !!r.derived,
            nsPerOp: r.nsPerOp, msPerOp: r.nsPerOp / 1e6,
            relToBareBlock: r.nsPerOp / unit, relToSetVar: unit2 === null ? null : r.nsPerOp / unit2,
            noisePct: r.noisePct, noiseNs: r.noiseNs, unresolved: !!r.unresolved,
            steps: r.steps, n: r.n, reps: r.reps,
            terms: r.terms || null, raw: r.raw || null
        })),
        errors: vmErrors
    };
    if (args.json) {
        const target = path.resolve(args.json);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, JSON.stringify(payload, null, 2) + '\n');
        out('');
        out(`json          ${target}`);
    }
}

main().catch((err) => {
    process.stderr.write('BENCH FAILURE: ' + (err && err.stack ? err.stack : String(err)) + '\n');
    process.exit(1);
});
