// Runs the built project and checks it against the Python engine.
//
//   node examples/raven/chess/tools/check.mjs
//
// The Scratch VM is found automatically -- `$SCRATCH_VM_ROOT` if it is set, and
// otherwise the checkouts this repository's READMEs tell you to clone. See
// `tools/vm-root.mjs`. It does not have to be configured to run this.
//
// The project is loaded into a real Scratch VM and its own engine is driven
// through the keys `board.rav` listens for: the checker writes a position into
// the engine's lists, presses a key, and reads back what the engine said.
//
// This is the check that has to pass. It compares
//
//   * the legal move list of the six standard perft positions against the
//     generator in `tools/maia.py`,
//   * perft leaf counts against the published numbers for those positions,
//   * the position the network is asked about, plane for plane, and
//   * the whole policy head, the value, and the move the bot picks.
//
// `tools/maia.py --dump` writes the data it compares against, and `--export`
// wrote the weights the project carries, so a failure here is a disagreement
// between two engines and not a stale fixture.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { loadVm } from "../../../../tools/vm-root.mjs";

const require = createRequire(import.meta.url);
const Module = require("module");
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@scratch/scratch-svg-renderer") {
    return {
      sanitizeSvg: { sanitizeByteStream: (data) => data },
      loadSvgString: () => Promise.resolve(),
      serializeSvgToString: () => "",
    };
  }
  return originalLoad.call(this, request, parent, isMain);
};

