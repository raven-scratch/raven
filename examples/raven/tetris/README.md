# tetris — Tetris, in raven

A 10×20 playfield with a seven-bag randomiser, all seven pieces with four
rotations each, soft and hard drop, gravity that speeds up a level every ten
lines, and line-clear scoring (100 / 300 / 500 / 800 × level).

```
src/stage.rav            the playfield, the rotation table, the bag, the state
src/sprites/board.rav    the whole game: drawing, input, gravity, rules
assets/                  a backdrop, nine cell tiles, six sound effects
```

```sh
raven check           # type check, write nothing
raven build           # dist/tetris.sb3
raven build --debug   # also dist/asm/ and dist/project.json
raven expand          # the whole game as raven-asm
```

## Controls

| Key | Action |
| --- | --- |
| ← / → | move one column |
| ↑ | rotate clockwise, with a one-cell nudge off a wall |
| ↓ | soft drop, one point a row |
| Space | hard drop, one point a row, then lock |
| R | new game |

`score`, `lines` and `level` are `watch`ed, so their monitors are the HUD.

## What it is made of

**The playfield is one list of 200 cells.** `0` is empty and `1`..`7` is the
piece that filled it. The current piece is never in the field: it is four offsets
in `shapes`, read against `px`/`py`/`rot`, and only written into the field when
it locks. That is why `fits` is the whole collision system — four cells, each
checked against the walls, the floor and the field.

**The rotation table is data, not arithmetic.** `shapes` holds seven pieces ×
four rotations × four cells, each cell an offset inside a 4×4 box, so rotating is
an index change. The list literal in `src/stage.rav` is the whole table: 112
numbers, generated once and written down.

**One sprite draws the board with the pen.** `redraw` wipes the pen layer and
stamps 200 cells plus the falling piece plus the next-piece preview — about 205
stamps, inside `warp`, so a redraw lands in one frame. There are no clones: 200
clones would be 200 sprites for the renderer to place on every frame.

**The seven-bag lives in a list.** `refill_bag` pushes `1`..`7` and shuffles
them, `next_piece` pops, so no piece can arrive twice before the other six have.

**The rules that matter are the loop's condition.** `hard_drop` falls while
`fits` still says yes — a `proc` call inside `repeat_until`, which raven
re-evaluates every iteration by repeating the hoisted call at the end of the
body. Everything that does more than a block's worth of work is `warp`: the
redraw, the line clear, the spawn, the hard drop.

## Scoring

| Lines | Score |
| --- | --- |
| 1 | 100 × level |
| 2 | 300 × level |
| 3 | 500 × level |
| 4 | 800 × level |

A soft or hard drop adds one point a row. A level is ten lines, and each level
takes 0.05 s off the gravity step, to a floor of 0.1 s.

The full guide is at <https://raven-scratch.github.io/raven/>.
