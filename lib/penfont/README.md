# penfont — a text and glyph-sheet library for raven projects

Scratch has no text primitive. A costume cannot carry `<text>`, the pen draws a
line and **has no fill block at all**, so there is nothing to draw a character
with that is not a picture of one. This library draws characters as ink: every
glyph of a real font, filled in one horizontal line per scan row, from tables the
project carries in its own arena. Chinese, Japanese and Korean, Latin, symbols
and the Nerd Font icons — 35,592 glyphs in all, and a project takes the sets it
draws — and the project never sees a font file, an outline or a curve.

Two things come out of it. **Text**, laid out and wrapped, at any size and in any
colour. And the **sheet**: every glyph the table has, twelve by eight to a page,
with a **find** that searches the table and turns to the page a character is on.

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
| `ink_glyph(g, x, y, scale, pen)` | the same, with the scale and the pen width already worked out — the one to call in a loop |
| `stroke_width(em)` | the pen width a size wants |
| `find_glyph(key)` | the index of a key, or 0 |
| `glyph_of(text)` | the index of the glyph a piece of text starts with, reading `\c` |
| `glyph_page(g)` / `sheet_pages()` | which page a glyph is on, and how many there are |
| `PF_EDGE_X`, `PF_EDGE_Y` | the box the pen may be moved inside |
| `PF_LEAD` | baseline to baseline, in ems |
| `PF_COLS`, `PF_ROWS`, `PF_CELL`, `PF_STEP`, `PF_SHEET_LEFT`, `PF_SHEET_TOP`, `PF_SHEET_EM` | the sheet's geometry |
| `FONT_ROWS`, `FONT_CAP` | the table's: scan rows to the em, and the capital's ink height in the same units |

`em` is the size of the em box in stage units, which is what the font's numbers
are fractions of — a CJK glyph is about 0.88 em of ink, a Latin cap about 0.7.
The stage is 480 by 360, so 16 to 48 is the useful range.

`FONT_ROWS` and `FONT_CAP` are the pair that lets you size text by its
*capitals* instead of by its em, which is what a layout usually wants: a capital
`size` units tall is an em of `size * FONT_ROWS / FONT_CAP`, and an advance is
that many rows over. Chess sizes everything that way.

Everything the library exports is named so that a project cannot already be
using the name: `pf_` on a parameter, `PF_` on a constant. That is not tidiness.
A module is compiled into each target that uses it, and neither a parameter nor a
constant keeps its own name there — see the note at the end of this file.

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

The text language stops there: `draw_text` draws the first face and nothing else.
A second face or a different typography is a `find_glyph` and an `ink_glyph` of
your own, which is what chess does — see **A second face** below.

## The box, which is the part that bites

Scratch does not clip a sprite asked to move past the edge of the stage: it
**moves it back**. A run drawn from x = 300 is not a run whose end is off-stage,
it is a run drawn at x = 225 — and a glyph whose rows reach past the edge piles
all of those rows onto the same column, which is a smear, not a letter.

This library cuts every run at `PF_EDGE_X` by `PF_EDGE_Y` first, so it never asks for a
position the fence will not give it. Those 225 by 165 are the *tightest* fence a
costume can give, not the widest — the stage half-width is 240 and Scratch takes
at least 15 off it once a costume is 30 units across — so a page inside this box
fits whatever the sprite is wearing. Keep your own layout inside it too, and if
your page can grow, `font2vm.py --stage` will tell you when it leaves.

## A second face, which is how bold works

`--bold PATH` builds a second font into the same table, under keys that are the
ordinary key with `\b` in front of it. So a bold `H` is `\b\cH`, the bold space
is `\b `, and a caller that builds its own keys gets both faces from one search
and one install — the engine itself needs to know nothing about it.

Chess uses that for its two weights: it walks a string, decides a character's key
from the case and weight the run is in, and calls `find_glyph` and `ink_glyph`
itself, because it needs a line centred on a point, sized by its capitals, and
styled by escapes in the text. That is the shape to copy when `draw_text` is not
what you want: the table and the pen are the library, and the typography is
yours.

## Two things that will bite you, and what to do about them

**A module procedure's parameter is not safe from your names.** A module is
compiled into each target that uses it, and neither a parameter nor a constant
survives that as its own: if the sprite that draws has `var text` and the
procedure takes a parameter called `text`, the procedure gets *your* text,
silently, and a project with its own `EDGE_X` collides with the library's. Every
parameter here is spelled `pf_something` and every constant `PF_something` for
that reason, with the table's own `FONT_*` names kept distinct for the same
reason. When you write a module of your own, do the same.

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
| `--fonts A.ttf,B.ttc:2` | the whole chain, which is also how to have exactly one font |
| `--bold PATH[:FACE]` | a second face, keyed under `\b` |
| `--charset a,b,c` | which glyph sets to take (default `all`) |
| `--list-charsets` | print them all, with the size this font gives each |
| `--text "…"`, `--chars-file FILE` | characters of your own to add |
| `--lay FILE` | the raven file holding your page's layout, for the three checks |

