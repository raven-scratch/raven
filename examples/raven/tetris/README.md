# tetris — Tetris, in raven

A 10×20 playfield with a seven-bag randomiser, all seven pieces with four
rotations each, soft and hard drop, gravity that speeds up a level every ten
lines, and line-clear scoring (100 / 300 / 500 / 800 × level).

```
src/stage.rav            the playfield, the rotation table, the bag, the state
src/sprites/board.rav    the game: drawing, input, gravity, rules
src/sprites/hud.rav      the display: a 3×5 pixel font, stamped with the pen
assets/                  a backdrop, nine cell tiles, twenty-two font tiles, sounds
```

```sh
raven check           # type check, write nothing
raven build           # dist/tetris.sb3
raven build --debug   # also dist/asm/ and dist/project.json
raven expand          # the whole game as raven-asm
```

## States and controls

The game opens on a **menu**, plays, can be **paused**, and ends on a
**game-over** card. `state` is that whole machine: 0 menu, 1 playing, 2 paused,
3 over.

| Key | Menu | Playing | Paused | Game over |
| --- | --- | --- | --- | --- |
| Space / Enter | start | hard drop | — | play again |
| ← / → | — | move one column | — | — |
| ↑ | — | rotate clockwise, one cell of wall kick | — | — |
| ↓ | — | soft drop, one point a row | — | — |
| P | — | pause | resume | — |
| R | — | back to the menu | → | → |

**The display is not a monitor.** Scratch deletes `<text>` from SVGs, so a text
display has to be shapes: `hud.rav` owns a 3×5 pixel font — one costume per
digit and one per word — and stamps them with the pen. `score`, `lines` and
`level` are drawn there, along with the menu, the pause card and the final
score, so this project declares **no Scratch variables and no variable
monitors**; the only monitor records left are the hidden ones Scratch gives
every list.

**The display has a layout, not a scatter of stamps.** Every costume is drawn to
its exact ink, so Scratch's default rotation centre — the middle of the costume —
is already the middle of the text, and `go_to_xy` centres a word or a number
without any per-costume offset. On top of that: the sprite runs at 125%, so a
glyph is 15×25 and text advances in 20s; the playfield keeps the left of the
stage, so the stats live in the right column (`x` 100..232) with each label
centred over its value, which means a number that gains a digit grows in both
directions instead of sliding; and each card is one word per line (`PRESS` over
`SPACE`), because side by side those phrases are wider than the field frame and
would run into the column. Numbers are drawn up to five digits, so the score
readout keeps telling the truth once a game passes 999.

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

## Music and sound

All of it is the `sound` extension playing WAV files under `assets/`, and every one
of them is started by `sound::play` — a sample that begins and returns, so nothing
waits and no key handler is held up.

The effects are a move, a rotate, a drop, a clear, a tetris and the game-over
sting, each played by the block that does the thing.

The background music is Korobeiniki: the Russian folk song the game has been
playing since 1984, and in the public domain for a century. It is not recorded
here and it is not a quotation of anyone's arrangement — it is *generated*, from
the tune written as notes:

```sh
node examples/raven/tetris/tools/music.mjs     # writes assets/theme.wav
```

`tools/music.mjs` holds the melody and the bass as note numbers with lengths in
beats, and `tools/synth.mjs` renders them: a band-limited square for the tune, a
triangle for one root a bar, an envelope so no note clicks, and a WAV header
around the result. Twelve and a half seconds of it, which is the loop.

```rav
sound "theme" = "assets/theme.wav";

on flag_clicked {
    forever {
        sound::play_until_done("theme");
    }
}
```

`play_until_done` is the one block that waits, which is exactly what a loop wants:
a Scratch sound cannot be told to loop by itself, so the `forever` is the loop.

The script is on the stage, and it keeps no `let` — that is the condition for
being there. A script's block-scoped cells live in a list named `_stack1`,
`_stack2` and so on, numbered per file; a stage's lists are project-wide and a
sprite's are its own, so a stage script that keeps a `let` and a sprite script
that keeps one both ask for a list called `_stack1` and the compiler refuses the
second as shadowing the first. This project happens not to trip it, but
`examples/raven/sudoku` did the moment its stage grew a `for` loop, so no stage
script in either project keeps a local.

The audio is checked, not assumed: `node tools/check-audio.mjs` reads every WAV
the examples ship, and asks each note of each tune whether the samples at that
moment are really at the pitch the tune gives it — against the semitone either
side, which is the point at which a listener says the note is wrong. It also
checks the theme's peak is *below* the effects', because an effect a player cannot
pick out over the loop is an effect they do not notice: the tune is rendered at
10% and the effects at 80%.

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
