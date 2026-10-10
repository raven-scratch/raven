// Where the Scratch VM is, on whatever machine this is.
//
//     import { loadVm, findVmRoot } from '../../../../tools/vm-root.mjs';
//     const { VirtualMachine, root } = loadVm();
//
// Every example in this repository runs its built project in a real Scratch VM,
// because that is the only end-to-end check there is -- and every one of them
// used to find that VM its own way. `examples/raven/desktop/tools/check.mjs`
// tried an environment variable and two checkouts beside the repository;
// `examples/raven/penfont/tools/check.mjs` and the chess check required the
// environment variable and gave up without it; `examples/raven/rv32ima`'s tools
// assumed a directory called `../scratch-vm`. So a check that passed on the
// machine it was written on failed on the next one for a reason that had nothing
// to do with the project.
//
// This is the one place that answers the question. A check calls `loadVm` and
// gets either a working constructor or an error that names every path it looked
// in, which is the difference between "it does not work here" and "put a
// checkout *there*".
//
// # The search
//
// In order, first hit wins:
//
//   1. `$SCRATCH_VM_ROOT`, because an explicit answer beats a guess. It may
//      name the package directory, the directory holding it
//      (`node_modules/scratch-vm`), or a checkout root.
//   2. `ref/turbowarp-vm` -- the compiler, and the one every example's README
//      tells you to clone, because Scratch's own interpreter cannot finish a
//      Linux boot inside a human lifetime.
//   3. `ref/scratch-vm/node_modules/scratch-vm`, then `ref/scratch-vm`, which
//      are the two shapes a `scratch-vm` checkout takes depending on whether it
//      was installed or cloned.
//   4. `ref/scratch-editor/packages/scratch-vm`, the monorepo layout.
//   5. `scratch-vm` and `../scratch-vm` as siblings of *this repository*, for a
//      checkout that lives beside it rather than inside it.
//
// None of these are required to exist. A machine with a VM at any of them needs
// no configuration at all; a machine with one somewhere else sets
// `SCRATCH_VM_ROOT`; and a machine with none gets told so.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

/// The repository root, from this file's own location. Never a literal, so a
/// checkout anywhere works.
export const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/// The file that makes a directory a Scratch VM. Both Scratch's own checkout and
/// TurboWarp's have it at this path, which is what makes one test serve both.
const ENTRY = path.join('src', 'virtual-machine.js');

/// The directory holding `src/virtual-machine.js`, given one that may be the
/// package itself, the `node_modules` directory holding it, or a checkout root.
function packageIn(dir) {
    const direct = path.join(dir, ENTRY);
    if (fs.existsSync(direct)) return dir;
    const nested = path.join(dir, 'node_modules', 'scratch-vm');
    if (fs.existsSync(path.join(nested, ENTRY))) return nested;
    const packages = path.join(dir, 'packages', 'scratch-vm');
    if (fs.existsSync(path.join(packages, ENTRY))) return packages;
    return null;
}

/// Every directory this search will try, in order, whether or not it exists.
///
/// Returned rather than hidden so that the failure message can name them: a
/// check that cannot find a VM should say where it looked, or the reader is left
/// guessing which of five layouts this machine was supposed to have.
export function vmCandidates() {
    const env = process.env.SCRATCH_VM_ROOT;
    const beside = path.resolve(REPO, '..');
    return [
        env && { from: '$SCRATCH_VM_ROOT', dir: path.resolve(env) },
        { from: 'ref/turbowarp-vm', dir: path.join(REPO, 'ref', 'turbowarp-vm') },
        {
            from: 'ref/scratch-vm',
            dir: path.join(REPO, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
        },
        { from: 'ref/scratch-vm', dir: path.join(REPO, 'ref', 'scratch-vm') },
        {
            from: 'ref/scratch-editor',
            dir: path.join(REPO, 'ref', 'scratch-editor', 'packages', 'scratch-vm')
        },
        { from: 'a sibling of the repository', dir: path.join(beside, 'scratch-vm') }
    ].filter(Boolean);
}

/// The first candidate that really is a Scratch VM, or `null`.
export function findVmRoot() {
    for (const candidate of vmCandidates()) {
        const found = packageIn(candidate.dir);
        if (found) return found;
    }
    return null;
}

/// How to find a VM, as a message a reader can act on.
export function vmHelp() {
    const lines = vmCandidates().map((c) => `  ${c.dir}   (${c.from})`);
    return [
        'No Scratch VM found. These are the examples\' end-to-end checks, and',
        'they need a `scratch-vm` to run a built project in. Looked in:',
        ...lines,
        '',
        'Either clone one into `ref/`:',
        '  git clone --depth 1 https://github.com/TurboWarp/scratch-vm ref/turbowarp-vm',
        '  (cd ref/turbowarp-vm && npm install)',
        'or point at one you already have:',
        '  SCRATCH_VM_ROOT=/path/to/scratch-vm <the command>'
    ].join('\n');
}

/// The VM constructor, and where it came from.
///
/// Throws with `vmHelp()` rather than exiting, so a caller can decide: a check
/// wants a non-zero exit, and a tool that has a fallback wants to use it.
export function loadVm() {
    const root = findVmRoot();
    if (!root) {
        const error = new Error(vmHelp());
        error.code = 'NO_SCRATCH_VM';
        throw error;
    }
    const require_ = createRequire(import.meta.url);
    // Scratch's VM and its dependencies reach for browser globals at import
    // time. A page that is hidden is the answer that skips the draw, which is
    // what a headless check wants and what every tool here used to set by hand.
    if (typeof globalThis.document === 'undefined') {
        globalThis.document = { hidden: true };
    }
    return { VirtualMachine: require_(path.join(root, ENTRY)), root };
}

/// A renderer that draws nothing, for the tools whose subject is not the picture.
///
/// Only the calls these projects make. It is the one both `check.mjs` files and
/// the profiling tools were each carrying a copy of.
export function nullRenderer({ width = 480, height = 360 } = {}) {
    let nextId = 1;
    const drawables = new Map();
    return {
        setLayerGrouping() {}, setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; }, createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; }, createPenSkin() { return nextId++; },
        destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {}, updateTextSkin() {},
        getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [width, height]; },
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
