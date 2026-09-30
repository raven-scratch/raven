# penfont — the demo of a text library, drawn with the pen

This is the worked example for [`lib/penfont`](../../../lib/penfont), which is
the library itself: [read its README](../../../lib/penfont/README.md) to use it
in a project of your own. What is here is the page around it — a stage, a mode
switch and some keys — over the same engine every consumer gets.

11,587 glyphs of English, digits, punctuation, symbols, simplified and
traditional Chinese, Japanese, Korean and the Nerd Font icons, each filled in
with pen lines from tables the project carries in its own arena. No costumes, no
stamps, and the project never sees a font file, an outline or a curve.

```
src/penfont/font.rav      generated: 11,587 glyphs as 1,215,772 pen runs (15.0 MB)
src/penfont/engine.rav    the library, copied in — do not edit it here
src/sprites/text.rav      the demo: the page layout, the two modes, the keys
src/stage.rav             the paper
icons.txt                 the icons the page draws, which no glyph set gives it
tools/check.mjs           runs the built project in a real Scratch VM and renders the stage
```

```sh
# install the library, taking the glyph sets this page actually sets
python lib/penfont/font2vm.py --project examples/raven/penfont \
  --charset chinese,japanese,korean,nf-dev \
  --chars-file examples/raven/penfont/icons.txt
python lib/penfont/font2vm.py --project examples/raven/penfont --list-charsets
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
edit it in place. `--stats` and `--stage` write nothing; they are the font
checked against FreeType, so they are not the install.

`--charset chinese,japanese,korean,nf-dev` is the point of the sets: the demo
writes those three scripts, the symbols that go with them, and one icon family,
and nothing else. The whole inventory is 35,592 glyphs and 41 MB; this is 11,587
and 15. `--list-charsets` prints every set and bundle with the size this font
gives it.

`icons.txt` is the other half of that: the demo draws eight Material Design
Icons, which are outside the basic plane, and four Devicons, and those twelve
characters are what `--chars-file` adds. Eight of them at U+F0001 and up are two
UTF-16 code units each, which is the case the engine has to join before it can
look one up; taking the whole `nf-md` set for them would be 6,880 glyphs.

The order of the rest matters, and it is the order that catches things. `--stage`
renders the page the project opens on and writes `dist/page.gray`; `check.mjs`
drives the built project and compares the stage it actually drew against that
file. What is in `dist/stage.png` afterwards is the picture, drawn by the pen,
that the VM produced.

| Key | the page | the sheet |
| --- | --- | --- |
| B | | turn between the two |
| click the stage, or Space | type a line; it is drawn as you enter it | **find**: type a character and it turns to its page and rings it |
| Enter | cycle the stock lines, one per script, and one of icons | back to the first page |
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
`PF_EDGE_X` by `PF_EDGE_Y`, which is 225 by 165 either side of the middle, so the
default page is five lines that draw as six rows: 330 units of box, 45 units of
leading at the default size, and seven rows is the most that can ever fit. The
icons row is the sixth and the last one that does — an eleventh glyph on either
Chinese line is 448 units, the measure exactly, and that one wrapped.

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
page: 77 glyphs, 14902 inked pixels at size 34
page: ink x -223.5..181.4, y -104.8..146.5 in a box of +/-225 by +/-165
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
drawn       4172 lines at pen size 1, 0 stamps
expected    4172 lines, scale 0.708
stage       x -224..181, y -104..147, page overlap 1.000
sheet       96 cells, 12866 strokes, x -217..195, y -139..149
find        "中" at 1444, page 16
```

A page overlap of 1.000 is the whole chain agreeing: fontTools, the scanline
converter, the emitted tables, the compiler, the VM, and the pen.

The full guide is at <https://raven-scratch.github.io/raven/>.
