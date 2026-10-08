"""A font's outlines -> the pen-span tables a raven project draws text from.

This is the generator half of `lib/penfont`. The other half is `engine.rav`,
which is the raven module that reads what this writes.

    python lib/penfont/font2vm.py --project myproject
    python lib/penfont/font2vm.py --project myproject --list-charsets
    python lib/penfont/font2vm.py --project myproject --charset chinese,japanese
    python lib/penfont/font2vm.py --project myproject --charset basic,icons
    python lib/penfont/font2vm.py --project myproject --font myfont.ttf --bold mybold.ttf
    python lib/penfont/font2vm.py --project myproject --text "Hello 世界"
    python lib/penfont/font2vm.py --project myproject --ligatures
    python lib/penfont/font2vm.py --project myproject --ligatures --features calt,ss01
    python lib/penfont/font2vm.py --project myproject --font 'MapleMono[wght].ttf' --axes
    python lib/penfont/font2vm.py --project myproject --font 'MapleMono[wght].ttf' --instance wght=700

`--project` is the project to install into: `src/penfont` under it, and one run
writes `font.rav` there and copies `engine.rav` beside it, so installing the
library in a project is one command and updating it is the same command again.
Nothing is read from this repository, so the tool can be copied anywhere and run
from anywhere.

Why spans and not outlines: Scratch's pen draws a line and has no fill block at
all, so a glyph is filled the way any concave polygon is filled on a raster --
one horizontal line per scan row, from where a ray enters the ink to where it
leaves it. `examples/raven-asm/poly` does that geometry inside Scratch for a
polygon dragged on the stage; doing it there for every row of every glyph of a
line of text means scanning every edge of the glyph once per row, so it happens
here instead, once per glyph, and the program is left with the drawing.

The raster is one glyph box: `REF_ROWS` scan rows to the em, one unit per row. A
row is sampled on its centre line (y = k + 0.5) and a column is inked when its
centre (x = j + 0.5) is inside the outline, so a run is a pixel span of that
reference raster and drawing it at size S scales by S / REF_ROWS. Winding is
non-zero, which is what TrueType means and what makes the hole in `o` a hole.

Which characters: `--charset` names the sets to take, and each set is one block of
characters a project either needs or does not, so a project pays for the
languages it sets. `GLYPH_SETS` below is the whole of it, one row a set, and
`--list-charsets` prints them all with the size the chosen font gives each.

    ascii, latin1, punct, maths, shapes, fullwidth, cjkpunct, kana, bopomofo,
    jamo, and the thirteen `nf-` sets that are the Nerd Font icons, one to an
    upstream icon family;

    then the codecs, which is what makes the CJK sets mean what their standards
    say rather than what a block happens to contain, read out of Python's own
    codecs so the tool needs no network and "why this character" has a standard's
    name as its answer:

    gb1 and gb2        GB 2312, the simplified Chinese common set and the rest;
    big5-1 and big5-2  Big5, the traditional Chinese common set and the rest;
    jis1 and jis2      JIS X 0208, the Japanese common kanji and the rest;
    hangul and hanja   KS X 1001;
    ideographs         every CJK Unified Ideograph the font has, wherever from.

The bundles in `GLYPH_BUNDLES` name several sets at once -- `basic`, `latin`,
`fullwidth`, `korean`, `japanese`, `chinese`, `hanzi`, `cjk`, `icons`, `all` --
because that is what a project usually asks for. `--text` or `--chars-file` adds
characters of your own. ASCII is always in, so a table always has a space, a
digit and a full stop however narrow it is.

Ligatures: `--ligatures` adds the font's own, read out of its GSUB table rather
than from a list of sequences written here. A programming font draws `->`, `==`
and `<=>` as one glyph each, and this puts what it draws in the table under the
sequence as the key -- one glyph or several composed into one run, since
Cascadia Code's `->` is two halves -- with the advance made the width of the
whole sequence, so the reader only has to look for the longest key that starts
where it is. `--features` chooses which features are read (the default is
`rlig,clig,liga,calt`, and a font's `ss`/`cv` sets can be asked for), and
`PF_LIG_MAX` in `engine.rav` is how long the search is allowed to be, which this
tool reads from there so the two cannot disagree.

Variable fonts: `--axes` prints what a font has, and `--instance wght=700` pins
one to a point on its axes and builds from the static font that draws. An axis
the font does not have is left alone, so a chain can take one specification.

Case: Scratch compares two strings case-insensitively, so `A` and `a` are one
string to the only lookup there is and a table cannot be keyed on the character.
A capital is keyed as the two characters `\\c` and the lowercase letter, and the
text marks it the same way, as `\\cHello`. The keys are sorted by `sort_key`,
which is the lowercased key as UTF-16 bytes -- the code-unit order a
case-insensitive string compare puts them in, which is why a character outside
the basic plane sorts by its first code unit and not last by its code point -- so
`engine.rav` binary-searches the list: a lookup by value (`index_of`) is a
generated procedure that walks the list, far too slow to run for every character
of every line. The search compares a sentinel-prefixed copy of both sides, because
Scratch's own `<` and `=` read a key like `0` or the hex ligature `0x3` as a
number and would search an order this sort is not in.

Output, in the `--out` directory:

    font.rav     the table: `font_chars` the keys, `font_at`/`font_runs`/
                 `font_adv` a glyph each, and `font_run` the runs, three
                 numbers each (row, first column, past the last column).
    engine.rav   a copy of this repository's library module.

Two things read the project back rather than assuming it, and both are optional:

    --stats      rasterise a sample of glyphs from the runs and again through
                 FreeType, and report how much of the two shapes is the same.
    --stage P    render the page the project opens on, from the tables and
                 through FreeType, and refuse to pass if the ink leaves the box
                 Scratch will let the pen be moved inside. This is the check
                 that has to pass before raven is built at all.
    --preview P  just draw the page from the runs, to look at.

Those three need `--lay`, a raven file holding the project's page layout, and
the engine module for the constants the two share.
"""

from __future__ import annotations

import argparse
import math
import re
import sys
from itertools import product
from pathlib import Path

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTCollection

HERE = Path(__file__).resolve().parent
ENGINE = HERE / "engine.rav"

# Where a project put the two files, by default: `src/penfont`, beside whatever
# else its raven sources are.
DEFAULT_OUT = Path("src") / "penfont"
# The demo's page layout, which `--stats`/`--stage`/`--preview` read so that the
# page they check is the page the project draws rather than a copy of the numbers.
DEFAULT_LAY = Path("src") / "sprites" / "text.rav"

# A font set is a first font and the fonts behind it. The first font that has a
# character draws it, so the chain is what covers a gap rather than what mixes
# two designs on purpose.
#
# Neither of these has Hangul, so Malgun Gothic is behind both and Korean comes
# from there. Both paths are this machine's; `--fonts` replaces the chain.
SETS = {
    "maple": {
        "fonts": [
            (r"C:\Users\Dilem\AppData\Local\Microsoft\Windows\Fonts\MapleMono-NF-CN-Regular.ttf", 0),
            (r"C:\Windows\Fonts\malgun.ttf", 0),
        ],
    },
    "yahei": {
        "fonts": [(r"C:\Windows\Fonts\msyh.ttc", 0), (r"C:\Windows\Fonts\malgun.ttf", 0)],
    },
}

REF_ROWS = 48  # scan rows to the em, and the units one row is
TOL = 0.2  # how far a curve may leave its chord before it is split, in rows
CAP = "\\c"
# The features a ligature comes from, in the order they are applied: the ones a
# script cannot do without, then the common and contextual ones. `calt` is where
# a programming font keeps its arrows and operators, and `liga`/`clig` where a
# text font keeps `fi` and `fl`. Discretionary ligatures (`dlig`) are off, which
# is what a reader expects unless they ask.
LIG_FEATURES = ("rlig", "clig", "liga", "calt")
# The longest key the engine will look for, used when `engine.rav` cannot be
# read for its own `PF_LIG_MAX`. A ligature longer than this is not carried: the
# key would never be found, and a table that carries it would be lying about
# what the reader draws.
DEFAULT_LIG_MAX = 4
# The stem a second face is keyed under. `\b` is the character the text language
# would read as an escape, so a table with a bold face in it is a table whose
# keys a caller can build without a convention of its own: the bold glyph for
# `\cH` is `\b\cH`, and the comparison that finds it lowercases both sides, so
# the prefix is the whole of it.
BOLD = "\\b"
# Tuples, not lists: a source is compared and de-duplicated as a whole, and a
# list inside it would make it unhashable.
BIG5_TAIL = tuple(range(0x40, 0x7F)) + tuple(range(0xA1, 0xFF))
EUC_TAIL = tuple(range(0xA1, 0xFF))

# ------------------------------------------------------------ glyph sets --
#
# What goes in the table is a list of named sets, and `--charset` is which of
# them to take. Each set is one block of characters a project either needs or
# does not, so a project that sets one language pays for one language: the whole
# inventory is 19 MB and 15,496 glyphs, ASCII is 106 KB and 95, and a project
# that only ever writes English notices.
#
# A source is either a Unicode range or a range of a two-byte codec, and the
# codec is what makes the Chinese and Japanese sets mean what their standards
# say rather than what a block happens to contain.
#
# ASCII is in every table whatever is asked for. A table with no space, no digit
# and no full stop cannot set a line, and it is 95 glyphs.

