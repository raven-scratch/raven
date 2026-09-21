# text — a text engine, in raven

A proportional pixel font, stamped with the pen. Text wraps itself to a column,
each character keeps its own colour, lines are placed by the alignment of the
moment, and the whole frame is rebuilt on every keystroke.

```
src/lib/font.rav              the face: 95 glyphs, their ink widths, the stamp   (generated)
src/lib/text.rav              the engine: buffer, wrapping, alignment, colour
src/sprites/typewriter.rav    the demo: one hat per typeable key, and the arrows
tools/font.mjs                the glyph art, which generates the two above
assets/font/*.svg             one costume per printable character — 38.8 KiB
assets/backdrop.svg           the dark stage the ink is drawn on
```

```sh
raven check           # type check, write nothing
raven build           # dist/text.sb3
raven build --debug   # also dist/asm/ and dist/project.json
raven expand          # the whole engine as raven-asm
```

## Controls

| Key | What it does |
| --- | --- |
| any letter, digit or space | types it |
| ↑ (shift) | holds: letters come out as capitals |
| ← (backspace) | deletes the last character |
| → | walks the alignment through left, centre, right |
| ↓ | walks the colour theme through plain, rainbow, classes, vowels |

## Why the stage fits what it draws

The stage is 480×360. A glyph's cell is **5 units wide and 8 units tall**: a
capital is the top 6 rows, the baseline is row 6 of the cell, and rows 6 and 7
are the descenders. A costume is as wide as its own ink, so the font is
proportional — `i` is one unit, `m` is five — and the engine adds one unit of
tracking between two glyphs.

The column is **216 units**, centred on the middle of the stage, so its left edge
is 108 units out. At the demo's size of 200% one unit is 2 pixels:

| | units | at 200% |
| --- | --- | --- |
| cell | 5 × 8 | 10 × 16 px |
| a wide glyph's advance | 6 | 12 px |
| line pitch | 9 | 18 px |
| the column | 216 | 432 px — 24 px clear on each side |
| a line holds | | 36 wide characters, more when they are narrow |
| 15 lines run from y = 160 down to y = −110 | | in front of the status bar at −150 |

The status bar is the same face at 100%, stamped twice one pixel apart: a bold
line of 6×8-pixel characters across the bottom.

The assets are 95 SVG files, one per printable ASCII character, **38.8 KiB in
total**. Each is a handful of `<rect>` elements — one per run of ink pixels in a
row, not one per pixel — so a glyph is a few hundred bytes. The `.sb3` writer
keys assets by their md5, so two characters that draw the same thing would be
stored once.

## How a character reaches the stage

1. `type_char` (or `write`) appends the character and its colour to `text` and
   `tint`. `text` is a list of single characters — the buffer is data, and
   Scratch variables are not the display.
2. `refresh` re-wraps and redraws. Wrapping works on **indices**: `starts` and
   `ends` are two lists holding the first and last index of each line, so moving
   a word to the next line is moving an index rather than rebuilding strings. A
   line ends at the last space that fits; a word with no space in it is cut where
   the column runs out, which is what any editor does with a word wider than the
   page.
3. `paint` runs in a single `warp` pass: erase the pen layer, then for every line
   stamp each glyph at its spot. A stamp is one block, so a frame is one block per
   character plus the steps of the colour changes.
4. The costume of a glyph is chosen by a `match` over the glyph table, ordered by
   English letter frequency, so the common characters walk a short chain. The
   table's last entry is `?`, which is what anything the font does not have draws.

## Using the engine

`src/lib/text.rav` is the whole interface; a second sprite can `use lib::text`
and get a copy of every proc in its own target.

| Item | What it does |
| --- | --- |
| `write(s)` | appends a string with no redraw |
| `type_char(c)` | appends one character in the case `shift` asks for, and redraws |
| `backspace()` | removes the last character |
| `cycle_align()`, `align` | 0 left, 1 centre, 2 right |
| `cycle_theme()`, `retint()` | the colour of every character already typed |
| `refresh()` | re-wrap and redraw; call it after `write` |
| `reset()` | empty the buffer, back to the default face |
| `size`, `weight` | the two knobs that make a face: 100 is a unit a pixel, 1 stamps every glyph twice |
| `bar`, `BAR_Y` | one bold line of status under the text |
| `shift` | which case `type_char` uses; the caller keeps it up to date |

The engine is honest about three things:

* **Punctuation cannot be typed here.** Scratch's `when key pressed` menu carries
  letters, digits, space, enter and the arrows, and `sensing_keypressed` takes one
  of the same fixed names, so no script can watch `,` or `.`. The font and the
  engine have no such limit — the buffer opens with a line of every printable
  character, and any script may call `type_char(",")`.
* **There is no newline.** `text` holds only glyphs, and wrapping is the only
  line break; a second paragraph is a matter of typing a space.
* **The redraw is per keystroke.** A few hundred characters of this size redraw
  well inside one frame; a program that types thousands would want to redraw the
  line that changed instead of all of them.

## Regenerating the font

```sh
node tools/font.mjs              # writes the assets, src/lib/font.rav, and the
                                 # costume block in src/sprites/typewriter.rav
node tools/font.mjs "Hello!"     # prints a string in the art, to look at a glyph
```

The art is 5 columns of `#` and `.` in `tools/font.mjs`, eight rows a glyph, with
`+N:` to pad blank rows on top. Add a character to the art, run the generator, and
the costume, the table, the widths and the match arm all follow — there is no
second list to keep in step.

## Checking it

```sh
cargo run -p raven -- check -m raven.toml
node ../../tools/validate-sb3.js dist/text.sb3 --steps 400   # needs SCRATCH_VM_ROOT
```

The second one loads the project into the real Scratch VM, runs the green-flag
scripts and fails on an unknown opcode or a runtime error.
