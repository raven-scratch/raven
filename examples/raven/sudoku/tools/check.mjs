#!/usr/bin/env node
/**
 * Check the Sudoku example against a real Scratch VM.
 *
 * The claim this example makes is a strong one — every puzzle it deals can be
 * solved with naked and hidden singles alone, so a player never has to guess —
 * and a claim like that is worth nothing unless something outside the project
 * verifies it. This script does three things:
 *
 *   * it drives the real key hats through the menu, deals a puzzle at each
 *     difficulty, and reads the `puzzle` and `solution` lists out of the VM;
 *   * it solves each puzzle with its own singles-only solver, written here and
 *     sharing no code with the generator, and fails if any puzzle needs a guess
 *     or comes out with more than one answer;
 *   * it plays whole games — one to a win, one to a loss — and fails if the
 *     project does not react.
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
  { level: 1, name: 'easy', target: 50 },
  { level: 2, name: 'medium', target: 44 },
  { level: 3, name: 'hard', target: 40 },
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
  // The VM only steps its threads while it is running.
  vm.start();

  const board = vm.runtime.targets.find((t) => t.getName() === 'Board');
  const list = (name) => {
    const variable = board.lookupVariableByNameAndType(name, 'list');
    return variable ? variable.value.map(Number) : null;
  };
  // The stage declares the shared scalar cells in a known order, so the arena is
  // the only way a test can see `state` and the try count: a `pub var` is a cell
  // of `_gvm`, not a Scratch variable with a name.
  const arena = () => {
    const stage = vm.runtime.targets.find((t) => t.getName() === 'Stage');
    const variable = stage.lookupVariableByNameAndType('_gvm', 'list');
    return variable ? variable.value : [];
  };
  const where = () => {
    const [state, sel, clues, tries, mistakes] = arena();
    return `state=${state} sel=${sel} clues=${clues} tries=${tries} mistakes=${mistakes}`;
  };
  const press = (key) => vm.runtime.startHats('event_whenkeypressed', { KEY_OPTION: key });

  // The HUD finds a character's costume by counting from the first glyph, so the
  // costume order and the `alphabet` list have to be the same list written
  // twice. Nothing else in the project would notice if they drifted: the text
  // would simply come out as the wrong letters. This notices.
  const hud = vm.runtime.targets.find((t) => t.getName() === 'Hud');
  const costumes = hud.sprite.costumes.map((costume) => costume.name);
  const alphabetVariable = hud.lookupVariableByNameAndType('alphabet', 'list');
  const alphabet = alphabetVariable ? alphabetVariable.value.map(String) : [];
  const glyphName = (character) => {
    if (character === ' ') return 'fspace';
    if (character === '-') return 'fdash';
    if (character === ':') return 'fcolon';
    if (character === '/') return 'fslash';
    if (character === '>') return 'fgt';
    return `f${character}`;
  };
  check(costumes[0] === 'dot', `the HUD's first costume is \`${costumes[0]}\`, not the pixel it parks on`);
  check(
    alphabet.length === costumes.length - 1,
    `the HUD has ${costumes.length - 1} glyphs but an alphabet of ${alphabet.length}`,
  );
  alphabet.forEach((character, i) => {
    check(
      glyphName(character) === costumes[i + 1],
      `the HUD's alphabet is out of step at \`${character}\`: costume ${i + 1} is \`${costumes[i + 1]}\``,
    );
  });

  /** Run the VM until `ready` holds, or give up. */
  const until = async (ready, budgetMs = 40000) => {
    const deadline = Date.now() + budgetMs;
    while (Date.now() < deadline) {
      if (ready()) return true;
      await sleep(25);
    }
    return false;
  };

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
    return right.findIndex((delta, i) => delta === 1 && down[i] === 9);
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

  /** The menu, once the flag script has built the tables and cleared the board. */
  const atMenu = () =>
    (list('cells') || []).length === 81 &&
    (list('cells') || []).every((v) => v === 0) &&
    (list('puzzle') || []).length === 0;

  /**
   * Deal a puzzle by walking the menu: the flag always leaves the marker on
   * easy, so a level is `level - 1` presses of down and then Enter.
   */
  const deal = async (level) => {
    vm.greenFlag();
    if (!(await until(atMenu))) return false;
    const before = (list('puzzle') || []).join(',');
    for (let i = 1; i < level; i += 1) {
      press('down arrow');
      await sleep(90);
    }
    press('enter');
    return until(() => {
      const puzzle = list('puzzle') || [];
      const cells = list('cells') || [];
      return (
        cells.length === 81 &&
        puzzle.length === 81 &&
        puzzle.join(',') !== before &&
        cells.join(',') === puzzle.join(',') &&
        puzzle.some((v) => v !== 0)
      );
    }, 90000);
  };

  // --- the menu, and generation ------------------------------------------

  const clueCounts = new Map(DIFFICULTY.map((d) => [d.name, []]));
  for (const difficulty of DIFFICULTY) {
    for (let round = 0; round < ROUNDS; round += 1) {
      const label = `${difficulty.name} ${round + 1}`;
      const dealt = await deal(difficulty.level);
      if (!check(dealt, `${label}: the menu did not deal a puzzle`)) continue;

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
        // A unit that is all clues is a finished line or box handed over, which
        // is most of the puzzle's work given away.
        check(
          seen.length < 9,
          `${label}: a unit came out already finished (${unit.join(',')})`,
        );
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

      console.log(`  ${label.padEnd(9)} ${clues} clues, solved by singles alone`);
      clueCounts.get(difficulty.name).push(clues);
    }
  }

  // The three menu entries have to actually mean something: the easiest deal is
  // never sparser than the hardest one.
  const widest = (name) => Math.max(...clueCounts.get(name));
  const sharpest = (name) => Math.min(...clueCounts.get(name));
  check(
    sharpest('easy') >= widest('medium') && sharpest('medium') >= widest('hard'),
    `the difficulties overlap: easy ${sharpest('easy')}..${widest('easy')}, ` +
      `medium ${sharpest('medium')}..${widest('medium')}, ` +
      `hard ${sharpest('hard')}..${widest('hard')}`,
  );

  // --- a whole game, played through the real key hats --------------------

  console.log('  playing a game through to a win');
  const dealt = await deal(1);
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

    // One press, one move: wait for the cursor to actually change before asking
    // for another. Pressing on a fixed timer outruns the VM — the arrow threads
    // queue up, the cursor overshoots, and a cell in an earlier row can no
    // longer be reached at all.
    const step = async (key) => {
      const before = cursorCell();
      press(key);
      await until(() => cursorCell() !== before, 3000);
    };
    // Walk to the wanted cell by watching where the cursor actually is. Each
    // `move_cursor` redraws the whole board, so presses are a frame apart; a
    // player cannot outrun that, and neither does this.
    const goTo = async (target) => {
      for (let guard = 0; guard < 60; guard += 1) {
        const cell = cursorCell();
        if (cell === target) return true;
        if (row(cell) < row(target)) await step('down arrow');
        else if (row(cell) > row(target)) await step('up arrow');
        else if (col(cell) > col(target)) await step('left arrow');
        else await step('right arrow');
      }
      return false;
    };

    const empty = [];
    for (let i = 0; i < 81; i += 1) if (puzzle[i] === 0) empty.push(i);

    // Fill the first row and watch what a finished line does. The wave steps a
    // clock and redraws for about half a second, so the arena keeps moving while
    // the board is otherwise idle; a tint that simply switched on would leave it
    // completely still, and nothing else about the game would notice.
    for (let c = 0; c < 9; c += 1) {
      if (puzzle[c] === 0) {
        await goTo(c);
        let wrote = false;
        for (let attempt = 0; attempt < 6 && !wrote; attempt += 1) {
          press(String(solution[c]));
          wrote = await until(() => (list('cells') || [])[c] !== 0, 1500);
        }
      }
    }
    const samples = [];
    for (let i = 0; i < 5; i += 1) {
      samples.push(vms().join(','));
      await sleep(110);
    }
    check(
      new Set(samples).size >= 3,
      'a finished line did not animate: the wave never stepped a clock',
    );

    let missed = 0;
    const notes = [];

    for (const cell of empty) {
      const before = (list('cells') || []).slice();
      const reached = await goTo(cell);
      // Press until it takes. Scratch drops a key press that arrives while a
      // thread from the same key hat is still running, and this test presses
      // far faster than a person can; a player would simply press again.
      let landed = false;
      let tries = 0;
      while (!landed && tries < 6) {
        press(String(solution[cell]));
        tries += 1;
        landed = await until(() => (list('cells') || [])[cell] !== 0, 1500);
      }
      if (!landed) {
        missed += 1;
        if (notes.length < 4) {
          const after = list('cells') || [];
          const locks = list('locked') || [];
          const changed = after.map((v, i) => (v !== before[i] ? i : -1)).filter((i) => i >= 0);
          notes.push(
            `wanted ${cell} (reached=${reached}, cursor=${cursorCell()}, ` +
              `locked=${locks[cell]}, cells=${after[cell]}, ` +
              `wrote ${changed.join('/') || 'nothing'}, ${where()})`,
          );
        }
      }
    }
    await until(() => (list('cells') || []).every((v) => v !== 0), 20000);
    // The last entry sets the board's wave off and the card comes after it, so
    // wait for the card rather than stopping the threads that are about to show
    // it — stopping them leaves the game in play with nothing left to play.
    const carded = await until(() => Number(arena()[0]) === 2, 30000);
    check(
      carded,
      `the last entry never reached the win card (${where()}, ` +
        `cells=${(list('flash_cells') || []).length})`,
    );
    vm.stopAll();

    const filled = list('cells') || [];
    const stillEmpty = filled.map((v, i) => (v === 0 ? i : -1)).filter((i) => i >= 0);
    check(
      filled.every((v) => v !== 0),
      `the playthrough left ${stillEmpty.length} cells empty ` +
        `(first ${stillEmpty.slice(0, 8).join(',')}; ${missed} placements did not land; ` +
        `${notes.join('; ')}; ${where()})`,
    );

    // Enter goes back to the menu, and the marker walks again: two downs and
    // Enter must deal the hardest puzzle, which is a different one again. That
    // is also what proves Enter left the card — the menu is the only state where
    // the arrows and Enter do anything.
    //
    // The whole board waves before the card appears, and keys pressed during a
    // wave are refused, so this knocks until the deal happens rather than
    // assuming how long the celebration lasts.
    const won = (list('puzzle') || []).join(',');
    let harder = false;
    for (let attempt = 0; attempt < 40 && !harder; attempt += 1) {
      press('down arrow');
      await sleep(90);
      press('down arrow');
      await sleep(90);
      press('enter');
      harder = await until(() => {
        const next = list('puzzle') || [];
        return (
          next.length === 81 &&
          next.some((v) => v !== 0) &&
          next.join(',') !== won &&
          next.filter((v) => v !== 0).length <= DIFFICULTY[2].target
        );
      }, 700);
    }
    vm.stopAll();
    check(harder, 'after a win, Enter and the menu did not deal hard');
  }

  // --- three mistakes ends the run ---------------------------------------

  console.log('  playing a game through to a loss');
  const again = await deal(1);
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

    for (let i = 0; i < 3; i += 1) {
      press(wrong);
      await sleep(80);
    }
    const after = list('cells') || [];
    check(
      after.every((v, i) => v === before[i]),
      'a wrong digit was written to the board',
    );

    // Once the run is over the board stops taking digits, and Enter twice —
    // card, then menu — deals another puzzle of the same difficulty.
    press(String(solution[at]));
    await sleep(120);
    check(
      (list('cells') || []).every((v, i) => v === before[i]),
      'the board kept taking digits after the run ended',
    );
    let restarted = false;
    for (let attempt = 0; attempt < 20 && !restarted; attempt += 1) {
      press('enter');
      await sleep(300);
      press('enter');
      restarted = await until(() => {
        const next = list('puzzle') || [];
        return (
          next.length === 81 &&
          next.some((v) => v !== 0) &&
          (list('cells') || []).join(',') === next.join(',') &&
          next.filter((v) => v !== 0).length <= DIFFICULTY[0].target
        );
      }, 700);
    }
    vm.stopAll();
    check(restarted, 'after a loss, Enter and the menu did not deal a puzzle');
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
