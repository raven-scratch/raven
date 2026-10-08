// The residue probe: sample the Stage *while the guest runs* and say which
// rows of the panel are not the card's picture, for how long, and whether a
// later pass ever repaired them.
//
//     node tools/probe-residue.mjs --budget 900 --after 45
//     node tools/probe-residue.mjs --runs 2 --budget 900 --after 45 --png dist/residue
//
// `check-rv32.mjs` proves the invariant at rest: it stops the guest, gives the
// monitor forty passes, and finds zero differing pixels. That is a real
// assertion and it is not this one. The picture that burns in is the one the
// monitor never *finishes*, and a machine that has stopped cannot show it --
// the pen has no erase, so a row whose last repaint happened before a change
// and which nothing ever repaints again keeps the older picture for ever.
//
// So this probe steps the machine one Scratch frame at a time and, after every
// frame, compares the raster the pen left against the card's own memory, row by
// row. For each row it keeps three things:
//
//   * the frame the card's row last changed             (lastCard)
//   * the frame the Stage's row last *matched* the card (lastMatch)
//   * how many consecutive frames it has been wrong     (run)
//
// A row is "repaired" when its last match is at or after its last card change.
// A row that is stale while the card is quiet *by wall clock* is the burn-in:
// the monitor had every opportunity and did not take it. `--quiet` is how many
// milliseconds of quiet before a stale row is called out. Under the monitor
// that repaints the whole panel every pass, a row can only be stale for the
// part of one pass; anything that survives a whole pass is a real residue.
//
// `--runs N` presses the green flag N times, which restarts the machine from
// reset. A reset empties the card, and the Stage is not empty, so the second
// run is where a reset with no repaint would show: it takes a whole-panel pass
// to put the black card back on the Stage.

import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { encodePng, scaleNearest } from './png.mjs';

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

const VM_ROOT = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean).find((d) => fs.existsSync(path.join(d, 'src', 'virtual-machine.js')));
if (!VM_ROOT) { console.error('no Scratch VM'); process.exit(2); }
const require_ = createRequire(import.meta.url);
const VirtualMachine = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));

function rasterRenderer(W, H) {
    let nextId = 1;
    const drawables = new Map();
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
        pixels, clears: 0,
        setLayerGrouping() {}, setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; }, createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; }, createPenSkin() { return nextId++; },
        destroySkin() {}, updateSVGSkin() {}, updateBitmapSkin() {}, updateTextSkin() {},
        getSkinSize() { return [1, 1]; }, getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; }, getNativeSize() { return [W, H]; },
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
        penClear() { this.clears++; pixels.fill(-1); },
        penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) { stamp(attrs.diameter, attrs.color4f, x0, y0, x1, y1); },
        penPoint(_skin, attrs, x, y) { stamp(attrs.diameter, attrs.color4f, x, y, x, y); },
        draw() {}
    };
}

const STAGE_W = 480, STAGE_H = 360;

const args = process.argv.slice(2);
const arg = (name, fallback) => {
    const at = args.indexOf(name);
    return at >= 0 ? Number(args[at + 1]) : fallback;
};
const budgetMs = arg('--budget', 900) * 1000;
const afterSeconds = arg('--after', 45);
const QUIET_MS = arg('--quiet', 1500);
const stopFrames = arg('--stop-frames', 200);
const sampleEvery = Math.max(1, arg('--sample', 1));
const runs = Math.max(1, arg('--runs', 1));
const pngArg = args.indexOf('--png');
const pngPrefix = pngArg >= 0 ? args[pngArg + 1] : null;
// Where the burn-in picture goes, written the first moment a row is stale with
// the card quiet. `--png` names the end-of-run pair instead.
const worstArg = args.indexOf('--png-worst');
const pngWorst = worstArg >= 0 ? args[worstArg + 1] : path.join(root, 'dist', 'residue-worst');
const sb3 = args.find((a) => a.endsWith('.sb3')) ||
    path.join(root, 'dist', 'desktop-rv32-linux.sb3');

