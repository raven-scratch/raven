#!/usr/bin/env node
/**
 * Check the Sudoku example against a real Scratch VM.
 *
 * The claim this example makes is a strong one — every puzzle it deals can be
 * solved with naked and hidden singles alone, so a player never has to guess —
 * and a claim like that is worth nothing unless something outside the project
 * verifies it. This script does three things:
 *
 *   * it runs the project, asks for a puzzle at each difficulty, and reads the
 *     `puzzle` and `solution` lists straight out of the VM;
 *   * it solves each puzzle with its own singles-only solver, written here and
 *     sharing no code with the generator, and fails if any puzzle needs a guess
 *     or comes out with more than one answer;
 *   * it plays a whole game through the real key hats — including a full win and
 *     a run that loses on three mistakes — and fails if the project does not
 *     react.
 *
 * It needs a checkout of the Scratch VM, like `tools/validate-sb3.js`:
 *
 *   git clone https://github.com/scratchfoundation/scratch-vm ../scratch-vm
 *   cd ../scratch-vm && npm install
 *
 * Then, from the repository root:
 *
 *   cargo run -p raven -- build -m examples/raven/sudoku/raven.toml
 *   SCRATCH_VM_ROOT=../scratch-vm node examples/raven/sudoku/tools/check.mjs
 *
 * Environment:
 *   SCRATCH_VM_ROOT  path to the `scratch-vm` package (defaults to the sibling
 *                    checkout described above).
 *   ROUNDS           how many puzzles to deal per difficulty (default 3).
 */

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');

// The VM is a workspace monorepo and the SVG sanitiser has no built `dist/` in a
// bare checkout. It does not affect block execution, so stub it, exactly as
// `tools/validate-sb3.js` does.
const Module = require('module');
const originalLoad = Module._load;
const STUBS = {
  '@scratch/scratch-svg-renderer': () => ({
    sanitizeSvg: { sanitizeByteStream: (data) => data },
    loadSvgString: () => Promise.resolve(),
    serializeSvgToString: () => '',
  }),
};
Module._load = function (request, parent, isMain) {
  if (Object.prototype.hasOwnProperty.call(STUBS, request)) return STUBS[request]();
  return originalLoad.call(this, request, parent, isMain);
};

const VM_ROOT = process.env.SCRATCH_VM_ROOT
  ? resolve(process.env.SCRATCH_VM_ROOT)
  : resolve(root, '../../../scratch-vm');

let VirtualMachine;
try {
  VirtualMachine = require(join(VM_ROOT, 'src/virtual-machine.js'));
} catch (error) {
  console.error(`Could not load the Scratch VM from ${VM_ROOT}`);
  console.error('Set SCRATCH_VM_ROOT to a scratch-vm checkout.');
  console.error(String(error.message || error));
  process.exit(2);
}

// ---------------------------------------------------------------------------
// The independent solver
// ---------------------------------------------------------------------------

const row = (i) => Math.floor(i / 9);
const col = (i) => i % 9;
const box = (i) => Math.floor(row(i) / 3) * 3 + Math.floor(col(i) / 3);

const PEERS = Array.from({ length: 81 }, (_, i) =>
  Array.from({ length: 81 }, (_, j) => j).filter(
    (j) => j !== i && (row(j) === row(i) || col(j) === col(i) || box(j) === box(i)),
  ),
);

const UNITS = [
  ...Array.from({ length: 9 }, (_, r) => Array.from({ length: 9 }, (_, c) => r * 9 + c)),
  ...Array.from({ length: 9 }, (_, c) => Array.from({ length: 9 }, (_, r) => r * 9 + c)),
  ...Array.from({ length: 9 }, (_, b) =>
    Array.from({ length: 9 }, (_, k) => {
      const br = Math.floor(b / 3);
      const bc = b % 3;
      return (br * 3 + Math.floor(k / 3)) * 9 + bc * 3 + (k % 3);
    }),
  ),
];

const canHold = (grid, i, d) => !PEERS[i].some((p) => grid[p] === d);

/**
 * Solve with naked and hidden singles only — never a guess, never a search.
 * Returns the finished grid, or null when the rules are not enough.
 */
