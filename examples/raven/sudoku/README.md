# sudoku — Sudoku, in raven

A complete Sudoku: a menu, three difficulties, a board you play with the keyboard,
a run you can win and a run you can lose. Every puzzle is generated in the project
while you wait, and every puzzle it deals can be finished with the two rules a
person would use — no guessing, ever.

```sh
cargo run -p raven -- check -m examples/raven/sudoku/raven.toml
cargo run -p raven -- build -m examples/raven/sudoku/raven.toml
node examples/raven/sudoku/tools/assets.mjs    # regenerate the costumes
node tools/validate-sb3.js examples/raven/sudoku/dist/sudoku.sb3 --steps 300
```

The project is one sprite and one stage. The stage holds a backdrop and no state
at all, so the whole game is `src/sprites/board.rav`.

## Controls

| Key | What it does |
| --- | --- |
| `1`, `2`, `3` | In the menu, deal an easy, medium or hard puzzle. In play, put that digit in the current cell. |
| Arrow keys | Move the highlight. It stops at the edges rather than wrapping. |
| `4`–`9` | Put that digit in the current cell. |
| `0` | Rub the current cell out. Sudoku never uses a zero, so the key is free. |
| `Enter` | After a win or a loss, go back to the menu. |

A cell that the puzzle started with cannot be changed or cleared. A wrong digit is
**refused** rather than written, and costs one of three tries; three of them end
the run. Because only a correct entry is ever written, a full board is always the
right board, and there is nothing to check at the end.

The three difficulties are clue counts: easy aims for 50, medium for 44, hard for
40. The number is an aim, not a promise — a pass stops when nothing left in it is
forced, which is the order's decision, so a deal can come out a few clues richer
than the number. The generator never removes a cell it cannot justify, so it takes
what it can get. Measured over a few thousand deals, the construction bottoms out
between 29 and 38 clues, which is why the aims sit where they do: asking for
fewer would only mean asking for something the last pass did not reach.

## Why the puzzles never need a guess

The generator is a construction, not a search, and the argument for it is short
enough to check by hand.

A finished grid comes first, and it comes from one that is already valid. The
pattern `(row * 3 + row / 3 + column) % 9` is a solved Sudoku; permuting whole
bands of rows, whole stacks of columns, and the nine digit names are the three
transformations that keep every grid valid. Composing a random one of each gives a
random finished grid without a single wrong placement to take back.

Then cells are taken away, in a random order, and a removal is **kept only when
the value it removed is still forced** by the cells that remain. A value is forced
when either

* it is the only digit its cell can still hold — a *naked single*; or
* it is the only place its digit can go in its row, its column or its box — a
  *hidden single*.

Those are exactly the two rules. Now the part that makes it a proof rather than a
hope: replay the removals **backwards**.

The last cell removed was forced by the clues that were left at that moment, and
those clues are precisely the puzzle the player is handed — so the player derives
it. The cell removed before that was forced by the clues left at *its* moment,
which is that puzzle plus the cell just derived. Derive that one too, and so on
back to the first removal. Every step is forced, so the solve never needs a guess,
and because every step is forced there is only one grid it can reach — the puzzle
is unique by construction, and the generator never checks for a second solution
because there cannot be one.

One pass of removals can stall on the order it happened to walk, so `dig` runs
another pass in a fresh random order, and stops when a whole pass takes nothing
away. That cannot weaken anything: the test is re-applied at every removal, and a
later pass can only reach cells that became forced since.

There is one trap in the first test that is worth knowing about, because it is
easy to write and it is wrong. The obvious way to ask "is this the cell's only
candidate" is to sum a bit per peer and compare the missing bits against the
digit. But two peers can hold the *same* digit, and a sum counts it twice, so the
missing-bits total can come out equal to one digit's bit while the cell still has
two candidates — a removal let through on a cell nothing forced. Counting the
digits no peer holds cannot do that, and that is what `forced` counts. The bug is
rare enough to survive a hundred hand-played games and common enough to appear in
one deal in a thousand, which is exactly the kind of thing a check that solves
thousands of dealt puzzles is for.

**This is checked, not asserted.** `tools/check.mjs` runs the project in a real
Scratch VM, asks it for puzzles at all three difficulties, and solves each one
with its own singles-only solver, written separately and sharing no code with the
generator. It also plays a whole game through the real key hats — a full win and a
full loss — and fails if the project does not react.

```sh
SCRATCH_VM_ROOT=../scratch-vm node examples/raven/sudoku/tools/check.mjs
```

`ROUNDS` sets how many puzzles are dealt per difficulty (three by default), which
is the knob to turn when the generator changes.

## What it is made of

There is no grid of grids. A board is **81 numbers in one flat list**, and three
lookup tables — `rows`, `cols` and `boxes` — answer "where is this cell" in one
list read, so nothing in the generator ever divides an index. `peers` holds each
cell's twenty companions (its row, its column and its box, without repeats)
twenty in a row, and `units` holds the nine cells of each of the twenty-seven
units, so the solver's inner loops are list reads and nothing else. The tables are
built once per run, in `build_tables`.

Candidate masks are one number per cell — nine bits, one per digit — and `bits[d]`
is the bit for digit `d`. Taking a digit away from a cell is a subtraction, and
"this cell has only one candidate left" is a membership test against that same
list, so there is no bit arithmetic to get wrong.

The drawing is one sprite and the pen. `draw_grid` rules the lines, with every
third one thick for a box; then the highlight is stamped under the digits, one
stamp per filled cell, and the sprite parks on a transparent pixel at the foot of
the stage so its speech bubble is out of the way. The whole frame is one `warp`
procedure.

`assets.mjs` writes the twenty-three costumes: a backdrop, the highlight, the
parking pixel, and a seven-segment digit in two faces — `given` for a clue, `user`
for a value the player entered. A digit is drawn as segments rather than as text
because Scratch deletes `<text>` elements from an SVG, and segments need no font.
Regenerate them rather than editing them.