_GB1 = range(0xB0, 0xD8)
_GB2 = range(0xD8, 0xF8)
_BIG5_1 = range(0xA4, 0xC7)
_BIG5_2 = range(0xC7, 0xF9)
_JIS_1 = range(0xB0, 0xD0)
_JIS_2 = range(0xD0, 0xF5)
_KR_HANGUL = range(0xB0, 0xC9)
_KR_HANJA = range(0xCA, 0xFE)

#     name, what it is, its sources
GLYPH_SETS = [
    ("ascii", "the 95 printable ASCII characters",
     [("u", 0x0020, 0x007E, "ASCII")]),
    ("latin1", "Latin-1 letters and symbols",
     [("u", 0x00A0, 0x00FF, "Latin-1")]),
    ("punct", "general punctuation, super- and subscripts, currency, letterlike, number forms",
     [("u", 0x2000, 0x206F, "general punctuation"),
      ("u", 0x2070, 0x209F, "super/subscripts"),
      ("u", 0x20A0, 0x20BF, "currency"),
      ("u", 0x2100, 0x214F, "letterlike"),
      ("u", 0x2150, 0x218F, "number forms")]),
    ("maths", "arrows, mathematical operators, miscellaneous technical",
     [("u", 0x2190, 0x21FF, "arrows"),
      ("u", 0x2200, 0x22FF, "maths"),
      ("u", 0x2300, 0x23FF, "misc technical")]),
    ("shapes", "enclosed alphanumerics, box drawing, blocks, geometric shapes, symbols, dingbats",
     [("u", 0x2460, 0x24FF, "enclosed alphanumerics"),
      ("u", 0x2500, 0x257F, "box drawing"),
      ("u", 0x2580, 0x259F, "block elements"),
      ("u", 0x25A0, 0x25FF, "geometric shapes"),
      ("u", 0x2600, 0x26FF, "misc symbols"),
      ("u", 0x2700, 0x27BF, "dingbats")]),
    ("fullwidth", "halfwidth and fullwidth forms, small form variants",
     [("u", 0xFE50, 0xFE6F, "small forms"),
      ("u", 0xFF00, 0xFFEF, "halfwidth/fullwidth")]),
    ("cjkpunct", "CJK punctuation, enclosed CJK, CJK compatibility and forms",
     [("u", 0x3000, 0x303F, "CJK punctuation"),
      ("u", 0x3200, 0x32FF, "enclosed CJK"),
      ("u", 0x3300, 0x33FF, "CJK compatibility"),
      ("u", 0xFE30, 0xFE4F, "CJK forms")]),
    ("kana", "hiragana and katakana",
     [("u", 0x3040, 0x309F, "hiragana"),
      ("u", 0x30A0, 0x30FF, "katakana")]),
    ("bopomofo", "bopomofo", [("u", 0x3100, 0x312F, "bopomofo")]),
    ("jamo", "hangul compatibility jamo", [("u", 0x3130, 0x318F, "hangul jamo")]),
    ("gb1", "GB 2312 level 1 -- the simplified Chinese common set, 3755 characters",
     [("c", "gb2312", _GB1, EUC_TAIL, "GB 2312 level 1")]),
    ("gb2", "GB 2312 level 2 and its symbols -- the rarer simplified characters",
     [("c", "gb2312", range(0xA1, 0xAA), EUC_TAIL, "GB 2312 symbols"),
      ("c", "gb2312", _GB2, EUC_TAIL, "GB 2312 level 2")]),
    ("big5-1", "Big5 level 1 -- the traditional Chinese common set, 5401 characters",
     [("c", "big5", _BIG5_1, BIG5_TAIL, "Big5 level 1")]),
    ("big5-2", "Big5 level 2 -- the rarer traditional characters",
     [("c", "big5", _BIG5_2, BIG5_TAIL, "Big5 level 2")]),
    ("jis1", "JIS X 0208 level 1 and its symbols -- the Japanese common kanji",
     [("c", "euc_jp", range(0xA1, 0xB0), EUC_TAIL, "JIS X 0208 symbols"),
      ("c", "euc_jp", _JIS_1, EUC_TAIL, "JIS X 0208 level 1")]),
    ("jis2", "JIS X 0208 level 2 -- the rarer Japanese kanji",
     [("c", "euc_jp", _JIS_2, EUC_TAIL, "JIS X 0208 level 2")]),
    ("hangul", "KS X 1001 hangul and its symbols -- the Korean common syllables",
     [("c", "euc_kr", range(0xA1, 0xAB), EUC_TAIL, "KS X 1001 symbols"),
      ("c", "euc_kr", _KR_HANGUL, EUC_TAIL, "KS X 1001 hangul")]),
    ("hanja", "KS X 1001 hanja",
     [("c", "euc_kr", _KR_HANJA, EUC_TAIL, "KS X 1001 hanja")]),
    ("ideographs", "every CJK Unified Ideograph the font has, wherever it comes from",
     [("u", 0x4E00, 0x9FFF, "CJK Unified Ideographs")]),

    # The Nerd Font icons, one set an upstream icon set, because that is what
    # Nerd Font is: a bundle of other people's icon fonts patched into the
    # private use area. The ranges are the ones `ryanoasis/nerd-fonts` states in
    # its own `bin/scripts/lib/i_*.sh`, not the runs this font happens to have,
    # so a set means the set and the font decides how much of it is there.
    #
    # `nf-md` is the Material Design Icons, and it is the one icon family outside
    # the basic plane: 6,880 of them at U+F0001 and up, which is why the engine
    # reads a surrogate pair. U+F0001 is two UTF-16 code units, a Scratch string
    # cannot hold it in one, and `letter of` hands back half at a time.
    ("nf-md", "Nerd Font: Material Design Icons, U+F0001 to U+F1AF0",
     [("u", 0xF0001, 0xF1AF0, "Material Design Icons")]),
    ("nf-fa", "Nerd Font: Font Awesome, U+ED00 to U+F2FF",
     [("u", 0xED00, 0xF2FF, "Font Awesome")]),
    ("nf-dev", "Nerd Font: Devicons, U+E700 to U+E958",
     [("u", 0xE700, 0xE958, "Devicons")]),
    ("nf-cod", "Nerd Font: Codicons, U+EA60 to U+EC84",
     [("u", 0xEA60, 0xEC84, "Codicons")]),
    ("nf-oct", "Nerd Font: Octicons, U+F400 to U+F533",
     [("u", 0xF400, 0xF533, "Octicons")]),
    ("nf-weather", "Nerd Font: Weather Icons, U+E300 to U+E3E3",
     [("u", 0xE300, 0xE3E3, "Weather Icons")]),
    ("nf-seti", "Nerd Font: Seti-UI and Custom, U+E5FA to U+E6BB",
     [("u", 0xE5FA, 0xE6BB, "Seti-UI")]),
    ("nf-fae", "Nerd Font: Font Awesome Extension, U+E200 to U+E2A9",
     [("u", 0xE200, 0xE2A9, "Font Awesome Extension")]),
    ("nf-logos", "Nerd Font: Font Logos, U+F300 to U+F385",
     [("u", 0xF300, 0xF385, "Font Logos")]),
    ("nf-ple", "Nerd Font: Powerline Extra Symbols, U+E0A0 to U+E0D7",
     [("u", 0xE0A0, 0xE0D7, "Powerline Extra")]),
    ("nf-pom", "Nerd Font: Pomicons, U+E000 to U+E00A",
     [("u", 0xE000, 0xE00A, "Pomicons")]),
    ("nf-extra", "Nerd Font: Fira Code progress indicators, U+EE00 to U+EE0B",
     [("u", 0xEE00, 0xEE0B, "progress indicators")]),
    ("nf-iec", "Nerd Font: IEC power symbols, U+23FB to U+23FE and U+2B58",
     [("u", 0x23FB, 0x23FE, "IEC power symbols"),
      ("u", 0x2B58, 0x2B58, "IEC power off")]),
]

# A bundle is a name for several sets at once, which is what a project usually
# asks for: `--charset chinese` rather than the seven sets that means.
GLYPH_BUNDLES = [
    ("basic", "ASCII, Latin-1 and the punctuation a line of English needs",
     ["ascii", "latin1", "punct"]),
    ("latin", "the above and the symbols a Latin page uses",
     ["ascii", "latin1", "punct", "maths", "shapes"]),
    ("japanese", "the Latin and fullwidth sets plus kana and the common kanji",
     ["ascii", "latin1", "punct", "maths", "shapes", "fullwidth", "cjkpunct",
      "kana", "jis1"]),
    ("korean", "the Latin and fullwidth sets plus hangul",
     ["ascii", "latin1", "punct", "maths", "shapes", "fullwidth", "cjkpunct",
      "jamo", "hangul"]),
    ("chinese", "the Latin and fullwidth sets plus the simplified and traditional common sets",
     ["ascii", "latin1", "punct", "maths", "shapes", "fullwidth", "cjkpunct",
      "gb1", "big5-1"]),
    ("hanzi", "the common Chinese sets, both scripts, both levels -- the big one",
     ["gb1", "gb2", "big5-1", "big5-2"]),
    ("cjk", "everything CJK: the hanzi, the kana, the hangul, the kanji",
     ["ascii", "latin1", "punct", "maths", "shapes", "fullwidth", "cjkpunct",
      "kana", "bopomofo", "jamo", "gb1", "gb2", "big5-1", "big5-2", "jis1",
      "jis2", "hangul", "hanja"]),
    ("icons", "every Nerd Font icon set -- the `nf-` sets, and the old name for them",
     ["nf-md", "nf-fa", "nf-dev", "nf-cod", "nf-oct", "nf-weather", "nf-seti",
      "nf-fae", "nf-logos", "nf-ple", "nf-pom", "nf-extra", "nf-iec"]),
    ("all", "every set there is, icons included", None),
]