async function main() {
    const board = (await import(
        new URL('../boards/mini-rv32.mjs', import.meta.url).href)).default;
    const guest = /doom/.test(path.basename(sb3)) ? 'doom' : 'linux';
    const p0 = { ...board.display, ...(board.guests[guest]?.display ?? {}) };
    const panel = { ...p0, ...board.formats[p0.format], indexed: p0.format === 'index8' };
    if (panel.indexed) {
        console.error('this probe is about the direct colour panel, whose pixels the ' +
            'check reads straight out of the card; the indexed panel is drawn from the ' +
            'board\'s latch instead');
        process.exit(2);
    }
    const W = panel.width, H = panel.height, WORDS = panel.pitch / 4;

    const vm = new VirtualMachine();
    const renderer = rasterRenderer(STAGE_W, STAGE_H);
    vm.attachRenderer(renderer);
    const warn = console.warn;
    console.warn = () => {};
    const data = fs.readFileSync(sb3);
    await vm.loadProject(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
    const runtime = vm.runtime;
    if (runtime.compilerOptions) runtime.compilerOptions.enabled = !args.includes('--no-compile');
    const stage = runtime.getTargetForStage();
    const find = (name) => Object.values(stage.variables).find((x) => x.name === name);
    const value = (name) => { const v = find(name); return v ? v.value : undefined; };
    const set = (name, v) => { const f = find(name); if (f) f.value = v; };
    const consoleText = () => Buffer.from(((value('console_trace') || [])
        .map((b) => Number(b) & 0xff))).toString('latin1');
    const timer = () => {
        const c = runtime.ioDevices && runtime.ioDevices.clock;
        return c && typeof c.projectTimer === 'function' ? Number(c.projectTimer()) : 0;
    };

    const r5 = new Int32Array(32), g6 = new Int32Array(64);
    for (let i = 0; i < 32; i++) r5[i] = Math.round(i * 255 / 31);
    for (let i = 0; i < 64; i++) g6[i] = Math.round(i * 255 / 63);
    const cardRow = new Int32Array(W);
    const readCardRow = (words, row) => {
        for (let x = 0; x < W; x += 2) {
            const word = Number(words[row * WORDS + (x >> 1)] || 0);
            for (let half = 0; half < 2; half++) {
                const v = half === 0 ? word % 65536 : Math.floor(word / 65536) % 65536;
                cardRow[x + half] = (r5[(v >> 11) & 31] << 16) |
                    (g6[(v >> 5) & 63] << 8) | r5[v & 31];
            }
        }
        return cardRow;
    };
    const sameRow = (row, card) => {
        const base = row * STAGE_W;
        const px = renderer.pixels;
        for (let x = 0; x < W; x++) {
            const got = px[base + x];
            if ((got < 0 ? 0 : got) !== card[x]) return false;
        }
        return true;
    };

    // ---- per-run state
    let S = null;
    const freshState = () => ({
        frame: 0,
        lastCard: new Int32Array(H).fill(-1),
        lastMatch: new Int32Array(H).fill(-1),
        run: new Int32Array(H),
        worstRun: new Int32Array(H),
        staleSince: new Float64Array(H).fill(-1),
        episodeStart: new Float64Array(H),
        cardAt: new Float64Array(H),
        cardHash: new Int32Array(H).fill(0x7fffffff),
        openEpisode: new Int32Array(H).fill(-1),
        maxRun: 0, maxRunRow: -1, maxQuietRun: 0, maxQuietRow: -1,
        maxQuietMs: 0, maxQuietRowAt: -1,
        maxMs: 0, maxMsRow: -1,
        quietRows: new Set(),
        staleHistogram: new Map(),
        episodes: [],
        bookkeeping: [],
        samples: 0,
        snapped: false,
        lastStale: 0
    });

    const sample = () => {
        const s = S;
        const words = value('efb_words') || [];
        const now = Date.now();
        s.samples++;
        let staleRows = 0, quietRows = 0;
        for (let r = 0; r < H; r++) {
            const card = readCardRow(words, r);
            let h = 0;
            for (let x = 0; x < W; x++) h = (h * 31 + card[x]) | 0;
            if (h !== s.cardHash[r]) {
                s.cardHash[r] = h;
                s.lastCard[r] = s.frame;
            }
            if (sameRow(r, card)) {
                s.lastMatch[r] = s.frame;
                s.run[r] = 0;
                s.staleSince[r] = -1;
                if (s.openEpisode[r] >= 0) {
                    if (s.frame - s.openEpisode[r] >= 4 && s.episodes.length < 200000) {
                        s.episodes.push({ row: r, from: s.openEpisode[r], to: s.frame,
                            ms: now - s.episodeStart[r] });
                    }
                    s.openEpisode[r] = -1;
                }
                continue;
            }
            staleRows++;
            s.run[r] += 1;
            if (s.run[r] > s.worstRun[r]) s.worstRun[r] = s.run[r];
            if (s.run[r] > s.maxRun) { s.maxRun = s.run[r]; s.maxRunRow = r; }
            if (s.openEpisode[r] < 0) { s.openEpisode[r] = s.frame; s.episodeStart[r] = now; }
            const ms = s.staleSince[r] < 0 ? 0 : now - s.staleSince[r];
            if (s.staleSince[r] < 0) s.staleSince[r] = now;
            if (ms > s.maxMs) { s.maxMs = ms; s.maxMsRow = r; }
            const quietMs = now - (s.cardAt[r] || now);
            if (quietMs >= QUIET_MS) {
                quietRows++;
                s.quietRows.add(r);
                if (s.run[r] > s.maxQuietRun) { s.maxQuietRun = s.run[r]; s.maxQuietRow = r; }
                if (quietMs > s.maxQuietMs) { s.maxQuietMs = quietMs; s.maxQuietRowAt = r; }
            }
            for (const k of [1, 2, 4, 8, 16, 32, 64, 128, 256]) {
                if (s.run[r] === k) s.staleHistogram.set(k, (s.staleHistogram.get(k) || 0) + 1);
            }
        }
        // When did each row's card content last change, in wall clock, for the
        // quiet test above.
        s.lastStale = staleRows;
        if (quietRows > 0 && s.bookkeeping.length < 30 && s.frame % 20 === 0) {
            s.bookkeeping.push(`frame ${s.frame} (t=${timer().toFixed(2)})  ` +
                `${quietRows} row(s) stale with the card quiet; worst row ${s.maxQuietRow} ` +
                `stale for ${((now - (s.cardAt[s.maxQuietRow] || now)) / 1000).toFixed(1)} s  ` +
                `monitor_frames ${value('monitor_frames')} ` +
                `monitor_reads ${value('monitor_reads')} monitor_runs ${value('monitor_runs')}`);
        }
        // The picture the owner is looking at, taken at the moment the burn-in
        // is real rather than after the monitor has caught up: the Stage as the
        // pen left it, the card's own pixels, and a white-on-black map of the
        // pixels where the two disagree. A pair of screenshots from before and
        // after a run cannot show this, because the Stage is *right* again by
        // then.
        //
        // The trigger is a run of frames rather than the wall clock: a row that
        // the card has left alone for forty frames while the Stage still shows
        // something else is the burn-in by definition, and it is a threshold the
        // fixed monitor does not reach.
        if (s.maxRun >= 40 && !S.snapped) {
            S.snapped = true;
            const shot = (name, rgbOf) => {
                const buf = Buffer.alloc(STAGE_W * STAGE_H * 3);
                for (let y = 0; y < STAGE_H; y++) {
                    const card = readCardRow(words, y);
                    for (let x = 0; x < STAGE_W; x++) {
                        const c = rgbOf(y, x, card);
                        const at = (y * STAGE_W + x) * 3;
                        buf[at] = (c >>> 16) & 0xff;
                        buf[at + 1] = (c >>> 8) & 0xff;
                        buf[at + 2] = c & 0xff;
                    }
                }
                const big = scaleNearest(STAGE_W, STAGE_H, buf, 2);
                fs.mkdirSync(path.dirname(path.resolve(name)), { recursive: true });
                fs.writeFileSync(name, encodePng(big.width, big.height, big.rgb));
            };
            shot(`${pngWorst}-stage.png`, (y, x) => {
                const got = renderer.pixels[y * STAGE_W + x];
                return got < 0 ? 0 : got;
            });
            shot(`${pngWorst}-card.png`, (y, x, card) => (x < W ? card[x] : 0));
            shot(`${pngWorst}-diff.png`, (y, x, card) => {
                const got = renderer.pixels[y * STAGE_W + x];
                const g = got < 0 ? 0 : got;
                return g === (x < W ? card[x] : 0) ? 0 : 0xffffff;
            });
            warn(`    picture       ${pngWorst}-stage.png, ${pngWorst}-card.png and ` +
                `${pngWorst}-diff.png taken at frame ${s.frame} (t=${timer().toFixed(2)}), the ` +
                `first moment a row had been the wrong picture for ${s.maxRun} frames`);
        }
        return { staleRows, quietRows };
    };

    const summarise = (rows) => {
        if (!rows.length) return '(none)';
        const out = [];
        let lo = rows[0], prev = rows[0];
        for (let i = 1; i <= rows.length; i++) {
            const r = rows[i];
            if (r !== prev + 1) {
                out.push(lo === prev ? `${lo}` : `${lo}-${prev}`);
                lo = r;
            }
            prev = r;
        }
        return out.join(', ');
    };

    const step = (n = 1) => { for (let i = 0; i < n; i++) { runtime._step(); S.frame++; if (S.frame % sampleEvery === 0) sample(); } };

    const report = (label) => {
        const s = S;
        warn(`--- ${label}: ${s.frame} frames, ${s.samples} samples`);
        warn(`    worst staleness      ${s.maxRun} frames (row ${s.maxRunRow}), ` +
            `${(s.maxMs / 1000).toFixed(1)} s (row ${s.maxMsRow})`);
        warn(`    worst while quiet    ${s.maxQuietRun} frames (row ${s.maxQuietRow}); ` +
            `${s.quietRows.size} rows were stale at least once with the card quiet for ` +
            `${(QUIET_MS / 1000).toFixed(1)} s or more`);
        let everStale = 0, neverRepaired = [];
        for (let r = 0; r < H; r++) {
            if (s.lastCard[r] >= 0 && s.lastMatch[r] < s.lastCard[r]) {
                everStale++;
                neverRepaired.push(r);
            }
        }
        warn(`    rows never repaired  ${everStale} of ${H}: ${summarise(neverRepaired)}`);
        warn(`    stale run lengths (frames -> rows that reached it): ` +
            [...s.staleHistogram.keys()].sort((a, b) => a - b)
                .map((k) => `${k}:${s.staleHistogram.get(k)}`).join(' '));
        const offenders = [...s.quietRows].sort((a, b) => s.worstRun[b] - s.worstRun[a]);
        if (offenders.length) {
            warn(`    rows stale with the card quiet, worst first:`);
            for (const r of offenders.slice(0, 30)) {
                warn(`      row ${String(r).padStart(3)}  ${String(s.worstRun[r]).padStart(4)} frames  ` +
                    `last card change frame ${s.lastCard[r]}, last Stage match ${s.lastMatch[r]}`);
            }
            if (offenders.length > 30) warn(`      ... and ${offenders.length - 30} more`);
        }
        const worst = s.episodes.sort((a, b) => b.ms - a.ms).slice(0, 12);
        if (worst.length) {
            warn(`    longest stale episodes (row: frames, seconds): `);
            for (const e of worst) {
                warn(`      row ${String(e.row).padStart(3)}  frames ${e.from}..${e.to} ` +
                    `(${e.to - e.from + 1})  ${(e.ms / 1000).toFixed(1)} s`);
            }
        }
        const open = [];
        for (let r = 0; r < H; r++) if (s.openEpisode[r] >= 0) open.push(r);
        warn(`    unclosed at the end  ${open.length} rows: ${summarise(open)}`);
        for (const line of s.bookkeeping) warn(`    ${line}`);
    };

    warn(`file          ${path.relative(process.cwd(), sb3)}`);
    warn(`vm            ${path.relative(repo, VM_ROOT)}`);
    warn(`panel         ${W}x${H} ${panel.format}, ${WORDS} words a row`);
    warn(`runs          ${runs} green flag(s), ${afterSeconds} s of running sampled each`);

    for (let runIndex = 1; runIndex <= runs; runIndex++) {
        S = freshState();
        // `cardAt[r]` is the wall clock at which the card's row r last changed;
        // the initial value is now, so a row that has never changed is as quiet
        // as the run is old.
        const t0 = Date.now();
        S.cardAt.fill(t0);

        if (runIndex > 1) {
            vm.greenFlag();
            warn(`green flag ${runIndex}  project timer now ${timer().toFixed(2)}; the ` +
                `machine is reset and the card is emptied, so a whole-panel pass has to ` +
                `put the black card back on the Stage`);
        } else {
            vm.greenFlag();
        }
        runtime.currentStepTime = 1000 / 30;
        const budgetUntil = Date.now() + budgetMs;
        let prompted = false;
        while (Date.now() < budgetUntil && !prompted) {
            runtime._step(); S.frame++;
            if (S.frame % sampleEvery === 0) { trackCardChanges(); sample(); }
            if (S.frame % 60 === 0) prompted = /Run \/init as init process/.test(consoleText());
        }
        prompted = /Run \/init as init process/.test(consoleText());
        warn(`boot          ${S.frame} frames, ${((Date.now() - t0) / 1000).toFixed(1)} s, ` +
            `${value('rv_instructions')} guest instructions${prompted ? '' : ' (NO HANDOVER)'}`);

        const runUntil = Date.now() + afterSeconds * 1000;
        while (Date.now() < runUntil) {
            runtime._step(); S.frame++;
            if (S.frame % sampleEvery === 0) { trackCardChanges(); sample(); }
        }
        const runningFrame = S.frame;

        if (stopFrames > 0) {
            set('rv_state', 0);
            for (let i = 0; i < stopFrames; i++) {
                runtime._step(); S.frame++;
                if (S.frame % sampleEvery === 0) { trackCardChanges(); sample(); }
            }
            const words = value('efb_words') || [];
            const still = [];
            for (let r = 0; r < H; r++) if (!sameRow(r, readCardRow(words, r))) still.push(r);
            warn(`stopped       at frame ${runningFrame}, plus ${stopFrames} frames with the ` +
                `guest held; ${still.length} of ${H} rows still differ: ${summarise(still)}`);
            warn(`monitor       ${value('monitor_frames')} passes, ` +
                `${value('monitor_reads')} pixels read, ${value('monitor_runs')} runs drawn`);
        }
        report(`run ${runIndex}`);

        if (pngPrefix) {
            const rgb = Buffer.alloc(STAGE_W * STAGE_H * 3);
            for (let i = 0; i < STAGE_W * STAGE_H; i++) {
                const c = renderer.pixels[i] < 0 ? 0 : renderer.pixels[i];
                rgb[i * 3] = (c >>> 16) & 0xff;
                rgb[i * 3 + 1] = (c >>> 8) & 0xff;
                rgb[i * 3 + 2] = c & 0xff;
            }
            const big = scaleNearest(STAGE_W, STAGE_H, rgb, 3);
            fs.writeFileSync(`${pngPrefix}-run${runIndex}-stage.png`,
                encodePng(big.width, big.height, big.rgb));
            const words = value('efb_words') || [];
            const diff = Buffer.alloc(STAGE_W * STAGE_H * 3);
            for (let y = 0; y < STAGE_H; y++) {
                const card = readCardRow(words, y);
                for (let x = 0; x < STAGE_W; x++) {
                    const got = renderer.pixels[y * STAGE_W + x];
                    const g = got < 0 ? 0 : got;
                    const c = x < W ? card[x] : 0;
                    const at = (y * STAGE_W + x) * 3;
                    const v = g === c ? 0 : 255;
                    diff[at] = v; diff[at + 1] = v; diff[at + 2] = v;
                }
            }
            const bigDiff = scaleNearest(STAGE_W, STAGE_H, diff, 3);
            fs.writeFileSync(`${pngPrefix}-run${runIndex}-diff.png`,
                encodePng(bigDiff.width, bigDiff.height, bigDiff.rgb));
            warn(`pictures      ${pngPrefix}-run${runIndex}-stage.png and ` +
                `${pngPrefix}-run${runIndex}-diff.png (white = the Stage is not the card)`);
        }
    }

    /// The wall clock at which each row of the card last changed, which is what
    /// "the card is quiet on this row" means.
    function trackCardChanges() {
        const words = value('efb_words') || [];
        const now = Date.now();
        for (let r = 0; r < H; r++) {
            const card = readCardRow(words, r);
            let h = 0;
            for (let x = 0; x < W; x++) h = (h * 31 + card[x]) | 0;
            if (h !== S.cardHash[r]) { S.cardHash[r] = h; S.cardAt[r] = now; }
        }
    }
}

main().catch((err) => {
    console.error('HARNESS FAILURE:', err && err.stack ? err.stack : err);
    process.exit(1);
});
