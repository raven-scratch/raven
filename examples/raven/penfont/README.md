# penfont — the demo of a text library, drawn with the pen

This is the worked example for [`lib/penfont`](../../../lib/penfont), which is
the library itself: [read its README](../../../lib/penfont/README.md) to use it
in a project of your own. What is here is the page around it — a stage, a mode
switch and some keys — over the same engine every consumer gets.

15,496 glyphs of English, digits, punctuation, symbols, simplified and
traditional Chinese, Japanese, Korean and the Nerd Font icons, each filled in
with pen lines from tables the project carries in its own arena. No costumes, no
stamps, and the project never sees a font file, an outline or a curve.

```
src/penfont/font.rav      generated: 15,496 glyphs as 1,568,200 pen runs (19 MB)
src/penfont/engine.rav    the library, copied in — do not edit it here
src/sprites/text.rav      the demo: the page layout, the two modes, the keys
src/stage.rav             the paper
tools/check.mjs           runs the built project in a real Scratch VM and renders the stage
```

```sh
python lib/penfont/font2vm.py --project examples/raven/penfont   # install the library
python lib/penfont/font2vm.py --project examples/raven/penfont --stats
cargo run -p raven -- check -m examples/raven/penfont/raven.toml
cargo run -p raven -- build -m examples/raven/penfont/raven.toml --debug
python lib/penfont/font2vm.py --project examples/raven/penfont \
  --stage examples/raven/penfont/dist/page
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm \
  node examples/raven/penfont/tools/check.mjs
```

The first command writes both files under `src/penfont/`, so it is how the
library is installed *and* how it is updated: edit `lib/penfont/engine.rav`, run
it again, and this project has the change. The copy here is generated, so do not
edit it in place.

The order of the rest matters, and it is the order that catches things. `--stage`
renders the page the project opens on and writes `dist/page.gray`; `check.mjs`
drives the built project and compares the stage it actually drew against that
file. What is in `dist/stage.png` afterwards is the picture, drawn by the pen,
that the VM produced.

| Key | the page | the sheet |
| --- | --- | --- |
| B | | turn between the two |
| click the stage, or Space | type a line; it is drawn as you enter it | **find**: type a character and it turns to its page and rings it |
| Enter | cycle the stock lines, one per script | back to the first page |
| Up / Down | larger, smaller | ten pages at a time |
| Left / Right | wrap earlier, wrap later | the next, the previous page |
| 1 – 6 | the ink | the ink |

## What the demo adds

Only three things that are not the library, and they are the three things a
consumer has to decide for themselves.

**Where the page starts.** `LEFT`, `TOP` and `CAP` are this project's choice —
the block is pinned to the top-left of the stage and the first baseline sits 0.9
ems under it. The library is given the baseline and does not care where it is.
The stage is 480 by 360 and the pen may only be moved inside the library's
`EDGE_X` by `EDGE_Y`, which is 225 by 165 either side of the middle, so the
default page is five short lines: 330 units of box, 45 units of leading at the
default size, and seven lines is the most that can ever fit.

**The mode.** `mode` is 0 for the page of text and 1 for the sheet, and B turns
between them. The library draws whichever it is asked for and keeps no state of
its own about which.

**The sheet's state.** `page`, `found` and `query` live here, not in the library:
`draw_sheet` is given a page and the glyph to ring, and a find is a call to the
library's `glyph_of` and `glyph_page` followed by a redraw. That is what makes
the sheet a mode rather than a mechanism.

## The checks

There are two, and neither can see what the other does.

**Before raven, in the library's tool.** `font2vm.py --stats` rasterises a sample
of glyphs from the runs and again through FreeType and reports how much of the
two shapes is the same — mean 0.943, worst 0.755, which is a thin bar like `-`
where hinting puts three rows of ink somewhere the pixel-centre rule does not
pick. `--stage` renders the whole page twice, from the tables and through
FreeType, and asks the question that has to be asked first — does the ink stay
inside the box — and then whether the two pictures are the same:

```
page: 70 glyphs, 14290 inked pixels at size 34
page: ink x -221.4..183.5, y -152.5..146.5 in a box of +/-225 by +/-165
page: tables against FreeType, 1.000 of the pen's ink on the font's,
      1.000 of the font's on the pen's, within a pixel
```

**After the build, here.** `raven check` and `raven build` say the project
type-checks and compiles; nothing so far says the pen put the lines where the
font says. So `tools/check.mjs` hands the `.sb3` to a real Scratch VM with a
renderer that records `penLine` instead of drawing it — with the renderer's own
fence rule, so that a coordinate Scratch would refuse is refused here too — and
drives it the way a reader does, through every mode.

Then it does four things with what came out. It compares the text page's recorded
lines one by one with the tables and the layout rules, which is the diagnosis
when the picture is wrong. It rasterises every stroke the pen made and holds the
stage against `dist/page.gray`, which is the verdict, and writes `dist/stage.png`
to look at. It turns to the sheet and checks the *grid*: all ninety-six cells
have ink, none of them is outside the box, and `dist/sheet.png` is written. And
it finds a character — the ring has to be round the cell that character is in, on
the page it is on, which is a page the sheet had to turn to get to.

```
drawn       4302 lines at pen size 1, 0 stamps
expected    4302 lines, scale 0.708
stage       x -222..183, y -152..147, page overlap 1.000
sheet       96 cells, 12934 strokes, x -217..195, y -139..149
find        "中" at 1502, page 16
```

A page overlap of 1.000 is the whole chain agreeing: fontTools, the scanline
converter, the emitted tables, the compiler, the VM, and the pen.

The full guide is at <https://raven-scratch.github.io/raven/>.