function solveBySingles(givens) {
  const grid = givens.slice();
  for (;;) {
    let progress = false;
    for (let i = 0; i < 81; i += 1) {
      if (grid[i] !== 0) continue;
      const candidates = [];
      for (let d = 1; d <= 9; d += 1) if (canHold(grid, i, d)) candidates.push(d);
      if (candidates.length === 0) return null;
      if (candidates.length === 1) {
        grid[i] = candidates[0];
        progress = true;
      }
    }
    for (const unit of UNITS) {
      for (let d = 1; d <= 9; d += 1) {
        if (unit.some((c) => grid[c] === d)) continue;
        const spots = unit.filter((c) => grid[c] === 0 && canHold(grid, c, d));
        if (spots.length === 0) return null;
        if (spots.length === 1) {
          grid[spots[0]] = d;
          progress = true;
        }
      }
    }
    if (!progress) break;
  }
  return grid.every((v) => v !== 0) ? grid : null;
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const ROUNDS = Number(process.env.ROUNDS || 3);
const DIFFICULTY = [
  { key: '1', level: 'easy', target: 50 },
  { key: '2', level: 'medium', target: 44 },
  { key: '3', level: 'hard', target: 40 },
];

const failures = [];
const check = (ok, message) => {
  if (!ok) failures.push(message);
  return ok;
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const sb3 = join(root, 'dist', 'sudoku.sb3');
  const vm = new VirtualMachine();
  const says = [];
  const errors = [];

  const originalError = console.error;
  console.error = (...args) => {
    const line = args.map(String).join(' ');
    // Node writes its own deprecation warnings here; they are not the project's.
    if (line.includes('DeprecationWarning')) return;
    errors.push(line);
    originalError(`  vm: ${line}`);
  };

  const bytes = readFileSync(sb3);
  await vm.loadProject(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
  vm.runtime.on('SAY', (target, type, text) => says.push(String(text)));
  // The VM only steps its threads while it is running.
  vm.start();

  const board = vm.runtime.targets.find((t) => t.getName() === 'Board');
  const list = (name) => {
    const variable = board.lookupVariableByNameAndType(name, 'list');
    return variable ? variable.value.map(Number) : null;
  };
  const press = (key) => vm.runtime.startHats('event_whenkeypressed', { KEY_OPTION: key });

  // `cursor` is a raven `var`, so it is a cell of the target's arena rather than
  // a Scratch variable a test could look up by name. Its slot is found instead
  // of assumed: a right press adds one and a down press adds nine to exactly one
  // cell, and that cell is the cursor.
  const vms = () => {
    const variable = board.lookupVariableByNameAndType('_vms', 'list');
    return variable ? variable.value.map(Number) : [];
  };
  const calibrateCursor = async () => {
    const see = async (key) => {
      const before = vms();
      press(key);
      await sleep(90);
      const after = vms();
      return after.map((v, i) => v - before[i]);
    };
    const right = await see('right arrow');
    const down = await see('down arrow');
    const slot = right.findIndex((delta, i) => delta === 1 && down[i] === 9);
    return slot;
  };
  let cursorSlot = -1;
  const cursorCell = () => vms()[cursorSlot] - 1; // 0-based

  // The cursor stops where the project clamps it, so pressing up and left
  // enough times leaves it on the top-left cell whatever row and column it
  // started in. The VM steps roughly one of these threads per frame, so this
  // presses once per round and watches, rather than firing twenty and hoping.
  const clampToFirst = async () => {
    for (let round = 0; round < 14; round += 1) {
      if (cursorSlot >= 0 && cursorCell() === 0) return true;
      press('up arrow');
      press('left arrow');
      await sleep(120);
    }
    return cursorSlot >= 0 && cursorCell() === 0;
  };

  /** Run the VM until `ready` holds, or give up. */
  const until = async (ready, budgetMs = 20000) => {
    const deadline = Date.now() + budgetMs;
    while (Date.now() < deadline) {
      if (ready()) return true;
      await sleep(25);
    }
    return false;
  };

  vm.greenFlag();
  await until(() => says.some((s) => s.includes('SUDOKU')), 40000);
  check(
    says.some((s) => s.includes('SUDOKU')),
    'the menu message never appeared, so the tables were never built',
  );
  check((list('cells') || []).length === 81, 'the green flag never built the board');
  vm.stopAll();

  const deal = async (difficulty) => {
    says.length = 0;
    vm.greenFlag();
    // The menu is said at the end of the flag script, after the lookup tables
    // are built. Pressing a difficulty before that would run `start` while
    // `build_tables` is still writing them.
    if (!(await until(() => says.some((s) => s.includes('SUDOKU')), 40000))) return false;
    vm.stopAll();
    says.length = 0;
    press(difficulty.key);
    // `start` finishes by saying the clue count, which is the only signal that
    // the deal is over — the lists themselves still hold the last puzzle.
    return until(
      () => says.some((s) => / clues, \d+ tries$/.test(s)),
      90000,
    );
  };

  // --- generation, and the no-guessing claim -----------------------------

  const clueCounts = new Map(DIFFICULTY.map((d) => [d.level, []]));
  for (const difficulty of DIFFICULTY) {
    for (let round = 0; round < ROUNDS; round += 1) {
      const label = `${difficulty.level} ${round + 1}`;
      const dealt = await deal(difficulty);
      if (!check(dealt, `${label}: no puzzle was dealt`)) continue;

      const puzzle = list('puzzle');
      const solution = list('solution');
      if (!check(solution.length === 81, `${label}: the solution is not 81 cells`)) continue;

      const clues = puzzle.filter((v) => v !== 0).length;
      check(
        solution.every((v) => v >= 1 && v <= 9),
        `${label}: the solution is not a grid of digits`,
      );
      for (const unit of UNITS) {
        const values = unit.map((c) => solution[c]);
        check(
          new Set(values).size === 9,
          `${label}: the solution repeats a digit in a unit`,
        );
      }
      check(
        puzzle.every((v, i) => v === 0 || v === solution[i]),
        `${label}: a clue disagrees with the solution`,
      );
      check(
        clues <= difficulty.target,
        `${label}: ${clues} clues, wanted at most ${difficulty.target}`,
      );
      for (const unit of UNITS) {
        const seen = unit.map((c) => puzzle[c]).filter((v) => v !== 0);
        check(new Set(seen).size === seen.length, `${label}: a unit repeats a clue`);
      }

      const solved = solveBySingles(puzzle);
      if (!check(solved !== null, `${label}: naked and hidden singles are not enough`)) {
        process.stdout.write(
          `  ${label} puzzle ${puzzle.join('')} solution ${solution.join('')}\n`,
        );
        continue;
      }
      check(
        solved.every((v, i) => v === solution[i]),
        `${label}: the puzzle solves to something other than the generated grid`,
      );

      console.log(
        `  ${label.padEnd(9)} ${clues} clues, solved by singles alone`,
      );
      clueCounts.get(difficulty.level).push(clues);
    }
  }

  // The three keys have to actually mean something: the easiest deal is never
  // sparser than the hardest one.
  const widest = (level) => Math.max(...clueCounts.get(level));
  const sharpest = (level) => Math.min(...clueCounts.get(level));
  check(
    sharpest('easy') >= widest('medium') && sharpest('medium') >= widest('hard'),
    `the difficulties overlap: easy ${sharpest('easy')}..${widest('easy')}, ` +
      `medium ${sharpest('medium')}..${widest('medium')}, ` +
      `hard ${sharpest('hard')}..${widest('hard')}`,
  );

  // --- a whole game, played through the real key hats --------------------

  console.log('  playing a game through to a win');
  const dealt = await deal(DIFFICULTY[0]);
  // Calibration drives a cell that can move both right and down, so the cursor
  // goes home first.
  press('up arrow');
  press('left arrow');
  await sleep(200);
  cursorSlot = await calibrateCursor();
  check(cursorSlot >= 0, 'could not find the cursor cell in the arena');
  check(await clampToFirst(), `the clamped cursor is on cell ${cursorCell() + 1}, not the first`);
  if (check(dealt, 'the game could not be dealt for the playthrough')) {
    const puzzle = list('puzzle');
    const solution = list('solution');
    vm.stopAll();

    const step = async (key) => {
      press(key);
      await sleep(40);
    };
    // Walk to the wanted cell by watching where the cursor actually is. Each
    // `move_cursor` redraws the whole board, so presses are one frame apart; a
    // player cannot outrun that, and neither does this.
    const goTo = async (target) => {
      for (let guard = 0; guard < 40; guard += 1) {
        const here = cursorCell();
        if (here === target) return true;
        if (row(here) < row(target)) await step('down arrow');
        else if (col(here) > col(target)) await step('left arrow');
        else await step('right arrow');
      }
      return false;
    };

    const empty = [];
    for (let i = 0; i < 81; i += 1) if (puzzle[i] === 0) empty.push(i);
    says.length = 0;
    let missed = 0;

    for (const cell of empty) {
      await goTo(cell);
      press(String(solution[cell]));
      await sleep(40);
      if ((list('cells') || [])[cell] === 0) missed += 1;
    }
    await until(() => (list('cells') || []).every((v) => v !== 0), 20000);
    vm.stopAll();

    const filled = list('cells') || [];
    const rejected = says.filter((s) => /^not \d/.test(s));
    const stillEmpty = filled.map((v, i) => (v === 0 ? i : -1)).filter((i) => i >= 0);
    check(
      filled.every((v) => v !== 0),
      `the playthrough left ${stillEmpty.length} cells empty ` +
        `(first ${stillEmpty.slice(0, 8).join(',')}; ${missed} placements did not land; ` +
        `${rejected.length} refused)`,
    );
    check(
      says.some((s) => s.includes('solved in')),
      'winning did not announce the puzzle as solved',
    );

    // Enter goes back to the menu, and a new difficulty deals a new puzzle.
    const won = (list('puzzle') || []).slice().join(',');
    press('enter');
    await sleep(200);
    press('2');
    const regrew = await until(() => {
      const next = list('puzzle') || [];
      return next.length === 81 && next.some((v) => v !== 0) && next.join(',') !== won;
    }, 60000);
    vm.stopAll();
    check(regrew, 'after a win, Enter and a difficulty did not deal a new puzzle');
  }

  // --- three mistakes ends the run ---------------------------------------

  console.log('  playing a game through to a loss');
  const again = await deal(DIFFICULTY[0]);
  if (check(again, 'the game could not be dealt for the loss')) {
    const puzzle = list('puzzle');
    const solution = list('solution');
    vm.stopAll();
    const before = (list('cells') || []).slice();
    // The cursor is where the deal left it, on an empty cell; a digit that is
    // not that cell's answer is refused rather than written.
    const at = cursorCell();
    check(puzzle[at] === 0, 'the deal left the cursor on a cell that is not empty');
    const wrong = String((solution[at] % 9) + 1);
    check(wrong !== String(solution[at]), 'the wrong digit was the right digit');

    says.length = 0;
    for (let i = 0; i < 3; i += 1) {
      press(wrong);
      await sleep(60);
    }
    await sleep(120);
    const after = list('cells') || [];
    check(
      after.every((v, i) => v === before[i]),
      'a wrong digit was written to the board',
    );
    check(
      says.some((s) => s.includes('out of tries')),
      'three mistakes did not end the run',
    );

    // Once the run is over the board stops taking digits, and Enter restarts.
    press(String(solution[at]));
    await sleep(120);
    check(
      (list('cells') || []).every((v, i) => v === before[i]),
      'the board kept taking digits after the run ended',
    );
    press('enter');
    await sleep(120);
    press('1');
    const restarted = await until(() => {
      const next = list('puzzle') || [];
      return next.length === 81 && next.some((v) => v !== 0);
    }, 60000);
    vm.stopAll();
    check(restarted, 'after a loss, Enter and a difficulty did not deal a new puzzle');
  }

  console.error = originalError;
  if (errors.length > 0) failures.push(`${errors.length} runtime error(s): ${errors[0]}`);

  if (failures.length > 0) {
    console.log(`\nFAIL ${sb3}`);
    for (const failure of failures) console.log(`  - ${failure}`);
    process.exit(1);
  }
  console.log(`\nPASS ${sb3}`);
  process.exit(0);
}

main().catch((error) => {
  process.stdout.write(
    `HARNESS FAILURE: ${error && error.stack ? error.stack : error}\n`,
  );
  process.exit(1);
});