## Choosing the glyphs, which is choosing the size

A project that sets one language should not carry the glyphs of four, so what
goes in the table is a list of named **sets** and `--charset` is which of them to
take. Twenty sets, each one block of characters a project either needs or does
not:

| set | | set | |
| --- | --- | --- | --- |
| `ascii` 95 | printable ASCII | `jis1` 3,486 | JIS X 0208 level 1, the Japanese common kanji |
| `latin1` 96 | Latin-1 | `jis2` 3,390 | JIS X 0208 level 2 |
| `punct` 131 | general punctuation, currency, letterlike | `hangul` 3,185 | KS X 1001 hangul |
| `maths` 116 | arrows, operators, technical | `hanja` 4,888 | KS X 1001 hanja |
| `shapes` 335 | box drawing, blocks, geometric, dingbats | `gb1` 3,755 | GB 2312 level 1, simplified common |
| `fullwidth` 154 | halfwidth and fullwidth forms | `gb2` 3,651 | GB 2312 level 2 and symbols |
| `cjkpunct` 211 | CJK punctuation and compatibility | `big5-1` 5,495 | Big5 level 1, traditional common |
| `kana` 189 | hiragana and katakana | `big5-2` 7,689 | Big5 level 2 |
| `bopomofo` 43 | bopomofo | `ideographs` 20,976 | every CJK ideograph the font has |
| `jamo` 94 | hangul compatibility jamo | | |

and thirteen more for the icons a Nerd Font adds, which are one set per upstream
family so a project pays only for the family it draws:

| set | | set | |
| --- | --- | --- | --- |
| `nf-md` 6,880 | Material Design Icons | `nf-seti` 191 | Seti-UI and Custom |
| `nf-fa` 1,487 | Font Awesome | `nf-fae` 170 | Font Awesome Extension |
| `nf-dev` 496 | Devicons | `nf-logos` 130 | Font Logos |
| `nf-cod` 438 | Codicons | `nf-ple` 40 | Powerline Extra Symbols |
| `nf-oct` 308 | Octicons | `nf-pom` 11 | Pomicons |
| `nf-weather` 228 | Weather Icons | `nf-extra` 12 | Fira Code progress indicators |
| | | `nf-iec` 5 | IEC power symbols |

The counts are Maple Mono NF CN's; `--list-charsets` prints the same table for
whatever font you point it at. The icon sets overlap, because the families do:
`nf-fa` reaches from `0xED00` to `0xF2FF` and so crosses `nf-weather`, `nf-dev`
and `nf-cod`, and the range in an `nf-` name is the upstream range rather than a
claim of disjointness.

Bundles name several at once, which is what a project usually wants:

| bundle | | size |
| --- | --- | --- |
| `basic` | ASCII, Latin-1 and the punctuation a line of English needs | 322 |
| `latin` | plus the symbols a Latin page uses | 773 |
| `korean` | the Latin sets plus fullwidth forms, CJK punctuation and hangul | 3,738 |
| `japanese` | plus kana and the common kanji | 4,407 |
| `chinese` | plus the simplified and traditional common sets | 7,953 |
| `hanzi` | the common Chinese sets, both scripts, both levels | 15,964 |
| `cjk` | everything CJK | 20,541 |
| `icons` | every Nerd Font icon set, the basic plane and the supplementary one | 10,384 |
| `all` | everything, icons included | 35,592 |

Sizes are the *union*, not the sum: every CJK set carries the same punctuation,
and what a reader wants to know is how big the table will be. The table is
roughly 1.3 KB a glyph, so `basic` is about 400 KB and the whole Chinese
inventory about 20 MB; the demo takes `chinese,japanese,korean,nf-dev`, which is
11,587 glyphs and 15 MB.

Every icon family but one lives in the basic plane. `nf-md` is the one that does
not, and that is where Scratch stops being free. A Scratch string is a sequence
of UTF-16 code units, so one of those characters is *two* of them, `letter of`
hands back one code unit, and a table lookup on half a character finds nothing. `font2vm.py`
keeps the character whole in the keys — it sorts in the same code-unit order
Scratch compares in, so the binary search still holds — and `draw_text` and
`glyph_of` in `engine.rav`, and the copy of `walk` in a project's checker,
recognise a high surrogate followed by a low one and look up the pair as one
character. A lone surrogate is refused by the generator rather than written as
half a glyph.

`--chars-file` is the other way to say what a project draws, and it is what a
project with a handful of icons should use rather than a whole family: the demo
here takes `chinese,japanese,korean,nf-dev` and its `icons.txt`, which is twelve
icons and not the 6,880 of a set.

ASCII is in every table whatever you ask for — a table with no space, no digit
and no full stop cannot set a line, and it is 95 glyphs. That also makes
`--charset ascii` the smallest table there is, which is what chess takes: two
weights of the 95 printable characters, 106 KB.

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