// The VM, found wherever it is on this machine rather than requiring the reader
// to set a variable and guess a layout. `tools/vm-root.mjs` is the one place
// that answers it for every example in this repository.
let VirtualMachine;
try {
  ({ VirtualMachine } = loadVm());
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
// The VM is stepped at the frame rate the project is written for, and the
// checker waits for a state the project reaches rather than for a number of
// milliseconds that happen to be long enough on the machine this was written on.
// The pacing is what the project's own `wait 0.025` means: a 30 Hz frame is
// longer than that wait, so the board's loop gets its turn every frame, where
// stepping in a tight loop would run the project slower than its own waits.
const FRAME_MS = 1000 / 30;

async function spin(frames = 1) {
  for (let i = 0; i < frames; i += 1) {
    const began = Date.now();
    vm.runtime._step();
    const left = FRAME_MS - (Date.now() - began);
    if (left > 0) await new Promise((resolve) => setTimeout(resolve, left));
  }
}

/// Spin frames until `test` holds, and say whether it did.
async function until(test, limit = 27000) {
  for (let spent = 0; spent < limit && !test(); spent += 1) await spin(1);
  return test();
}

const { cases, weights } = JSON.parse(readFileSync("examples/raven/chess/tools/checkdata.json", "utf8"));
const bytes = readFileSync("examples/raven/chess/dist/chess.sb3");
console.log(`loading ${(bytes.length / 1048576).toFixed(1)} MiB of project`);
const started = Date.now();
const vm = new VirtualMachine();
await vm.loadProject(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
// The sequencer gives a step a wall-clock budget of 75% of `currentStepTime`, and
// that is null until `runtime.start()` sets it — so without this line a step runs
// no threads at all and every check passes against a machine that never ran.
// Setting it here rather than calling `start()` is what keeps the stepping
// synchronous: one `_step()` is one 30 Hz frame.
vm.runtime.currentStepTime = 1000 / 30;
console.log(`loaded in ${((Date.now() - started) / 1000).toFixed(1)} s`);

const stage = vm.runtime.targets.find((t) => t.getName() === "Stage");

// A raven `list` is a run of an arena, not a Scratch list with a name, so the
// build writes down where each one is (`--debug` keeps `dist/layout.json`).
// This reads a name through that layout, so the rest of the checker can go on
// saying `list("board").value`.
let layout;
try {
  layout = JSON.parse(readFileSync("examples/raven/chess/dist/layout.json", "utf8"));
} catch {
  console.error("no examples/raven/chess/dist/layout.json — build with --debug");
  process.exit(1);
}
const globals = ["_vms", "_heap", "_console"];
const list = (name) => {
  const where = layout.find((entry) => entry.name === name);
  if (!where) throw new Error(`the project has no list called ${name}`);
  const owner = globals.includes(where.list)
    ? stage
    : vm.runtime.targets.find((t) => t.getName() === where.target);
  const variable = owner.lookupVariableByNameAndType(where.list, "list");
  if (where.handle === 0) return variable;
  const arena = variable.value;
  const handle = where.handle;
  return {
    get value() {
      const base = Number(arena[handle - 1]);
      const length = Number(arena[handle]);
      return base === 0 || length === 0 ? [] : arena.slice(base - 1, base - 1 + length);
    },
    set value(next) {
      if (next.length === 0) {
        arena[handle] = 0;
        return;
      }
      let base = Number(arena[handle - 1]);
      const capacity = Number(arena[handle + 2]);
      if (base === 0 || capacity < next.length) {
        // The checker may put more in than the project ever would; the arena is
        // an ordinary array here, so it grows in place.
        base = arena.length + 1;
        for (let i = 0; i < next.length; i += 1) arena.push("");
        arena[handle - 1] = base;
        arena[handle + 2] = next.length;
      }
      for (let i = 0; i < next.length; i += 1) arena[base - 1 + i] = next[i];
      arena[handle] = next.length;
    },
  };
};

// A scalar is one cell, and `--debug` lays it out the same way: the value is the
// item at the handle the entry records.
const cell = (name) => {
  const where = layout.find((entry) => entry.name === name && entry.scalar);
  if (!where) throw new Error(`the project has no scalar called ${name}`);
  const owner = globals.includes(where.list)
    ? stage
    : vm.runtime.targets.find((t) => t.getName() === where.target);
  const arena = owner.lookupVariableByNameAndType(where.list, "list").value;
  return Number(arena[where.handle - 1]);
};

function press(key) {
  vm.runtime.startHats("event_whenkeypressed", { KEY_OPTION: key });
}

/** Write a position into the engine, then press `key` and wait for `test`. */
async function ask(state, key, depth, elo, limitFrames = 27000) {
  list("load").value = state.board.concat(
    [state.side, state.castle, state.ep, state.rule50, state.histn],
    state.hist,
    [depth ?? 0, 0, elo ?? 0],
  );
  list("test").value = [];
  press("s");
  await spin(4);
  press(key);
  // A key script that is not in warp mode yields between frames, so the answer
  // arrives a few values at a time. The list is finished when its length has
  // stopped changing, not when its first value lands.
  let last = -1;
  let settled = 0;
  for (let spent = 0; spent < limitFrames; spent += 1) {
    await spin(1);
    const n = list("test").value.length;
    if (n > 0 && n === last) {
      settled += 1;
      if (settled >= 8) return list("test").value.map(Number);
    } else {
      settled = 0;
    }
    last = n;
  }
  throw new Error(`no answer to ${key} in ${limitFrames} frames`);
}

const unpack = (value) => ({
  from: Math.floor(value / 1000),
  to: Math.floor(value / 10) % 100,
  promo: value % 10,
});

let failures = 0;
function fail(what, detail) {
  failures += 1;
  console.log(`FAIL ${what}`);
  if (detail) console.log(`     ${detail}`);
}

// ---------------------------------------------------------------------------
// The rules
// ---------------------------------------------------------------------------

for (const test of cases.filter((c) => c.legal)) {
  const got = await ask(test.state, "l", 0, 0);
  const n = got[0];
  const mine = new Set(got.slice(1));
  const theirs = new Set(test.legal);
  const missing = [...theirs].filter((m) => !mine.has(m)).map(nameOf);
  const extra = [...mine].filter((m) => !theirs.has(m)).map(nameOf);
  if (n !== test.legal.length || missing.length || extra.length) {
    fail(`legal moves of ${test.name}`,
      `${n} vs ${test.legal.length}; missing ${missing}; extra ${extra}`);
  } else {
    console.log(`ok   legal moves of ${test.name}: ${n}`);
  }
}

for (const test of cases.filter((c) => c.perft)) {
  for (let depth = 1; depth <= test.perft.length; depth += 1) {
    const got = await ask(test.state, "t", depth, 0);
    const want = test.perft[depth - 1];
    if (got[0] !== want) {
      fail(`perft ${depth} of ${test.name}`, `${got[0]} vs ${want}`);
    } else {
      console.log(`ok   perft ${depth} of ${test.name}: ${want}`);
    }
  }
}

function nameOf(value) {
  const m = unpack(value);
  const name = (cell) => "abcdefgh"[(cell % 10) - 1] + (Math.floor(cell / 10) - 1);
  return name(m.from) + name(m.to) + (m.promo ? " nbrq"[m.promo] : "");
}

// ---------------------------------------------------------------------------
// The network
// ---------------------------------------------------------------------------

for (const test of weights) {
  const began = Date.now();
  const load = new Array(680).fill(0);
  load[679] = test.elo;
  list("load").value = load;
  list("test").value = [];
  press("b");
  // Unpacking a network appends to `test` a value at a time, so it is finished
  // when the length stops growing.
  let last = -1;
  let settled = 0;
  for (let spent = 0; spent < 27000; spent += 1) {
    await spin(1);
    const n = list("test").value.length;
    if (n > 0 && n === last) {
      settled += 1;
      if (settled >= 8) break;
    } else {
      settled = 0;
    }
    last = n;
  }
  const seconds = (Date.now() - began) / 1000;
  const w = list("w").value;
  const b = list("b").value;
  let worst = 0;
  let where = "";
  // A missing item reads as NaN, and a NaN must fail rather than slip through
  // every comparison, which is what a short list would otherwise do.
  const worstOf = (list_, pairs, name) => {
    for (const [i, want] of pairs) {
      const got = Number(list_[i]);
      const d = Number.isFinite(got) ? Math.abs(got - want) : Infinity;
      if (d > worst) {
        worst = d;
        where = `${name}${i}`;
      }
    }
  };
  worstOf(w, test.w, "w");
  worstOf(b, test.b, "b");
  // One unit in the last place, which is all that is left of folding the batch
  // norm in a different order.
  if (worst > 1e-12) {
    fail(`the weights of elo ${test.elo}`, `${where} is off by ${worst}`);
  } else {
    console.log(`ok   the weights of elo ${test.elo}: unpacked in ${seconds.toFixed(1)} s`);
  }
}

for (const test of cases.filter((c) => c.head)) {
  const began = Date.now();
  const got = await ask(test.state, "y", 0, test.elo);
  const seconds = (Date.now() - began) / 1000;
  const move = { from: got[0], to: got[1], promo: got[2] };
  const packed = (move.from * 100 + move.to) * 10 + move.promo;
  if (packed !== test.move) {
    fail(`the move for ${test.name}`, `${nameOf(packed)} vs ${nameOf(test.move)}`);
  } else {
    console.log(`ok   the move for ${test.name}: ${nameOf(packed)} in ${seconds.toFixed(1)} s`);
  }
  for (let i = 0; i < 3; i += 1) {
    if (Math.abs(got[3 + i] - test.wdl[i]) > 2e-3) {
      fail(`the value for ${test.name}`, `wdl ${got.slice(3)} vs ${test.wdl}`);
      break;
    }
  }
  const planes = list("planes").value.map(Number);
  {
    let worst = 0;
    let at = 0;
    for (let i = 0; i < test.planes.length; i += 1) {
      const d = Math.abs(planes[i] - test.planes[i]);
      if (d > worst) {
        worst = d;
        at = i;
      }
    }
    if (worst > 1e-6) {
      fail(`the input planes for ${test.name}`, `worst ${worst} at ${at}`);
    } else {
      console.log(`ok   the input planes for ${test.name}`);
    }
  }
  const head = list("bufl2").value.map(Number);
  let worst = 0;
  let at = 0;
  for (let i = 0; i < test.head.length; i += 1) {
    const d = Math.abs(head[i] - test.head[i]);
    if (d > worst) {
      worst = d;
      at = i;
    }
  }
  if (worst > 5e-3) {
    fail(`the policy head for ${test.name}`, `worst ${worst.toFixed(6)} at ${at}`);
  } else {
    console.log(`ok   the policy head for ${test.name}: worst ${worst.toExponential(2)}`);
  }
}

// ---------------------------------------------------------------------------
// The mouse
// ---------------------------------------------------------------------------

// Two clicks: the menu button that starts a game as white, then the pawn on e2
// and the square e4. The board catches them by sampling the mouse in its own
// loop rather than by being clicked, so the press has to last an iteration.
function toCanvas(x, y) {
  return { x: 240 + x, y: 180 - y };
}

function mouse(x, y, isDown) {
  vm.postIOData("mouse", {
    x: 240 + x,
    y: 180 - y,
    isDown,
    canvasWidth: 480,
    canvasHeight: 360,
  });
}

async function click(x, y) {
  mouse(x, y, true);
  await spin(2);
  mouse(x, y, false);
  await spin(2);
}

/** Press on one square, walk the pointer to another, and let go. */
async function drag(from, to) {
  mouse(from.x, from.y, true);
  await spin(2);
  for (let step = 1; step <= 4; step += 1) {
    mouse(
      from.x + ((to.x - from.x) * step) / 4,
      from.y + ((to.y - from.y) * step) / 4,
      true,
    );
    await spin(2);
  }
  mouse(to.x, to.y, false);
  await spin(2);
}

// A square is 36 units, the board's bottom left corner is (-222, -144) and the
// centre of file a's first rank is (-204, -126): the numbers layout.rav holds
// and board.rav tests a click against.
const square = (name) => {
  const file = "abcdefgh".indexOf(name[0]);
  const rank = Number(name[1]) - 1;
  return { x: -204 + file * 36, y: -126 + rank * 36 };
};

function e4moved(what) {
  const played = list("log").value.slice();
  if (played[0] !== "e2e4") {
    fail(what, `the log says ${JSON.stringify(played)}`);
    return;
  }
  // A cell is (rank + 2) * 10 + file + 1, so e2 is 35 and e4 is 55. The board
  // list is one longer than the cell numbers, and the project keeps the same
  // array the engine indexes, so a cell's value is at that index in the list.
  const board = list("board").value.map(Number);
  if (board[55] !== 1 || board[35] !== 0) {
    fail(what, `e2 holds ${board[35]} and e4 holds ${board[55]}`);
    return;
  }
  console.log(`ok   ${what}`);
}

// A green flag draws the menu over a frame or two before anything on it can be
// pressed, and the game page takes the pointer only once `board.rav` says the
// move is yours. Both are states the project reaches, so both are waited for
// rather than timed.
const MENU_FRAMES = 18;

/// The board's own test for a press it should act on: `mode` 1 is "YOUR MOVE",
/// and `mouse.rav` returns early while it is anything else.
const my_move = () => until(() => cell("mode") === 1);
const logged = (n) => until(() => list("log").value.length >= n);

// Two games: one moved by clicking the piece and then its square, and one by
// dragging the piece there. The play button is at (M_PLAY_X, M_PLAY) in
// layout.rav, and `wseat` 0 -- the human -- is the white card's default.
for (const how of ["click", "drag"]) {
  vm.greenFlag();
  await spin(MENU_FRAMES);
  await click(-60, -70);
  await my_move();
  if (how === "click") {
    await click(square("e2").x, square("e2").y);
    await click(square("e4").x, square("e4").y);
  } else {
    await drag(square("e2"), square("e4"));
  }
  await logged(1);
  e4moved(`${how}ing e2 to e4 plays it`);
}

// ---------------------------------------------------------------------------
// A bot on both sides
// ---------------------------------------------------------------------------

// The 1100 network on the white card is the seat at -110, and black is that
// network already, so this is bot against bot: both sides are played by the
// board and the pointer is not an input at all. The ten seats of a card run
// from M_SEAT_X0, -140, on a pitch of 30, and the white card is at M_CARD_A, 52.
vm.greenFlag();
await spin(MENU_FRAMES);
await click(-110, 52);
await click(-60, -70);

// The log only ever grows, so waiting for its first entry and reading that is
// the same however long the networks take to unpack and think. The first move is
// the one `maia.py --dump` gives for the start position.
await logged(1);
const start = cases.find((c) => c.name === "start" && c.elo === 0);
const opened = list("log").value.slice();
if (opened[0] !== nameOf(start.move)) {
  fail("bot against bot", `the log says ${JSON.stringify(opened)}`);
} else {
  console.log(`ok   bot against bot plays ${opened[0]} by itself`);
}

// A bot is to move, and the board refuses the pointer while one is: pressing a
// piece's square and then the square it would move to leaves the selection
// alone. `mode` 2 is "THINKING", and both bots playing the dump's line means
// there is always one to move.
const theirs = await until(() => cell("mode") === 2, 600);
await click(square("e2").x, square("e2").y);
await click(square("e4").x, square("e4").y);
await spin(4);
const picked = cell("sel");
if (!theirs || picked !== 0) {
  fail("a bot's side", `mode ${cell("mode")}, the pointer selected ${picked}`);
} else {
  console.log("ok   a bot's side ignores the pointer");
}

console.log(failures === 0 ? "\nall checks passed" : `\n${failures} check(s) failed`);
process.exit(failures === 0 ? 0 : 1);