# Old names, kept working: 0.3 shipped `all`, `ascii`, `latin`, `cjk`, `icons`.
CHARSET_ALIASES = {"all": "all", "ascii": "ascii", "latin": "latin", "cjk": "cjk"}

# A set and a bundle with the same name would be two different answers to one
# `--charset` word, and the bundle would silently win. `fullwidth` was both for a
# while; this is what says so at import rather than at the end of a build.
assert not ({name for name, _, _ in GLYPH_SETS} & {name for name, _, _ in GLYPH_BUNDLES}), \
    "a glyph set and a bundle share a name"


def resolve_charset(names, registry):
    """The set names asked for, as sources, in registry order, without repeats.

    Raises SystemExit naming what is unknown, and what the alternatives are.
    """
    available = {name for name, _, _ in registry}
    bundles = {name: sets for name, _, sets in GLYPH_BUNDLES}
    wanted: set[str] = set()
    for name in names:
        name = CHARSET_ALIASES.get(name, name)
        if name in bundles:
            bundle = bundles[name]
            wanted |= set(available if bundle is None else bundle)
        elif name in available:
            wanted.add(name)
        else:
            raise SystemExit(
                f"unknown --charset {name!r}; try --list-charsets "
                f"({', '.join(sorted(bundles) + sorted(available))})")
    sources = []
    seen = set()
    for name, _, src in registry:
        if name not in wanted:
            continue
        for s in src:
            if s in seen:
                continue
            seen.add(s)
            sources.append(s)
    return sources


def charset_catalogue(registry, cmaps):
    """Each set and bundle with how many characters this font actually has.

    A bundle's number is the size of the *union* of the sets it names, not the
    sum: they overlap, every CJK set holds the same punctuation, and the number a
    reader wants is how big the table will be.
    """
    have = set()
    for cm in cmaps:
        have |= set(cm)

    def chars_of(src):
        if src[0] == "u":
            return {chr(c) for c in range(src[1], src[2] + 1) if c in have}
        return {c for c in two_byte_codec(src[1], src[2], src[3]) if ord(c) in have}

    per_set = {name: set().union(*[chars_of(s) for s in src]) if src else set()
               for name, _, src in registry}
    rows = [(name, what, len(per_set[name])) for name, what, _ in registry]
    bundles = []
    for name, what, sets in GLYPH_BUNDLES:
        chosen = set(per_set) if sets is None else set(sets)
        union = set().union(*[per_set[n] for n in chosen]) if chosen else set()
        bundles.append((name, what, len(union)))
    return rows, bundles


# --------------------------------------------------------------- inventory --


def two_byte_codec(enc: str, first, second) -> list[str]:
    """Every character a two-byte codec decodes in those byte ranges."""
    out = []
    for b1 in first:
        for b2 in second:
            try:
                ch = bytes([b1, b2]).decode(enc)
            except UnicodeDecodeError:
                continue
            if len(ch) == 1:
                out.append(ch)
    return out


def key_of(ch: str) -> str:
    """The string the font table is keyed on, and the one the text is read with."""
    low = ch.lower()
    return ch if low == ch else CAP + low


def sort_key(s: str) -> bytes:
    """What Scratch's `<` compares two keys by, so the table can be searched.

    Not the code point. A Scratch string is a string of UTF-16 code units, so a
    character outside the basic plane is two of them and orders by the first --
    below U+E000, where its code point would put it last. Both sides lowercase
    before comparing, which is why this is on the lowercased key.
    """
    return s.lower().encode("utf-16-be")


def load_font(path: str, face: int):
    """A font from either a plain file or a collection, by face index."""
    from fontTools.ttLib import TTFont

    try:
        return TTCollection(path, lazy=True).fonts[face]
    except Exception:
        return TTFont(path, fontNumber=face, lazy=True)


def instantiate(font, location: dict):
    """A variable font pinned to a point on its axes, as the static font it draws.

    A variable font is one file with a weight, a width and so on as axes rather
    than as separate files, and `location` is what to set them to. An axis the
    font does not have is left alone, and a font with no axes at all is handed
    back unchanged, so a chain of a variable font and its fell-back companion
    can take the same specification.
    """
    if not location or font.get("fvar") is None:
        return font
    from fontTools.varLib import instancer

    tags = {a.axisTag for a in font["fvar"].axes}
    here = {tag: value for tag, value in location.items() if tag in tags}
    if not here:
        return font
    return instancer.instantiateVariableFont(font, here, inplace=False, updateFontNames=False)


def variation_axes(font, location: dict):
    """The axis values a source is pinned to, in `fvar` order, for PIL.

    PIL opens the original file and pins it the same way, so the FreeType side
    of `--stats` and `--stage` draws the instance the tables were built from.
    """
    if not location or font.get("fvar") is None:
        return None
    return [location.get(a.axisTag, a.defaultValue) for a in font["fvar"].axes]


def describe_axes(font) -> None:
    """An axis a line, and a named instance a line, of one variable font."""
    name = font["name"].getDebugName(4) or font["name"].getDebugName(1) or "font"
    fvar = font.get("fvar")
    if fvar is None:
        print(f"{name}: no axes")
        return
    print(f"{name}: " + ", ".join(
        f"{a.axisTag} {a.minValue:g}..{a.maxValue:g} (default {a.defaultValue:g})"
        for a in fvar.axes))
    for inst in fvar.instances:
        label = font["name"].getDebugName(inst.subfamilyNameID) or "?"
        coords = ", ".join(f"{a.axisTag}={inst.coordinates[a.axisTag]:g}" for a in fvar.axes)
        print(f"  {label}: {coords}")


def inventory(cmaps, sources, only=None):
    """The characters to draw, keyed and sorted, and the census that found them."""
    def have(cp: int) -> bool:
        # A lone surrogate is not a character and has no key: it is half of one,
        # and a table keyed on half a character draws half an icon. Everything
        # else is in, including outside the basic plane -- a Scratch string is
        # UTF-16, so an icon there is two code units and the engine reads a pair
        # as one character.
        return not (0xD800 <= cp <= 0xDFFF) and any(cp in cm for cm in cmaps)

    census: list[tuple[str, int]] = []
    chars: set[str] = set()
    for src in sources:
        if src[0] == "u":
            _, lo, hi, name = src
            take = [chr(c) for c in range(lo, hi + 1) if have(c)]
        else:
            _, enc, first, second, name = src
            take = [c for c in two_byte_codec(enc, first, second) if have(ord(c))]
        chars.update(take)
        census.append((name, len(take)))
    if only:
        take = [c for c in only if have(ord(c))]
        chars.update(take)
        census.append(("--text", len(take)))

    by_key: dict[str, str] = {}
    for ch in sorted(chars):
        cp = ord(ch)
        # A control code cannot stand in a line of text, and a case mapping that
        # is not one character has no key the reader could build.
        if cp < 0x20 or cp == 0x7F or len(ch.lower()) != 1:
            continue
        by_key.setdefault(key_of(ch).lower(), ch)
    return sorted(by_key, key=sort_key), census


# ------------------------------------------------------------- outline -> ink --


