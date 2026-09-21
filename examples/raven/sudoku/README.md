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

A cell that the puzzle started with cannot be changed, and there is no key for
rubbing a cell out — there would be nothing for it to do. A digit is **refused**
rather than written unless it is the cell's answer, so the board never holds a
value the puzzle did not have and a full board is always the right board. A wrong
digit costs one of three tries, and three of them end the run.

The board highlights every cell that holds the same digit as the cell the cursor
is on. With the cursor on a 7, every other 7 reads as one group, which is most of
how a person scans a grid.

Finishing a row, a column, a box or the whole board sets a wave off along it: the
group tints blue and fades back, and the tint starts a little later on each cell,
so it travels across the group instead of blinking on. The strength is computed
rather than drawn — the tile is one flat blue and the sprite's *ghost* effect is
the ramp, a half sine, which is what puts the fast part of the transition in the
middle: the tint eases away from nothing, crosses blue quickly, and eases back
out.

One placement can finish two things at once — the cell that completes a row and
its box is the usual case, and the last cell of a puzzle finishes the board as
well. They go on a queue rather than each starting its own animation, because a
broadcast restarts the thread that receives it and the second wave would cut the
first one off. The queue is drained in order — row, column, box, and the board
last, with the win card following the board's wave — so every one of them is
played. Two waves that arrive together are why the queue exists; `check.mjs`
fills a cell's row and box around it, presses it, and watches both waves appear in
that order.

A wave waits between steps to animate, and Scratch drops a key press that arrives
while a thread from the same key hat is still running. So nothing slow is allowed
on a key handler: the queue is filled there and one broadcast hands it to a thread
of its own, and the handler that took the key is finished before the animation
starts.

The three difficulties are clue counts: easy aims for 50, medium for 44, hard for
40. The number is an aim, not a promise — a pass stops when nothing left in it is
forced, which is the order's decision, so a deal can come out a few clues richer
than the number. The generator never removes a cell it cannot justify, so it takes
what it can get. Measured over a few thousand deals, the construction bottoms out
between 29 and 38 clues, which is why the aims sit where they do: asking for
fewer would only mean asking for something the last pass did not reach.

No row, column or box is ever handed over already finished. A pass stops the
moment the clue count reaches its target, so a unit the walk never happened to
open can survive with all nine of its cells still clues — a finished line given
away, which is most of the puzzle's work. `open_units` opens one cell in any unit
that came out full, and it can always: with the unit still full, any of its cells
is the only empty one in that unit, so the hidden-single test passes for it and
the removal cannot cost the guarantee.

## The screen

The stage is 480 by 360, and both halves are worked out from that rather than
guessed. The board is 9 cells of 26, which is 234, and it runs from x = -228 to
x = 6: 12 units of margin at the left edge, and the grid centred vertically in the
63 units of margin above and below it. What is left on the right — from x = 18 to
the edge — is the HUD's column, and the play readout is centred on x = 129, which
is the middle of it.

The HUD writes at one size: a glyph costume is 8 by 12 and the sprite runs at
200%, so a character is 16 wide and 24 tall and advances 19. A line of `n`
characters is therefore `(n - 1) * 19 + 16` wide, which is the number each of these
was checked against:

| Text | Characters | Width | Where it lands |
| --- | --- | --- | --- |
| `ARROWS MOVE`, the widest column line | 11 | 206 | 26 to 232 — 8 inside the edge, 20 clear of the board |
| `ENTER FOR THE MENU`, the widest centred line | 18 | 339 | ±169.5, which is 70 inside the stage |
| `TIME 123 SECONDS` | 16 | 301 | ±150.5 |

Vertically a line is 24 tall, so two lines need 24 between them to miss, and the
readout uses 36 between a label and its value and 32 to 70 between everything else:
`CLUES` at 110 and 74, `TRIES` at 22 and -14, then the hints at -80 and -112. That
leaves 58 units above and 56 below. The menu and the cards are spaced the same way,
with nothing closer than 32.

The menu, the wait for a puzzle and the two cards a run ends on are centred on the
stage, because the board is not drawn on any of them — the pen layer is cleared and
left empty for the text. Nothing in the game uses a speech bubble.

Every character is drawn as a stroke — lines and quadratic curves in an SVG path,
with round caps and joins — rather than as a grid of pixels, so a letter is a
letter at any size. The board's digits are strokes too, seven segments of a cell.
Neither needs a font installed on the machine.

A costume's `width`, `height` and `viewBox` are the same box, always, and that is
load-bearing. Scratch draws a costume at its `width` and `height` and does not
scale a `viewBox` that disagrees with them: a costume asking for one size and
declaring another renders at its own units in the top-left corner of the larger
box, which reads as a half-size glyph in the corner rather than an error. The size
a glyph is drawn at is therefore the sprite's, set with `set_size_to`, and
`check.mjs` fails any asset whose viewBox is not its own box at the origin.

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
separately and sharing no code with the generator. It fails a puzzle that needs a
guess, a puzzle whose units are not clean, a unit handed over already finished and
a unit still contradicting the digits, and it compares the HUD's two alphabets.
It also plays whole games through the real key hats — one to a win, one to a loss —
and fails if the project does not react: that a finished line actually animates
rather than switching on, that the last entry reaches the win card, and that the
menu deals again afterwards.

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
the cursor's digit; the wave's tint goes over that; the cursor's outline goes over
that; and the digits go last, so nothing is ever stamped on top of them. Both
sprites then park on the same transparent pixel, so neither is ever visible on the
stage.

`assets.mjs` writes every costume. The board's are a backdrop, the parking pixel,
the cursor outline, the same-digit tint, the blue wave tile, and a seven-segment
digit in two faces — `given` for a clue, `user` for a value the player entered.
The HUD's are one per character of the stroked font, 8 by 12 each: lines and
quadratic curves padded by one unit so the stroke has room, on a glyph box that is
washed in at an opacity no one can see, because a costume's rotation centre is the
middle of its box and a letter narrower than its box would otherwise be centred on
its own ink. The font's costumes are written in the order the HUD's `alphabet`
list reads, because the HUD finds a character by counting from the first glyph —
`check.mjs` compares the two lists, and checks that every asset is 1:1 with its own
viewBox at the origin. Regenerate the costumes rather than editing them.
