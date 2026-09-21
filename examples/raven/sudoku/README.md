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

The project is one stage and two sprites. `src/sprites/board.rav` is the puzzle:
the generator, the play, and the grid it draws with the pen.
`src/sprites/hud.rav` is every word and number in the game. They share one pen
layer, so the board clears it, draws, and then broadcasts `hud` for the HUD to
write on top; the HUD never clears anything.

## Controls

| Key | What it does |
| --- | --- |
| Up, Down | Move the menu's marker, or the board's cursor. The state decides which. |
| Left, Right | Move the board's cursor. Every move stops at the edge rather than wrapping. |
| `Enter` | Start the difficulty the menu is on, or leave a result card for the menu. |
| `1`–`9` | Put that digit in the current cell. |
| `0` | Rub the current cell out. Sudoku never uses a zero, so the key is free. |

A cell that the puzzle started with cannot be changed or cleared. A wrong digit is
**refused** rather than written, and costs one of three tries; three of them end
the run. Because only a correct entry is ever written, a full board is always the
right board, and there is nothing to check at the end.

The board highlights every cell that holds the same digit as the cell the cursor
is on. With the cursor on a 7, every other 7 reads as one group, which is most of
how a person scans a grid.

The three difficulties are clue counts: easy aims for 50, medium for 44, hard for
40. The number is an aim, not a promise — a pass stops when nothing left in it is
forced, which is the order's decision, so a deal can come out a few clues richer
than the number. The generator never removes a cell it cannot justify, so it takes
what it can get. Measured over a few thousand deals, the construction bottoms out
between 29 and 38 clues, which is why the aims sit where they do: asking for
fewer would only mean asking for something the last pass did not reach.

## The screen

The stage is 480 by 360, and the two halves are worked out from that rather than
guessed. The board is 9 cells of 26, which is 234, and it runs from x = -228 to
x = 6: 12 units of margin at the left edge, and the grid centred vertically in the
63 units of margin above and below it. What is left on the right — from x = 18 to
the edge — is the HUD's column, and the play readout is centred on x = 129, which
is the middle of it. A twelve-character line is 172 wide, so it fits the column
with room to spare and never reaches the board.

The HUD writes at one size: a glyph is 12 by 20 and a character advances 16, so a
long line is 384 wide and still fits the full stage when one is centred on it. The
menu, the wait for a puzzle and the two cards a run ends on are centred on the
stage, because the board is not drawn on any of them — the pen layer is cleared
and left empty for the text. Nothing in the game uses a speech bubble.

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
Scratch VM, walks the menu with the arrow keys and Enter to ask for a puzzle at
each difficulty, and solves each one with its own singles-only solver, written
separately and sharing no code with the generator. It also plays whole games
through the real key hats — one to a win, one to a loss — and fails if the project
does not react.

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

The board draws with the pen in one `warp` procedure: `draw_grid` rules the lines,
with every third one thick for a box; the tint goes down under every cell holding
the cursor's digit; the cursor's outline goes over that; and the digits go last, so
nothing is ever stamped on top of them. Both sprites then park on the same
transparent pixel, so neither is ever visible on the stage.

`assets.mjs` writes every costume. The board's are a backdrop, the parking pixel,
the cursor outline, the tint, and a seven-segment digit in two faces — `given` for
a clue, `user` for a value the player entered. The HUD's are one per character of a
3x5 font: a digit is drawn as segments and a character as pixels, rather than as
text, because Scratch deletes `<text>` elements from an SVG, and neither needs a
font installed on the machine. The font's costumes are written in the order the
HUD's `alphabet` list reads, because the HUD finds a character by counting from the
first glyph — `check.mjs` compares the two lists and fails if they drift apart.
Regenerate the costumes rather than editing them.