def decompose(font, glyph_name: str, tol_font: float):
    """The glyph's contours as polylines in font units, curves flattened.

    `BasePen` is what turns a recorded glyph into one call per segment and, more
    to the point, resolves TrueType's implied on-curve points -- the midpoint
    between two off-curve points, and the contour that has no on-curve point at
    all -- which a raw recording leaves for the caller.
    """
    contours: list[list[tuple[float, float]]] = []
    current: list[tuple[float, float]] = []

    class Flatten(BasePen):
        def _moveTo(self, pt):
            if len(current) > 2:
                contours.append(current[:])
            current[:] = [pt]

        def _lineTo(self, pt):
            current.append(pt)

        def _qCurveToOne(self, p1, p2):
            quad(current[-1], p1, p2, 0)

        def _curveToOne(self, *pts):
            raise SystemExit("this font is cubic; only quadratic is flattened")

        def _closePath(self):
            if len(current) > 2:
                contours.append(current[:])
            current.clear()

        def _endPath(self):
            self._closePath()

    def quad(p0, p1, p2, depth):
        dx, dy = p2[0] - p0[0], p2[1] - p0[1]
        span = math.hypot(dx, dy)
        if depth >= 12 or span == 0:
            current.append(p2)
            return
        if abs((p1[0] - p0[0]) * dy - (p1[1] - p0[1]) * dx) / span <= tol_font:
            current.append(p2)
            return
        a = ((p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2)
        b = ((p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2)
        m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
        quad(p0, a, m, depth + 1)
        quad(m, b, p2, depth + 1)

    font.getGlyphSet()[glyph_name].draw(Flatten(font.getGlyphSet()))
    if len(current) > 2:
        contours.append(current[:])
    return contours


def spans(contours, upem: int) -> list[tuple[int, int, int]]:
    """Scan rows over the contours -> (row, first column, past the last column)."""
    if not contours:
        return []
    s = REF_ROWS / upem
    scaled = [[(x * s, y * s) for x, y in c] for c in contours]
    ys = [y for c in scaled for _, y in c]
    out: list[tuple[int, int, int]] = []
    for k in range(math.floor(min(ys) - 0.5), math.ceil(max(ys) - 0.5) + 1):
        y = k + 0.5
        crossings: list[tuple[float, int]] = []
        for c in scaled:
            for i in range(len(c)):
                x0, y0 = c[i]
                x1, y1 = c[(i + 1) % len(c)]
                # Half-open in y, so a vertex crossed at is counted once.
                if (y0 <= y < y1) or (y1 <= y < y0):
                    t = (y - y0) / (y1 - y0)
                    crossings.append((x0 + t * (x1 - x0), 1 if y1 > y0 else -1))
        if len(crossings) < 2:
            continue
        crossings.sort()
        wind = 0
        run = 0.0
        for x, d in crossings:
            if wind == 0:
                run = x
            wind += d
            if wind == 0:
                a = math.ceil(run - 0.5)
                b = math.floor(x - 0.5)
                if b >= a:
                    out.append((k, a, b + 1))
    return out


# --------------------------------------------------------------- ligatures --
#
# A ligature is a run of characters the font draws as one glyph: `->` in a
# programming font, `fi` in a text one. It has no code point, so it is not in
# the cmap and the only place it exists is the GSUB table, which is a small
# substitution engine of its own. What is here is the part of that engine a
# ligature needs -- single, ligature and chaining-context substitutions,
# applied in the order the features list them.
#
# The sequences are not written down in this file. A contextual rule names the
# glyphs it matches, so its coverage is a candidate sequence; the candidates are
# shaped, and one that comes back as a single inked glyph with blanks around it
# is a ligature. That is the font's own answer, rather than a list here that a
# font could disagree with.


def ligature_lookups(font, features=LIG_FEATURES):
    """The GSUB lookups the given features are made of, in application order."""
    gsub = font.get("GSUB")
    if gsub is None:
        return []
    by_tag: dict[str, list[int]] = {}
    for rec in gsub.table.FeatureList.FeatureRecord:
        by_tag.setdefault(rec.FeatureTag, []).extend(rec.Feature.LookupListIndex)
    out, seen = [], set()
    for tag in features:
        for li in by_tag.get(tag, []):
            if li not in seen:
                seen.add(li)
                out.append(li)
    return out


def context_match(s, glyphs, pos):
    """A chaining-context subtable's first matching rule, or None.

    The answer is what the record list needs: which rules to apply, how many
    glyphs of the input they consumed, and the buffer they apply to.
    """
    fmt = getattr(s, "Format", None)
    if fmt == 3:
        inp = [set(c.glyphs) for c in s.InputCoverage]
        back = [set(c.glyphs) for c in (s.BacktrackCoverage or [])]
        ahead = [set(c.glyphs) for c in (s.LookAheadCoverage or [])]
        if any(not c for c in inp + back + ahead):
            return None
        if pos + len(inp) > len(glyphs) or pos < len(back):
            return None
        if not all(glyphs[pos + i] in inp[i] for i in range(len(inp))):
            return None
        if not all(glyphs[pos - 1 - i] in back[i] for i in range(len(back))):
            return None
        if pos + len(inp) + len(ahead) > len(glyphs):
            return None
        if not all(glyphs[pos + len(inp) + i] in ahead[i] for i in range(len(ahead))):
            return None
        return glyphs, len(inp), s.SubstLookupRecord
    if fmt == 1 and hasattr(s, "ChainSubRuleSet"):
        cov = s.Coverage.glyphs
        if glyphs[pos] not in cov:
            return None
        idx = cov.index(glyphs[pos])
        if idx >= len(s.ChainSubRuleSet) or not s.ChainSubRuleSet[idx]:
            return None
        for r in s.ChainSubRuleSet[idx].ChainSubRule:
            back = list(r.Backtrack or [])
            more = list(r.Input or [])
            ahead = list(r.LookAhead or [])
            if pos < len(back):
                continue
            if any(glyphs[pos - 1 - i] != back[i] for i in range(len(back))):
                continue
            if glyphs[pos + 1:pos + 1 + len(more)] != more:
                continue
            if glyphs[pos + 1 + len(more):pos + 1 + len(more) + len(ahead)] != ahead:
                continue
            return glyphs, 1 + len(more), r.SubstLookupRecord
    return None


def apply_lookup(font, li, glyphs, pos):
    """Apply GSUB lookup `li` anchored at `pos` -> (glyphs, advance, matched).

    A rule that matched but records nothing still answers `True`: in the real
    engine the first matching rule at a position wins, and a guard rule is how a
    font stops a shorter ligature from forming inside a longer one.
    """
    lookup = font["GSUB"].table.LookupList.Lookup[li]
    typ = lookup.LookupType
    if typ == 1:
        for s in lookup.SubTable:
            if glyphs[pos] in s.mapping:
                return glyphs[:pos] + [s.mapping[glyphs[pos]]] + glyphs[pos + 1:], 1, True
        return glyphs, 1, False
    if typ == 4:
        best = None
        for s in lookup.SubTable:
            for lig in s.ligatures.get(glyphs[pos], []):
                comp = list(lig.Component)
                if glyphs[pos + 1:pos + 1 + len(comp)] == comp:
                    if best is None or len(comp) > len(best[1]):
                        best = (lig.LigGlyph, comp)
        if best:
            return glyphs[:pos] + [best[0]] + glyphs[pos + 1 + len(best[1]):], 1 + len(best[1]), True
        return glyphs, 1, False
    if typ == 6:
        for s in lookup.SubTable:
            got = context_match(s, glyphs, pos)
            if got is None:
                continue
            out, consumed, records = got
            for rec in records:
                out, _, _ = apply_lookup(font, rec.LookupListIndex, out, pos + rec.SequenceIndex)
            return out, consumed, True
    return glyphs, 1, False


def shape(font, glyphs, lookups):
    """Run the lookups, in order, over a glyph buffer and hand back what is left."""
    for li in lookups:
        if font["GSUB"].table.LookupList.Lookup[li].LookupType not in (1, 4, 6):
            continue
        pos = 0
        while pos < len(glyphs):
            glyphs, advance, _ = apply_lookup(font, li, glyphs, pos)
            pos += max(1, advance)
    return glyphs


def combine(choices, cap):
    """Every sequence a rule's positions can stand for, or nothing.

    A position is a list of the strings its glyph can stand for -- one character
    for a glyph with a code point, a whole sequence for a ligature already
    found -- and a rule is a sequence of those, so the answer is their product.
    A rule with no answer at some position is dropped: `SPC`, the blank a font
    leaves where a ligature swallowed a character, is the usual one. A rule
    whose product is enormous is dropped too, rather than explored.
    """
    if not choices or len(choices) > cap or any(not c for c in choices):
        return set()
    total = 1
    for c in choices:
        total *= len(c)
    if total > 4096:
        return set()
    return {"".join(combo) for combo in product(*choices)}


def glyph_choices(name, resolve):
    """What one glyph stands for, or None for a blank or a component."""
    got = resolve(name)
    return sorted(got) if got else None


def coverage_choices(coverage, resolve):
    """What a coverage class stands for, or None if nothing in it does."""
    out = set()
    for g in coverage.glyphs:
        got = resolve(g)
        if got:
            out.update(got)
    return sorted(out) if out else None


def ligature_candidates(font, lookups, resolve, cap, backtrack=False):
    """The sequences the font's own rules could draw as one glyph.

    Only a rule that substitutes something is a candidate. Its input is the
    characters it matches and its lookahead the ones after, and its backtrack
    the ones before, so the three together are the sequence it acts on and the
    substitution lands on the last character of it. This over-generates on
    purpose -- a candidate that does not shape to a ligature is dropped a moment
    later -- because a font's rules overlap and the shaping is the only thing
    that decides.

    The backtrack is only read once the input alone has been tried, because the
    characters it names have usually been substituted already by a shorter
    ligature, which is what the round of `font_ligatures` that asks for it is
    for; taking it in the first pass multiplies the candidates for nothing.
    """
    out = set()
    table = font["GSUB"].table
    for li in lookups:
        lookup = table.LookupList.Lookup[li]
        for s in lookup.SubTable:
            if lookup.LookupType == 4:
                for first, ligs in s.ligatures.items():
                    for lig in ligs:
                        names = [first] + list(lig.Component)
                        choices = [glyph_choices(n, resolve) for n in names]
                        if all(c is not None for c in choices):
                            out |= combine(choices, cap)
            elif lookup.LookupType == 6:
                fmt = getattr(s, "Format", None)
                if fmt == 3:
                    if not s.SubstLookupRecord:
                        continue
                    coverages = (list(reversed(s.BacktrackCoverage or [])) if backtrack else []) \
                        + list(s.InputCoverage) + list(s.LookAheadCoverage or [])
                    choices = [coverage_choices(c, resolve) for c in coverages]
                    if all(c is not None for c in choices):
                        out |= combine(choices, cap)
                elif fmt == 1 and hasattr(s, "ChainSubRuleSet"):
                    cov = s.Coverage.glyphs
                    for i, rules in enumerate(s.ChainSubRuleSet):
                        if not rules or i >= len(cov):
                            continue
                        for r in rules.ChainSubRule:
                            if not r.SubstLookupRecord:
                                continue
                            names = (list(reversed(r.Backtrack or [])) if backtrack else []) + [cov[i]] \
                                + list(r.Input or []) + list(r.LookAhead or [])
                            choices = [glyph_choices(n, resolve) for n in names]
                            if all(c is not None for c in choices):
                                out |= combine(choices, cap)
    return out


def detect_ligature(font, seq, cmap, no_ink, hmtx, lookups):
    """What a sequence shapes to, or None: (glyphs, advance, inked).

    A sequence is a ligature when shaping it changes it and something draws ink.
    The font may do that as one glyph -- Maple Mono's `==` is one glyph with the
    first character blanked -- or as several: Cascadia Code's `->` is a hyphen
    half and a greater half, and neither stands for the whole run on its own.
    `inked` is how many of the result carry ink, which is what says whether one
    of them can be read back as the whole sequence.
    """
    base = [cmap.get(ord(ch)) for ch in seq]
    if not all(base):
        return None
    out = shape(font, base, lookups)
    if out == base:
        return None
    inked = [g for g in out if not no_ink(g)]
    if not inked:
        return None
    return tuple(out), sum(hmtx[g][0] for g in out), inked


def font_ligatures(font, cap, features=LIG_FEATURES):
    """The font's ligatures: (key, glyph names, advance) each.

    `key` is the lowercased sequence and `glyph names` the glyphs the font draws
    it with, in order and each at the pen position the one before it leaves; the
    caller composes them into the runs the pen draws. The advance is the width
    of the whole sequence, in font units.

    A ligature can be made of other ligatures -- a font that draws `-------`
    extends the run it has already drawn -- so the rules are read again with
    every ligature found so far standing for the characters it replaced, until a
    pass learns nothing it did not know. That is what makes a chain of rules
    come out as one sequence.
    """
    if font.get("GSUB") is None:
        return []
    cmap = font.getBestCmap()
    rev: dict[str, str] = {}
    for cp, name in cmap.items():
        rev.setdefault(name, chr(cp))
    gs = font.getGlyphSet()
    hmtx = font["hmtx"]
    inkless: dict[str, bool] = {}

    def no_ink(name: str) -> bool:
        if name not in inkless:
            from fontTools.pens.boundsPen import BoundsPen

            pen = BoundsPen(gs)
            gs[name].draw(pen)
            inkless[name] = not pen.bounds
        return inkless[name]

    lookups = ligature_lookups(font, features)
    found: dict[str, tuple] = {}
    known: dict[str, str] = {}

    def resolve(name):
        if name in rev:
            return [rev[name]]
        seq = known.get(name)
        return [seq] if seq else None

    for round_ in range(6):
        learned = False
        for seq in sorted(ligature_candidates(font, lookups, resolve, cap, backtrack=round_ > 0)):
            # A backslash or a break cannot stand in the text language's key, so
            # a ligature that needs one is not a key a reader could ever build.
            if not 2 <= len(seq) <= cap or any(ch in "\\\n\r" for ch in seq):
                continue
            got = detect_ligature(font, seq, cmap, no_ink, hmtx, lookups)
            if got is None:
                continue
            out, advance, inked = got
            found.setdefault(seq.lower(), (out, advance))
            # Only a glyph that draws the whole run on its own can stand for the
            # characters of the sequence in a longer rule; a half of an arrow
            # stands for half of it and is no use to the next round.
            if len(inked) == 1 and inked[0] not in known:
                known[inked[0]] = seq
                learned = True
        if not learned:
            break
    return [(key,) + found[key] for key in sorted(found)]


def build_ligatures(font, cap, stem: str = "", src: int = 0,
                    features=LIG_FEATURES) -> list["Glyph"]:
    """The font's ligatures as table rows, keyed by the whole sequence.

    The glyphs the font draws the sequence with are composed into one run of ink,
    each at the pen position the one before it leaves, and the advance is the
    width of the whole sequence rather than of any one glyph, so the reader
    consumes every character the ligature stands for and moves the pen past all
    of it.
    """
    upem = font["head"].unitsPerEm
    hmtx = font["hmtx"]
    tol = TOL * upem / REF_ROWS
    glyphs = []
    for key, names, advance in font_ligatures(font, cap, features):
        contours, offset = [], 0
        for name in names:
            for c in decompose(font, name, tol):
                contours.append([(x + offset, y) for x, y in c])
            offset += hmtx[name][0]
        glyphs.append(Glyph(stem + key, key, spans(contours, upem),
                            round(advance * REF_ROWS / upem, 4), src, True))
    return glyphs


class Glyph:
    __slots__ = ("key", "char", "runs", "adv", "src", "lig")

    def __init__(self, key: str, char: str, runs, adv: int, src: int, lig: bool = False):
        self.key, self.char, self.runs, self.adv, self.src, self.lig = key, char, runs, adv, src, lig


def build(fonts, keys: list[str], stem: str = "", src_offset: int = 0) -> list[Glyph]:
    glyphs = []
    for key in keys:
        char = key[len(CAP):] if key.startswith(CAP) else key
        if key.startswith(CAP):
            # The key is `\c` and the lowercase letter, so the glyph it names is
            # the capital: reading it back as the lowercase letter would draw
            # every capital as a small one.
            char = char.upper()
        for which, font in enumerate(fonts):
            cmap = font.getBestCmap()
            name = cmap.get(ord(char))
            if name is None:
                continue
            upem = font["head"].unitsPerEm
            tol_font = TOL * upem / REF_ROWS
            runs = spans(decompose(font, name, tol_font), upem)
            # The advance keeps its fraction: a run is placed glyph by glyph, so
            # rounding this to a whole scan row would drift the end of a line by
            # a couple of percent of its own width. The runs are integers because
            # they are a raster; the advance is not, because it is not.
            adv = round(font["hmtx"][name][0] * REF_ROWS / upem, 4)
            glyphs.append(Glyph(stem + key, char, runs, adv, which + src_offset))
            break
    return glyphs


def cap_height(font) -> float:
    """The capital's ink height, in reference rows, which is how a caller that
    sizes text by its capitals converts a size into an em."""
    upem = font["head"].unitsPerEm
    os2 = font.get("OS/2")
    units = getattr(os2, "sCapHeight", 0) if os2 else 0
    if not units:
        # No `OS/2`, or one that does not say: measure an `H`, which is what a
        # cap height is.
        from fontTools.pens.boundsPen import BoundsPen

        name = font.getBestCmap().get(ord("H"))
        if name is not None:
            pen = BoundsPen(font.getGlyphSet())
            font.getGlyphSet()[name].draw(pen)
            units = pen.bounds[3] if pen.bounds else 0
    return round(units * REF_ROWS / upem, 4)


# ------------------------------------------------------------------- output --


def rav_string(s: str) -> str:
    """A raven string literal for any key: every character as `\\u{…}`."""
    return '"' + "".join(f"\\u{{{ord(ch):04X}}}" for ch in s) + '"'


def wrap(items: list[str], width: int = 108, indent: int = 4) -> str:
    pad = " " * indent
    lines, line = [], pad
    for i, item in enumerate(items):
        piece = item + ("," if i + 1 < len(items) else "")
        if line.strip() and len(line) + len(piece) > width:
            lines.append(line.rstrip())
            line = pad
        line += piece + " "
    if line.strip():
        lines.append(line.rstrip())
    return "\n".join(lines)


def emit(glyphs: list[Glyph], path: Path, name: str, cap: float) -> None:
    chars = [g.key for g in glyphs]
    ats, counts, advs, flat = [], [], [], []
    # `font_at` is where a glyph's runs start *in `font_run`*, which is three
    # numbers a run, not a run count. The engine reads `font_run[at + 3k]`, so
    # counting runs here instead of numbers puts every glyph on the third of the
    # table that its own index lands in -- which draws, and draws nothing at all
    # like the font.
    cursor = 0
    for g in glyphs:
        ats.append(cursor + 1)
        counts.append(len(g.runs))
        advs.append(g.adv)
        for row, a, b in g.runs:
            flat.extend((row, a, b))
        cursor += 3 * len(g.runs)

    # Read every glyph back out of the numbers alone, with the indexing the
    # engine uses, and refuse to write a table that does not come back. An
    # `font_at` that counts runs instead of numbers is a table that still
    # compiles, still draws, and draws something that is not the font, so the
    # only place it can be caught is here.
    for i, g in enumerate(glyphs):
        got = [
            tuple(flat[ats[i] - 1 + 3 * k: ats[i] + 2 + 3 * k])
            for k in range(counts[i])
        ]
        assert got == g.runs, f"{g.char!r} does not come back out of font_run"

    total = sum(counts)
    ligs = sum(1 for g in glyphs if g.lig)
    lig_note = ("//\n"
                "// The ligatures are keyed by the whole sequence they stand for -- `->`,\n"
                "// `==` -- and their runs are moved to where that sequence starts, so the\n"
                "// reader finds one with the same search and draws it at the same origin.\n"
                ) if ligs else ""
    body = f"""// {name}'s outlines, as the pen spans the engine draws them.
// Generated by font2vm.py -- do not edit it by hand.
//
// {len(glyphs)} glyphs, {total} runs. One unit is one scan row, {REF_ROWS} to the
// em, so drawing at size S scales every span by S/FONT_ROWS. `font_chars` is
// keyed the way the text reads: a lowercase character is itself, a capital is
// `\\c` and its lowercase letter. The list is sorted the way Scratch's `<`
// compares two strings -- which lowercases them -- and that is what makes the
// engine's binary search over it correct.
//
// A capital's key is two characters, so the backslash that starts it is the
// character the text reader treats as an escape; a literal backslash is `\\\\`.
{lig_note}
/// Scan rows to the em: what a text size is divided by to get the scale.
pub const FONT_ROWS: num = {REF_ROWS};

/// The capital's ink height in the same units: a caller that sizes text by its
/// capitals -- "this word is eight units tall" -- divides the size by this and
/// multiplies by FONT_ROWS to get the em those tables are fractions of.
pub const FONT_CAP: num = {cap};

/// The key of every glyph, in comparison order.
pub var font_chars: list<str> = [
{wrap([rav_string(c) for c in chars])}
];

/// Where a glyph's runs start in `font_run`, and how many there are.
pub var font_at: list<num> = [
{wrap([str(v) for v in ats])}
];
pub var font_runs: list<num> = [
{wrap([str(v) for v in counts])}
];

/// How far the pen moves after a glyph, in the same units.
pub var font_adv: list<num> = [
{wrap([str(v) for v in advs])}
];

/// Three numbers a run: its scan row, the first column of its ink, and the
/// column past the last one. The row's line is at row + 0.5.
pub var font_run: list<num> = [
{wrap(", ".join(str(v) for v in flat).split(", "))}
];
"""
    path.parent.mkdir(parents=True, exist_ok=True)
    # `newline="\n"` and not the platform's: a generated file that changes line
    # endings with the machine that generated it is a file that diffs as a whole
    # rewrite for no reason.
    path.write_text(body, encoding="utf-8", newline="\n")
    print(f"wrote {path} ({path.stat().st_size / 1e6:.1f} MB)")


# ------------------------------------------------------------- self-check --


def self_check(glyphs: list[Glyph]) -> None:
    """Fail loudly if the tables the engine reads could not be read."""
    for g in glyphs:
        assert g.adv >= 0, f"{g.char!r} has a negative advance"
        row_seen, col = None, 0
        for row, a, b in g.runs:
            assert b > a, f"{g.char!r} row {row} is empty"
            assert row_seen is None or row >= row_seen, f"{g.char!r} rows go back"
            if row != row_seen:
                col = a
            assert a >= col, f"{g.char!r} row {row} is out of column order"
            row_seen, col = row, b

    by_key = {g.key.lower(): g for g in glyphs}
    keys = [g.key for g in glyphs]
    assert keys == sorted(keys, key=sort_key), \
        "the keys are not in Scratch's comparison order"
    assert len(set(keys)) == len(keys), "two glyphs share a key"

    # The hole in `o` is the point of the winding rule: a row through the middle
    # of it has to be two runs, not one.
    o = by_key.get("o")
    assert o is not None, "no `o` in the table"
    mid = o.runs[len(o.runs) // 2][0]
    assert len([r for r in o.runs if r[0] == mid]) == 2, "`o` has no hole"


def shape_check(glyphs, sources, sample: str) -> int:
    """The tables against a rasteriser that has nothing to do with them.

    The scanline converter is the part of the tool that could be quietly wrong
    -- a winding rule, a half-pixel, a scale -- and every structural check would
    still pass. So a sample of glyphs is rasterised twice: once from the runs,
    as the pen would fill them, and once by FreeType through PIL, which knows
    nothing about this file. The overlap says whether they are the same shape.
    """
    from PIL import Image, ImageDraw

    pil = [pil_font(sources, i, REF_ROWS) for i in range(len(sources))]
    by_key = {g.char: g for g in glyphs}
    worst, total, scored = (1.0, ""), 0.0, 0

    for ch in sample:
        g = by_key.get(ch)
        if g is None or not g.runs:
            continue
        x0 = min(r[1] for r in g.runs) - 2
        x1 = max(r[2] for r in g.runs) + 2
        y0 = min(r[0] for r in g.runs) - 2
        y1 = max(r[0] for r in g.runs) + 3
        w, h = x1 - x0, y1 - y0
        ours = Image.new("L", (w, h), 0)
        draw = ImageDraw.Draw(ours)
        for row, a, b in g.runs:
            draw.rectangle([a - x0, y1 - row - 1, b - x0 - 1, y1 - row - 1], fill=255)

        theirs = Image.new("L", (w, h), 0)
        # `ls` is the left of the baseline, which is the frame the runs are in.
        ImageDraw.Draw(theirs).text((-x0, y1), ch, font=pil[g.src], fill=255, anchor="ls")
        theirs = theirs.point(lambda v: 255 if v >= 128 else 0)

        mine, font_side = ours.tobytes(), theirs.tobytes()
        both = sum(1 for p, q in zip(mine, font_side) if p and q)
        either = sum(1 for p, q in zip(mine, font_side) if p or q)
        iou = both / either if either else 1.0
        total += iou
        scored += 1
        if iou < worst[0]:
            worst = (iou, ch)

    print(f"  shapes: mean overlap {total / max(scored, 1):.3f} over {scored} glyphs, "
          f"worst {worst[0]:.3f} {worst[1]!r}")
    # A thin bar -- a hyphen, an underscore, a CJK horizontal -- is three or four
    # rows tall, and FreeType's hinting puts it on a row the pixel-centre rule
    # here does not pick, which costs a quarter of a shape that small. Anything
    # actually wrong with the converter drops far below this.
    return 0 if worst[0] >= 0.65 else 1


# --------------------------------------------------------------- the page --

STAGE_W, STAGE_H = 480, 360  # Scratch's stage, which is also the pen layer


def rav_decode(literal: str) -> str:
    """A raven string literal's value: the escapes the lexer reads."""
    out, i = "", 0
    while i < len(literal):
        if literal[i] == "\\":
            n = literal[i + 1]
            out += {"n": "\n", "r": "\r", "t": "\t", '"': '"', "\\": "\\", "0": "\0"}.get(n, n)
            i += 2
        else:
            out += literal[i]
            i += 1
    return out


def engine_lig_max(engine_path: Path) -> int:
    """How far the engine looks for a ligature, read from the engine itself.

    The generator reads this rather than holding a number of its own, so the
    longest key it writes is the longest key the reader will look for.
    """
    try:
        text = engine_path.read_text(encoding="utf-8")
    except OSError:
        return DEFAULT_LIG_MAX
    m = re.search(r"pub const PF_LIG_MAX: num = (\d+);", text)
    return int(m.group(1)) if m else DEFAULT_LIG_MAX


def read_layout(engine_path: Path, lay_path: Path) -> dict:
    """What the project says the page is, read from the project.

    The two files between them: the engine owns the leading and the box, because
    they are what it draws with, and the page owns where the block starts and how
    big it is, because that is what a project chooses.
    """
    engine = engine_path.read_text(encoding="utf-8")
    src = lay_path.read_text(encoding="utf-8")

    def num(text, pattern, what, where):
        m = re.search(pattern, text)
        if not m:
            raise SystemExit(f"{where} has no {what}")
        return float(m.group(1))

    return {
        "left": num(src, r"const LEFT: num = ([\d.eE+-]+);", "LEFT", lay_path.name),
        "top": num(src, r"const TOP: num = ([\d.eE+-]+);", "TOP", lay_path.name),
        "cap": num(src, r"const CAP: num = ([\d.eE+-]+);", "CAP", lay_path.name),
        "size": num(src, r"var size: num = ([\d.eE+-]+);", "size", lay_path.name),
        "limit": num(src, r"var limit: num = ([\d.eE+-]+);", "limit", lay_path.name),
        "ink": re.search(r'var ink: str = "([^"]*)";', src).group(1),
        "text": rav_decode(re.search(r'var text: str = "(.*)";', src).group(1)),
        "lead": num(engine, r"const PF_LEAD: num = ([\d.eE+-]+);", "PF_LEAD", engine_path.name),
        "edge_x": num(engine, r"const PF_EDGE_X: num = ([\d.eE+-]+);", "PF_EDGE_X", engine_path.name),
        "edge_y": num(engine, r"const PF_EDGE_Y: num = ([\d.eE+-]+);", "PF_EDGE_Y", engine_path.name),
        "lig_max": engine_lig_max(engine_path),
    }


def layout_page(glyphs, text: str, lay: dict):
    """Where every glyph of the page goes, by the rules `draw_text` uses."""
    by_key: dict[str, Glyph] = {}
    for g in glyphs:
        by_key.setdefault(g.key.lower(), g)

    scale = lay["size"] / REF_ROWS
    pen = max(1, math.ceil(scale))
    lead = lay["size"] * lay["lead"]
    left, limit = lay["left"], lay["limit"]
    px, py = left, lay["top"] - lay["size"] * lay["cap"]

    page, cap, i = [], False, 0
    while i < len(text):
        c = text[i]
        step, show, newline = 1, True, False
        if c in "\n\r":
            newline, show = True, False
        elif c == "\\":
            d = text[i + 1] if i + 1 < len(text) else ""
            if d in "cC":
                cap, show, step = True, False, 2
            elif d in "nN":
                newline, show, step = True, False, 2
            elif d == "\\":
                step = 2
        if newline:
            px, py = left, py - lead
        elif show:
            key = ("\\c" + c) if cap else c
            g = None
            # The longest ligature that starts here wins, which is why the
            # candidates grow a character at a time and the last hit is kept:
            # `===` is the three-character key, not `==` and then `=`.
            if not cap and c != "\\" and lay.get("lig_max", 1) > 1:
                run = ""
                k = 0
                while k < lay["lig_max"] and i + k < len(text):
                    d = text[i + k]
                    if d in "\\\n\r":
                        break
                    run += d
                    k += 1
                    if k > 1:
                        hit = by_key.get(run.lower())
                        if hit is not None and hit.lig:
                            g, step = hit, k
            cap = False
            if g is None:
                g = by_key.get(key.lower())
            adv = g.adv * scale if g is not None else lay["size"] / 2
            if limit > 0 and px > left and px + adv > left + limit:
                px, py = left, py - lead
            if g is not None:
                page.append((g, px, py))
            px += adv
        i += step
    return page, scale, pen


def blank_canvas() -> bytearray:
    return bytearray(STAGE_W * STAGE_H)


def stamp_run(mask: bytearray, x0: float, x1: float, y: float, pen: float) -> None:
    """One pen stroke onto the stage: a capsule of diameter `pen`, round caps.

    This is the whole pen. Scratch's renderer draws a line of `penAttributes`
    diameter between two points and the ends are round, so the ink of a stroke
    is every pixel whose centre is within `pen / 2` of the segment -- the same
    rule for a run of a glyph as for a line a reader drew by hand.
    """
    r = pen / 2
    ix0 = max(0, math.floor(x0 + STAGE_W / 2 - r - 1))
    ix1 = min(STAGE_W - 1, math.ceil(x1 + STAGE_W / 2 + r + 1))
    iy0 = max(0, math.floor(STAGE_H / 2 - y - r - 1))
    iy1 = min(STAGE_H - 1, math.ceil(STAGE_H / 2 - y + r + 1))
    for iy in range(iy0, iy1 + 1):
        dy = (STAGE_H / 2 - (iy + 0.5)) - y
        row = iy * STAGE_W
        for ix in range(ix0, ix1 + 1):
            dx = (ix + 0.5 - STAGE_W / 2) - min(max(ix + 0.5 - STAGE_W / 2, x0), x1)
            if dx * dx + dy * dy <= r * r:
                mask[row + ix] = 255


def render_tables(page, scale: float, pen: float) -> bytearray:
    """The page as the project draws it: the runs of `font.rav`, as pen lines."""
    mask = blank_canvas()
    for g, px, py in page:
        for row, a, b in g.runs:
            y = py + (row + 0.5) * scale
            x0 = px + a * scale + pen / 2
            x1 = px + b * scale - pen / 2
            if x1 <= x0:
                x0 = x1 = px + (a + b) * scale / 2
                x1 = x0 + 0.05
            stamp_run(mask, x0, x1, y, pen)
    return mask


def pil_font(sources, src: int, size: float):
    """A source as a PIL font at a size, on the axes the tables were built from."""
    from PIL import ImageFont

    path, index, axes = sources[src]
    font = ImageFont.truetype(path, int(round(size)), index=index)
    if axes:
        font.set_variation_by_axes(axes)
    return font


def render_freetype(page, sources, size: float) -> bytearray:
    """The page as the font itself draws it, which shares no code with above."""
    from PIL import Image, ImageDraw

    cache: dict[tuple[int, int], object] = {}

    def face(src: int):
        key = (src, int(round(size)))
        if key not in cache:
            cache[key] = pil_font(sources, src, size)
        return cache[key]

    img = Image.new("L", (STAGE_W, STAGE_H), 0)
    draw = ImageDraw.Draw(img)
    for g, px, py in page:
        # `ls` is the left of the baseline: the pen's own origin.
        draw.text((px + STAGE_W / 2, STAGE_H / 2 - py), g.char, font=face(g.src),
                  fill=255, anchor="ls")
    return bytearray(img.point(lambda v: 255 if v >= 128 else 0).tobytes())


def covered(a: bytearray, b: bytearray, within: int = 1) -> float:
    """How much of `a`'s ink has `b`'s ink within `within` pixels of it.

    A plain overlap is the wrong measure for a pen against FreeType: the pen is
    a hard capsule and FreeType anti-aliases and hints, so a stroke the font puts
    on one row and the pen puts on the next counts as two mistakes out of three
    on a thin bar, and the number says nothing. What does say something is
    whether each is where the other is, to within the pixel a different
    rasteriser is allowed to disagree by.
    """
    total = hits = 0
    for i, v in enumerate(a):
        if not v:
            continue
        total += 1
        x, y = i % STAGE_W, i // STAGE_W
        for dy in range(-within, within + 1):
            yy = y + dy
            if not 0 <= yy < STAGE_H:
                continue
            for dx in range(-within, within + 1):
                xx = x + dx
                if 0 <= xx < STAGE_W and b[yy * STAGE_W + xx]:
                    hits += 1
                    break
            else:
                continue
            break
    return hits / total if total else 1.0


def write_png(mask: bytearray, path: Path) -> None:
    from PIL import Image

    Image.frombytes("L", (STAGE_W, STAGE_H), bytes(mask)).save(path)


def stage_check(glyphs, sources, text: str, lay: dict, prefix: Path) -> int:
    """The page, twice: the tables as pen lines, and the font through FreeType.

    This is the check that has to pass before raven is built at all. It cannot
    see the compiler or the VM, and it is not meant to: it answers whether the
    tables and the layout put a page of text where the font says it goes, at the
    size and the wrapping the project will use. If the two pictures disagree,
    nothing downstream can be right.
    """
    page, scale, pen = layout_page(glyphs, text, lay)
    # A ligature cannot go through FreeType: PIL has no shaper here, so drawing
    # the characters it stands for would draw them apart. The page's other ink
    # is the comparison and the ligatures are reported instead, which is honest
    # about what this check does and does not see.
    plain = [e for e in page if not e[0].lig]
    ours = render_tables(page, scale, pen)
    theirs = render_freetype(plain, sources, lay["size"])
    ours_plain = render_tables(plain, scale, pen)
    mine = covered(ours_plain, theirs)
    font_side = covered(theirs, ours_plain)

    # What the page asks of the pen, against the box Scratch will let it move
    # inside. This is the measurement that has to come first: a run past the edge
    # is not clipped by Scratch, it is *moved*, so a page that leaves the box is
    # not a page with its edge shaved off, it is a page drawn in the wrong place,
    # and no amount of getting the glyphs right survives it.
    xs, ys = [], []
    for g, px, py in page:
        for row, a, b in g.runs:
            xs += (px + a * scale + pen / 2, px + b * scale - pen / 2)
            ys.append(py + (row + 0.5) * scale)
    inside = all(-lay["edge_x"] - pen / 2 <= v <= lay["edge_x"] + pen / 2 for v in xs) \
        and all(-lay["edge_y"] - pen / 2 <= v <= lay["edge_y"] + pen / 2 for v in ys)

    prefix.parent.mkdir(parents=True, exist_ok=True)
    write_png(ours, prefix.with_name(prefix.name + "-tables.png"))
    write_png(theirs, prefix.with_name(prefix.name + "-freetype.png"))
    # The raw one is what a VM check compares its own stage against.
    prefix.with_name(prefix.name + ".gray").write_bytes(bytes(ours))

    ink = sum(1 for v in ours if v)
    print(f"  page: {len(page)} glyphs, {ink} inked pixels at size {lay['size']:g}")
    if len(plain) != len(page):
        print(f"  page: {len(page) - len(plain)} of them ligatures, which the "
              f"FreeType side below does not draw")
    print(f"  page: ink x {min(xs):.1f}..{max(xs):.1f}, y {min(ys):.1f}..{max(ys):.1f} "
          f"in a box of +/-{lay['edge_x']:g} by +/-{lay['edge_y']:g}")
    print(f"  page: tables against FreeType, {mine:.3f} of the pen's ink on the "
          f"font's, {font_side:.3f} of the font's on the pen's, within a pixel")
    if not inside:
        print("  page: THE PAGE LEAVES THE BOX -- Scratch would move the sprite "
              "back and smear the ink")
    return 0 if inside and min(mine, font_side) >= 0.97 else 1


def preview(glyphs, text: str, size: float, lay: dict, path: Path) -> None:
    """The whole page as the project draws it, at the project's own layout."""
    lay = dict(lay)
    lay["size"] = float(size)
    page, scale, pen = layout_page(glyphs, text, lay)
    write_png(render_tables(page, scale, pen), path)
    print(f"wrote {path} ({STAGE_W}x{STAGE_H})")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--project", metavar="DIR",
                    help="the raven project to install into: src/penfont and "
                         "src/sprites/text.rav under it, which is what a project "
                         "the tool has not seen before looks like")
    ap.add_argument("--out", metavar="DIR",
                    help="the module directory to install into (default src/penfont)")
    ap.add_argument("--lay", metavar="FILE",
                    help="a raven file holding the page's own layout, for --stats/--stage/--preview")
    ap.add_argument("--engine", default=str(ENGINE), metavar="FILE",
                    help="the engine module to copy in, and to read the box and the leading from")
    ap.add_argument("--set", default="maple", choices=sorted(SETS),
                    help="which font set to build the tables from")
    ap.add_argument("--fonts", metavar="PATH[:FACE],...",
                    help="the whole font chain, in place of the set's own")
    ap.add_argument("--font", help="a first font, in place of the set's own")
    ap.add_argument("--face", type=int, default=0)
    ap.add_argument("--instance", metavar="TAG=VAL,...",
                    help="a variable font pinned to a point on its axes, e.g. "
                         "`wght=700`; an axis the font does not have is left "
                         "alone, so a chain can take one specification")
    ap.add_argument("--axes", action="store_true",
                    help="print the axes and the named instances of the fonts, "
                         "and write nothing")
    ap.add_argument("--bold", metavar="PATH[:FACE]",
                    help="a second face, keyed `\\b` and the ordinary key, so a "
                         "caller builds the key and the engine needs no change")
    ap.add_argument("--charset", default="all",
                    help="comma-separated glyph sets or bundles; --list-charsets "
                         "prints them with their sizes (default all)")
    ap.add_argument("--list-charsets", action="store_true",
                    help="print every set and bundle, with the size this font gives it")
    ap.add_argument("--text", metavar="CHARS",
                    help="add these characters to the inventory")
    ap.add_argument("--chars-file", metavar="FILE",
                    help="add every character in this file to the inventory")
    ap.add_argument("--ligatures", action="store_true",
                    help="add the font's own ligatures: a key of several "
                         "characters that one glyph draws, for `draw_text` to "
                         "find the longest of")
    ap.add_argument("--features", metavar="TAG,...", default=",".join(LIG_FEATURES),
                    help="which OpenType features carry ligatures (default "
                         "%(default)s); a project can ask for a font's own "
                         "stylistic sets and character variants here too, and "
                         "the order is the order they are applied in")
    ap.add_argument("--name", help="what to call the font in the generated header")
    ap.add_argument("--stats", action="store_true",
                    help="check a sample of glyphs against FreeType, and write nothing")
    ap.add_argument("--stage", metavar="PREFIX",
                    help="render the project's own page and check it against FreeType")
    ap.add_argument("--preview", metavar="PNG",
                    help="draw the project's own page from the runs")
    ap.add_argument("--size", type=float, default=None, help="override the page's size")
    ap.add_argument("--limit", type=int, help="build only the first N glyphs, to try it out")
    args = ap.parse_args()

    charset = [c.strip() for c in args.charset.split(",") if c.strip()]
    features = tuple(f.strip() for f in args.features.split(",") if f.strip())

    location: dict[str, float] = {}
    if args.instance:
        for part in args.instance.split(","):
            if "=" not in part:
                raise SystemExit(f"--instance wants TAG=VALUE, not {part!r}")
            tag, value = part.split("=", 1)
            location[tag.strip()] = float(value)

    project = Path(args.project) if args.project else None
    out = Path(args.out) if args.out else (project / DEFAULT_OUT if project else DEFAULT_OUT)
    lay_path = Path(args.lay) if args.lay else (
        project / DEFAULT_LAY if project else DEFAULT_LAY)

    only = ""
    if args.text:
        only += args.text
    if args.chars_file:
        only += Path(args.chars_file).read_text(encoding="utf-8")

    chosen = SETS[args.set]
    if args.fonts:
        chain = []
        for part in args.fonts.split(","):
            # A trailing `:2` is a face index; a drive letter is not.
            m = re.fullmatch(r"(.*):(\d+)", part.strip())
            chain.append((m.group(1), int(m.group(2))) if m else (part.strip(), 0))
    else:
        chain = list(chosen["fonts"])
        if args.font:
            chain[0] = (args.font, args.face)

    fonts, sources = [], []
    for path, face in chain:
        font = load_font(path, face)
        # The axes are read before the instance is taken: an instanced font is
        # a static one and no longer says what it was pinned to, and the PIL
        # side of the checks needs to pin the file the same way.
        sources.append((path, face, variation_axes(font, location)))
        fonts.append(font)
    if args.axes:
        for font in fonts:
            describe_axes(font)
        return 0
    if location:
        fonts = [instantiate(font, location) for font in fonts]
    cmaps = [f.getBestCmap() for f in fonts]
    registry = GLYPH_SETS
    if args.list_charsets:
        rows, bundles = charset_catalogue(registry, cmaps)
        print(f"{args.set}: {len(fonts)} fonts")
        for name, what, n in rows:
            print(f"  {name:<12} {n:>6}  {what}")
        print("  " + "-" * 68)
        for name, what, n in bundles:
            print(f"  {name:<12} {n:>6}  {what}")
        return 0
    keys, census = inventory(cmaps, resolve_charset(charset, registry), only)
    if args.limit:
        keys = keys[: args.limit]

    glyphs = build(fonts, keys)
    # A second face goes in the same table under the `\b` stem, sorted in with
    # the rest, so one install is one weight or two and the engine never learns
    # which it was given.
    if args.bold:
        m = re.fullmatch(r"(.*):(\d+)", args.bold.strip())
        bold_path, bold_face = (m.group(1), int(m.group(2))) if m else (args.bold.strip(), 0)
        bold = load_font(bold_path, bold_face)
        sources += [(bold_path, bold_face, variation_axes(bold, location))]
        if location:
            bold = instantiate(bold, location)
        glyphs += build([bold], keys, BOLD, len(sources) - 1)
        glyphs.sort(key=lambda g: sort_key(g.key))

    # The font's own ligatures, keyed by the sequence, sorted in with the rest so
    # one search finds a character, a capital and a run of them the same way.
    lig_count = 0
    if args.ligatures:
        lig_max = engine_lig_max(Path(args.engine))
        ligs = build_ligatures(fonts[0], lig_max, "", 0, features)
        if args.bold:
            ligs += build_ligatures(bold, lig_max, BOLD, len(sources) - 1, features)
        glyphs += ligs
        glyphs.sort(key=lambda g: sort_key(g.key))
        lig_count = len(ligs)

    runs = sum(len(g.runs) for g in glyphs)
    blank = sum(1 for g in glyphs if not g.runs)
    print(f"{args.name or args.set}: {len(fonts) + (1 if args.bold else 0)} fonts, "
          f"{len(glyphs)} glyphs, {runs} runs")
    print("  " + ", ".join(f"{n} {c}" for n, c in census if c))
    if args.bold:
        print(f"  {len(keys)} of them again under {BOLD!r} for the bold face")
    if lig_count:
        longest = max(len(g.key) for g in glyphs if g.lig)
        print(f"  {lig_count} ligatures, the longest {longest} characters")
    print(f"  {blank} glyphs draw nothing (spaces and the like)")

    self_check(glyphs)
    engine_path = Path(args.engine)
    if args.preview:
        lay = read_layout(engine_path, lay_path)
        preview(glyphs, args.text if args.text else lay["text"],
                args.size if args.size is not None else lay["size"], lay, Path(args.preview))
    if args.stats:
        # A spread over the scripts, plus everything whose shape is easy to get
        # wrong: the holes, the curves, the CJK whose rows are many and thin.
        sample = (
            "AaBbGgOoQqRrSsWwXxZz0123456789"
            ".,;:!?()[]{}\"'+-=*/\\%@&#$~^_|<> "
            "的一二三四十口日回国凹凸日月水火山人入八力刀又子女子小"
            "國語漢字臺灣龍鳳"
            "ぁあいうえおアイウエオ"
            "가나다라마바사"
            "←↑→↓∞≠≤≥★☆①②③"
        )
        return shape_check(glyphs, sources, sample)
    if args.stage:
        lay = read_layout(engine_path, lay_path)
        if args.size is not None:
            lay["size"] = args.size
        return stage_check(glyphs, sources, lay["text"], lay, Path(args.stage))

    emit(glyphs, out / "font.rav", args.name or args.set, cap_height(fonts[0]))
    # The engine goes in beside the table, so one command installs both and the
    # same command updates them.
    (out / "engine.rav").parent.mkdir(parents=True, exist_ok=True)
    (out / "engine.rav").write_text(engine_path.read_text(encoding="utf-8"),
                                    encoding="utf-8", newline="\n")
    print(f"wrote {out / 'engine.rav'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
