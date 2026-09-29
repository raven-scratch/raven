# penfont — a text and glyph-sheet library for raven projects

Scratch has no text primitive. A costume cannot carry `<text>`, the pen draws a
line and **has no fill block at all**, so there is nothing to draw a character
with that is not a picture of one. This library draws characters as ink: every
glyph of a real font, filled in one horizontal line per scan row, from tables the
project carries in its own arena. Fifteen thousand glyphs of Chinese, Japanese,
Korean, Latin and icons, and the project never sees a font file, an outline or a
curve.

Two things come out of it. **Text**, laid out and wrapped, at any size and in any
colour. And the **sheet**: every glyph the font has, twelve by eight to a page
and 162 pages of them, with a **find** that searches the table and turns to the
page a character is on.

## Installing it

Two files and one command. `font2vm.py` writes both, so installing is the same
command as updating:

```sh
python lib/penfont/font2vm.py --project path/to/your/project
```

That writes

```
path/to/your/project/src/penfont/font.rav      the glyph table
path/to/your/project/src/penfont/engine.rav    this library
```

Then, in the sprite that draws:

```rav
use penfont::engine;

sprite "Text" {
    costume "blank" = "assets/blank.svg";   // one unit square, transparent

    var ink: str = "#1f2430";

    on flag_clicked {
        looks::hide();
        pen::clear();
        draw_text("\\cHello, 世界", -224, 120, 34, ink, 448);
    }
}
```

The sprite has to exist and has to be hidden — the pen draws wherever a sprite
is, and a sprite is the only thing that has one — but a one-unit transparent
costume is all it needs to wear.

## What you get

| | |
| --- | --- |
| `draw_text(content, x, y, em, colour, wide)` | a block of text from the top-left of its first baseline, wrapped inside `wide` (0 is no wrap) |
| `draw_sheet(page, mark, note, colour, accent)` | one page of the glyph sheet; `mark` is a glyph index to ring, `note` a line to print under it |
| `glyph(g, x, y, em, colour)` | one glyph by index |
| `find_glyph(key)` | the index of a key, or 0 |
| `glyph_of(text)` | the index of the glyph a piece of text starts with, reading `\c` |
| `glyph_page(g)` / `sheet_pages()` | which page a glyph is on, and how many there are |
| `EDGE_X`, `EDGE_Y` | the box the pen may be moved inside |
| `LEAD` | baseline to baseline, in ems |
| `COLS`, `ROWS`, `CELL`, `STEP`, `SHEET_LEFT`, `SHEET_TOP`, `SHEET_EM` | the sheet's geometry |

`em` is the size of the em box in stage units, which is what the font's numbers
are fractions of — a CJK glyph is about 0.88 em of ink, a Latin cap about 0.7.
The stage is 480 by 360, so 16 to 48 is the useful range.

## The text language

Scratch compares two strings case-insensitively — `A` is `a` — so a table cannot
be keyed on the character. A capital is marked in the text instead:

```
\cH     a capital H
\\      a literal backslash
\n      a line break, and a real newline in the string does the same
```

A character the font does not carry advances half an em and draws nothing, so a
missing glyph leaves a gap rather than a hole in the line.

## The box, which is the part that bites

Scratch does not clip a sprite asked to move past the edge of the stage: it
**moves it back**. A run drawn from x = 300 is not a run whose end is off-stage,
it is a run drawn at x = 225 — and a glyph whose rows reach past the edge piles
all of those rows onto the same column, which is a smear, not a letter.

This library cuts every run at `EDGE_X` by `EDGE_Y` first, so it never asks for a
position the fence will not give it. Those 225 by 165 are the *tightest* fence a
costume can give, not the widest — the stage half-width is 240 and Scratch takes
at least 15 off it once a costume is 30 units across — so a page inside this box
fits whatever the sprite is wearing. Keep your own layout inside it too, and if
your page can grow, `font2vm.py --stage` will tell you when it leaves.

## Two things that will bite you, and what to do about them

**A module procedure's parameter is not safe from your names.** A module is
compiled into each target that uses it, and a parameter does not survive that as
a local: if the sprite that draws has `var text` and the procedure takes a
parameter called `text`, the procedure gets *your* text, silently. Every
parameter in this library is therefore spelled `pf_something`, which is not a
style but the fix. When you write a procedure of your own that other targets
import, do the same.

**A non-`pub` item in a module is dropped.** `pub proc stroke_width` and not
`proc stroke_width`, or the rest of the module cannot call it. The same goes for
`var`, `const`, `fn` and `macro`.

## The generator

```
python lib/penfont/font2vm.py --project DIR      install into DIR/src/penfont
python lib/penfont/font2vm.py --out DIR          install into DIR instead
python lib/penfont/font2vm.py --stats            check a sample of glyphs, write nothing
python lib/penfont/font2vm.py --stage PREFIX     render the page and check it against FreeType
python lib/penfont/font2vm.py --preview OUT.png  just draw the page
```

| | |
| --- | --- |
| `--set maple` \| `yahei` | which font chain to build from (default `maple`) |
| `--font PATH`, `--face N` | a first font in place of the set's own |
| `--fonts A.ttf,B.ttc:2` | the whole chain |
| `--charset all` \| `latin,cjk,icons` | which standards and blocks to take |
| `--text "…"`, `--chars-file FILE` | characters of your own to add |
| `--lay FILE` | the raven file holding your page's layout, for the three checks |

Without `--charset` or `--text` the inventory is every standard block the font
has — around 15,000 glyphs and 19 MB of table for a CJK font. `--charset latin`
is about 1,600 glyphs and well under a megabyte, which is what a project that
only sets English wants. ASCII is always in, so there is always a space, a digit
and a full stop.

The three checks read `--lay`, a raven file holding `const LEFT`, `const TOP`,
`const CAP`, `var size`, `var limit`, `var ink` and `var text`, so that the page
they check is the page your project draws rather than a copy of the numbers:

```sh
python lib/penfont/font2vm.py --project myproj --stats
python lib/penfont/font2vm.py --project myproj --stage myproj/dist/page
```

`--stats` rasterises a sample of glyphs from the runs and again through FreeType
and reports how much of the two shapes is the same. `--stage` renders the whole
page twice, from the tables and through FreeType, refuses to pass if the ink
leaves the box, and writes `PREFIX-tables.png` and `PREFIX-freetype.png` to look
at. `PREFIX.gray` is the raw page, which is what a check that runs the built
project in a real Scratch VM compares its own stage against —
`examples/raven/penfont/tools/check.mjs` is that check, and it is worth copying
into your project too.

## What it does not do

* **No wrapping at word boundaries.** A line breaks between any two characters,
  which is what Chinese wants and is all English gets.
* **No kerning.** The pen moves by the glyph's own advance and no other
  correction.
* **One weight and one slope.** A bold or italic face is a second `--set`.
* **No hinting.** The raster is what the outlines say at 48 rows to the em.

## Where the numbers are

`FONT_ROWS` is in the generated `font.rav`: 48 scan rows to the em, one unit per
row, so drawing at size S scales every span by S/48. A run is three numbers — its
row, the first column of its ink, and the column past the last one — and
`font_at`, `font_runs` and `font_adv` are a glyph each. `font_chars` is the key
of every glyph, sorted the way Scratch's `<` compares two strings, which is what
makes the binary search over it correct and what makes the sheet's pages the
inventory order.
