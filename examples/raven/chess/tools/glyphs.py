"""Turn Montserrat into a set of Scratch costumes, one SVG per character.

Run from the repository root:

    python examples/raven/chess/tools/glyphs.py

Why: the chess project draws its board and its panel with the pen, and it has no
text primitive -- Scratch has none.  A glyph is therefore a *costume* that the
pen stamps, and the stamps only line up if every costume is the same box with the
outline sitting in it exactly where the font puts it.  A font is turned into
outlines with fontTools, not into text: nothing here depends on an installed
font, on SVG <text>, or on whatever fonts a browser happens to have.

Naming scheme (fixed; the character-to-stem map is also written to the JSON):

    Every stem is "g" + an optional "b" for the bold weight + one letter for the
    character class + the character's code name:

        class   plain        bold         code name
        A..Z    guA          gbuA         the letter itself
        a..z    gla          gbla         the letter itself
        0..9    gn0          gbn0         the digit itself
        punct   gsdot        gbsdot       the ASCII name table below

    " " gsspace    . gsdot        , gscomma      : gscolon     ; gssemicolon
    !   gsbang     ? gsquestion   ' gsquote      " gsdblquote  - gshyphen
    +   gsplus     = gsequal      / gsslash      \\ gsbackslash ( gslparen
    )   gsrparen   [ gslbracket   ] gsrbracket   { gslbrace    } gsrbrace
    <   gsless     > gsgreater    % gspercent    * gsasterisk  & gsamp
    @   gsat       # gshash       _ gsunderscore | gspipe      ~ gstilde
    ^   gscaret    $ gsdollar

    The class letter is why A and a are not "gA" and "ga": a filesystem that
    folds case (NTFS, the one this runs on) makes those one file, and the plain
    and bold sets would silently overwrite each other as well.  The scheme must
    give every character a name that is distinct with the case ignored, and the
    self-check proves it is.

Geometry, and why:

    The box is one box for every glyph -- if it were fitted per glyph, the pen's
    stamps would not line up.  Montserrat's unitsPerEm is 1000 and its hhea ascent
    and descent are 968 and -251, so the box is that 1219-unit em box plus a
    margin on every side -- the tool prints the union of the ink of all 94
    characters to show it fits.  The pitch is the widest glyph's advance width,
    which is what "the right gap" can mean for a fixed pitch: any smaller and the
    widest glyph's ink would run into the next stamp, and the mean advance (about
    half the widest) would overlap badly.  The margin is 100 font units (0.1 em)
    rather than the 0.04 em one would guess for a margin: Montserrat's round
    brackets and its reverse solidus have negative left side bearings that reach
    past 0.09 em left of the origin, and the box has to hold those too.  The tool
    checks every glyph's ink against the box and refuses to write one that leaves
    it.

    Costume units are font units scaled by SCALE = 0.25, so the numbers are small
    enough to be usable as sprite sizes.  W and H are the ceiling of the scaled
    box: a box a fraction of a unit wider than the text does no harm, and the
    baseline stays on a whole costume unit regardless.

    The y axis is flipped (SVG's grows down, the font's grows up) by the affine
    transform (x, y) -> (SCALE*x + MX, -SCALE*y + BASELINE), so capitals sit
    above the baseline and descenders below it.

Output, all inside examples/raven/chess/assets/:

    g<code>.svg     the plain set, fill #e8e6e3
    gb<code>.svg    the bold set, fill #ffffff
    glyphs.json     {"box": {"w", "h"}, "advance", "baseline", "advances",
                     "cap": {...}, "bold": {...}}
                    where a set maps each character to its costume file stem,
                    "advances" is the hmtx advance of each character in CHARS
                    order in font units, and "baseline" is where the font's
                    baseline sits in the box.

It also writes examples/raven/chess/src/sprites/glyphs.rav, the module the HUD
stamps the font from: the box, the baseline, the advance list, and the two
matches a costume name needs.  The same run splices the costume list into
hud.rav and board.rav, between `// BEGIN GLYPH COSTUMES` and
`// END GLYPH COSTUMES`.

A character with no outline at all (the space) gets a fully transparent box of
the same size instead of a path.  The render at the end needs playwright and is
skipped, with a printed warning, when it is not importable.
"""

from __future__ import annotations

import base64
import json
import math
import sys
from pathlib import Path

from fontTools.misc.transform import Transform
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

HERE = Path(__file__).resolve()
ROOT = HERE.parents[4]
ASSETS = ROOT / "examples" / "raven" / "chess" / "assets"
FONT_DIR = (
    ROOT
    / "ref"
    / "scratch-editor"
    / "node_modules"
    / "@scratch"
    / "scratch-vm"
    / "node_modules"
    / "docdash"
    / "static"
    / "fonts"
    / "Montserrat"
)
PNG = ROOT / "ref" / "_ptmp" / "glyphs.png"
SPRITE_DIR = ROOT / "examples" / "raven" / "chess" / "src" / "sprites"
RAV = SPRITE_DIR / "glyphs.rav"

SCALE = 100.0 / 1419.0  # the box is 100 units tall  # costume units per font unit
MARGIN = 100  # font units of margin on every side; 0.1 em, see the note below
CAP_COLOR = "#e8e6e3"
BOLD_COLOR = "#ffffff"

# Stage units of a run's em per unit of the nominal text size a `text(...)` call
# passes.  The HUD's sizes were tuned against the old hand-drawn box, whose cap
# height was ten of its twelve units, and the mean advance here is 0.582 em, so
# 0.17 keeps both the pitch (0.099 against 0.095) and the cap height (0.119
# against 0.100) near what they were.
# The capital's ink height in a costume, measured on the glyphs this tool
# writes; the self-check fails if a font change moves it.
CAP_INK = 175.0 * SCALE / 0.25

GSIZE = 0.17

# The blocks both sprites carry between: a costume name is a literal in raven, so
# the 188 declarations have to be written into each file that stamps a glyph.
BEGIN = "    // BEGIN GLYPH COSTUMES"
END = "    // END GLYPH COSTUMES"

PUNCT = [
    " ",
    ".",
    ",",
    ":",
    ";",
    "!",
    "?",
    "'",
    '"',
    "-",
    "+",
    "=",
    "/",
    "\\",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "<",
    ">",
    "%",
    "*",
    "&",
    "@",
    "#",
    "_",
    "|",
    "~",
    "^",
    "$",
]
NAMES = {
    " ": "space",
    ".": "dot",
    ",": "comma",
    ":": "colon",
    ";": "semicolon",
    "!": "bang",
    "?": "question",
    "'": "quote",
    '"': "dblquote",
    "-": "hyphen",
    "+": "plus",
    "=": "equal",
    "/": "slash",
    "\\": "backslash",
    "(": "lparen",
    ")": "rparen",
    "[": "lbracket",
    "]": "rbracket",
    "{": "lbrace",
    "}": "rbrace",
    "<": "less",
    ">": "greater",
    "%": "percent",
    "*": "asterisk",
    "&": "amp",
    "@": "at",
    "#": "hash",
    "_": "underscore",
    "|": "pipe",
    "~": "tilde",
    "^": "caret",
    "$": "dollar",
}

CHARS = (
    [chr(c) for c in range(ord("A"), ord("Z") + 1)]
    + [chr(c) for c in range(ord("a"), ord("z") + 1)]
    + [chr(c) for c in range(ord("0"), ord("9") + 1)]
    + PUNCT
)


def stem(ch: str, bold: bool) -> str:
    """The costume file stem of one character in one weight."""
    code = code_of(ch)
    if "A" <= ch <= "Z":
        klass = "u"
    elif "a" <= ch <= "z":
        klass = "l"
    elif "0" <= ch <= "9":
        klass = "n"
    else:
        klass = "s"
    return "g" + ("b" if bold else "") + klass + code


def code_of(ch: str) -> str:
    if ("A" <= ch <= "Z") or ("a" <= ch <= "z") or ("0" <= ch <= "9"):
        return ch
    if ch not in NAMES:
        raise SystemExit(f"no code name for {ch!r}: extend NAMES")
    return NAMES[ch]


def num(v: float) -> str:
    """A path coordinate: two decimals at most, no trailing zeros, no -0."""
    v = round(float(v), 2)
    if v == 0:
        v = 0.0
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return s or "0"


def jsnum(v: float) -> int | float:
    """A number as JSON or as raven source writes it: an integer when it is one."""
    return int(v) if float(v).is_integer() else round(float(v), 4)


def rav_literal(ch: str) -> str:
    """A character as a raven string literal, escapes and all."""
    return json.dumps(ch, ensure_ascii=False)


class Weight:
    """One TTF, opened for drawing."""

    def __init__(self, path: Path, bold: bool):
        self.path = path
        self.bold = bold
        self.font = TTFont(path)
        self.upm = self.font["head"].unitsPerEm
        self.ascent = self.font["hhea"].ascent
        self.descent = self.font["hhea"].descent
        self.glyph_set = self.font.getGlyphSet()
        self.cmap = self.font.getBestCmap()
        self.hmtx = self.font["hmtx"]

    def glyph(self, ch: str):
        name = self.cmap.get(ord(ch))
        if name is None:
            raise SystemExit(f"{self.path.name}: no glyph for {ch!r}")
        return name

    def advance(self, ch: str) -> int:
        return self.hmtx[self.glyph(ch)][0]

    def ink(self, ch: str):
        """The glyph's ink bounds in font units, or None when it draws nothing."""
        pen = BoundsPen(self.glyph_set)
        self.glyph_set[self.glyph(ch)].draw(pen)
        return pen.bounds


def write_rav(doc: dict) -> None:
    """The module the HUD stamps the font from, generated from the JSON."""
    w, h = doc["box"]["w"], doc["box"]["h"]
    adv = doc["advances"]
    space = CHARS.index(" ") + 1
    lines = [
        "// The HUD font, generated by tools/glyphs.py from Montserrat: one costume",
        "// per character per weight. Do not edit it by hand.",
        "//",
        "// A costume's name is a literal in raven, so a character cannot name its own",
        "// costume; `gindex` and `gdraw` are one arm each instead, and they are also",
        "// the only place the alphabet's order is written down.",
        "",
        "/// The width and height of the one box every glyph's costume is, and where",
        "/// the font's baseline sits in it, in costume units.",
        f"pub const GBOX: num = {w};",
        f"pub const GBOXH: num = {h};",
        f"pub const GBASE: num = {jsnum(doc['baseline'])};",
        "",
        "/// The costume units one font unit is. The box is the font's em box scaled",
        "/// by this, so it is what turns a unit of advance into a length.",
        f"pub const GSCALE: num = {SCALE};",
        "",
        "/// The glyph origin's x in the box: a stamp is centred on the box, not on",
        "/// where the pen starts, so a run is placed this far off GBOX/2.",
        f"pub const GORIGIN: num = {jsnum(MARGIN * SCALE)};",
        "",
        "/// One unit of a nominal text size is this much of the box height: a run's",
        "/// em is GSIZE*size, so the sizes the layout was tuned with still mean the",
        "/// pitch and the cap height they meant with the old glyph box.",
        f"pub const GCAP: num = {CAP_INK};",
        f"pub const GSIZE: num = {GSIZE};",
        "",
        "/// Every character's advance, in font units, in the order `gindex` numbers",
        "/// them: a character's width is its own, not a fixed pitch.",
        "pub var gadv: list<num> = [",
    ]
    for i in range(0, len(adv), 16):
        part = adv[i : i + 16]
        tail = "," if i + 16 < len(adv) else ""
        lines.append("    " + ", ".join(str(a) for a in part) + tail)
    lines += [
        "];",
        "",
        "/// The 1-based index of a character into `gadv` and `gdraw`. A character",
        "/// the font does not have is the space, which draws nothing.",
        "///",
        "/// Scratch compares two strings case-insensitively and `item # of list` is",
        "/// the only lookup there is, so a lowercase letter is found at its capital:",
        "/// every letter lands on 1..26 and the renderer reaches the lowercase",
        "/// glyphs by adding 26 to the index.",
        "pub proc gindex(c: str) -> num warp {",
        "    match c {",
    ]
    # One arm per character that case-folds to itself. Scratch compares strings
    # without case, so an arm for 'a' answers for 'A' too and the match would
    # return whichever the compiler happened to place first -- wrong advance and
    # wrong glyph. The lowercase is reached by adding 26 to the capital's index.
    for i, ch in enumerate(CHARS):
        if ch.islower() and ch.upper() in CHARS:
            continue
        lines.append(f"        {rav_literal(ch)} => {{ return {i + 1}; }},")
    lines += [f"        _ => {{ return {space}; }}", "    }", "}"]
    per = 8  # characters per dispatch procedure
    groups = (len(CHARS) + per - 1) // per
    lines += [
        "",
        "/// The costume a character's index and weight draw it with, over `i * 2 +",
        "/// bold` so the plain and the bold set alternate.",
        "///",
        "/// One small procedure per eight characters rather than one long match: a",
        "/// single match with every pair in it is deeper than the compiler lowers.",
        "pub proc gdraw(i: num, bold: num) warp {",
        "    let k = i * 2 + bold;",
        # The groups are (i - 1) // per, because the indices start at one:
        # `floor(i / per)` sends the first character of every group -- H is
        # the eighth -- into the group before it, which has no arm for it.
        f"    match floor((i - 1) / {per}) {{",
    ]
    for g in range(groups):
        lines.append(f"        {g} => {{ gd{g + 1}(k); }},")
    lines += [
        f'        _ => {{ looks::switch_costume_to("{stem(" ", False)}"); }}',
        "    }",
        "}",
    ]
    for g in range(groups):
        lo, hi = g * per + 1, min((g + 1) * per, len(CHARS))
        lines += [
            "",
            f"/// The costumes of characters {lo}..{hi}, over the same `k`.",
            f"pub proc gd{g + 1}(k: num) warp {{",
            "    match k {",
        ]
        for i in range(g * per, min((g + 1) * per, len(CHARS))):
            ch = CHARS[i]
            n = i + 1
            lines.append(f'        {n * 2} => {{ looks::switch_costume_to("{stem(ch, False)}"); }},')
            lines.append(f'        {n * 2 + 1} => {{ looks::switch_costume_to("{stem(ch, True)}"); }},')
        lines += [
            f'        _ => {{ looks::switch_costume_to("{stem(" ", False)}"); }}',
            "    }",
            "}",
        ]
    RAV.write_text("\n".join(lines) + "\n", encoding="utf-8")


def costume_block() -> list[str]:
    """The declarations both sprites carry, one per character per weight."""
    return [
        BEGIN,
        "    // Everything between these two markers is generated by tools/glyphs.py:",
        "    // one costume per character per weight. Do not edit it by hand.",
        *(
            f'    costume "{stem(ch, bold)}" = "assets/{stem(ch, bold)}.svg";'
            for bold in (False, True)
            for ch in CHARS
        ),
    ]


def splice_costumes(block: list[str]) -> None:
    """Replace each sprite's glyph costume list, or plant it after the pieces."""
    body = "\n".join(block) + "\n"
    for name in ("board.rav", "hud.rav"):
        path = SPRITE_DIR / name
        text = path.read_text(encoding="utf-8")
        if BEGIN in text and END in text:
            i, j = text.index(BEGIN), text.index(END)
            new = text[:i] + body + text[j:]
        else:
            # The first run has no markers yet: the glyphs go after the pieces,
            # which are their own marker pair and are spliced by tools/assets.mjs.
            k = text.index("// END PIECE COSTUMES")
            k = text.index("\n", k) + 1
            new = text[:k] + body + END + "\n" + text[k:]
        path.write_text(new, encoding="utf-8")


def main() -> int:
    weights = {
        "cap": Weight(FONT_DIR / "Montserrat-Regular.ttf", False),
        "bold": Weight(FONT_DIR / "Montserrat-Bold.ttf", True),
    }
    for w in weights.values():
        if w.upm != 1000:
            raise SystemExit(f"{w.path.name}: unitsPerEm {w.upm}, expected 1000")

    # --- the box -----------------------------------------------------------------
    ascent = max(w.ascent for w in weights.values())
    descent = min(w.descent for w in weights.values())
    all_advances = [w.advance(c) for w in weights.values() for c in CHARS]
    # The plain weight lays the run out: text is only one weight at a time, and
    # bold advances differ from it by a hair, not by enough to matter for a pitch.
    cap_advances = [weights["cap"].advance(c) for c in CHARS]
    pitch_units = max(all_advances)  # the pitch: the widest glyph's advance
    box_w = math.ceil((pitch_units + 2 * MARGIN) * SCALE)
    box_h = math.ceil((ascent + MARGIN - descent + MARGIN) * SCALE)
    baseline = (ascent + MARGIN) * SCALE
    margin_x = MARGIN * SCALE
    advance = round(pitch_units * SCALE, 4)

    ink_tops = [b[3] for w in weights.values() for c in CHARS if (b := w.ink(c))]
    ink_bots = [b[1] for w in weights.values() for c in CHARS if (b := w.ink(c))]
    ink_rights = [b[2] for w in weights.values() for c in CHARS if (b := w.ink(c))]
    ink_lefts = [b[0] for w in weights.values() for c in CHARS if (b := w.ink(c))]
    leftmost = sorted(
        ((b[0], w.path.name, c) for w in weights.values() for c in CHARS if (b := w.ink(c))),
        key=lambda t: t[0],
    )

    print("font    : Montserrat-Regular.ttf + Montserrat-Bold.ttf (docdash static fonts)")
    print(f"units   : unitsPerEm {weights['cap'].upm}, ascent {ascent}, descent {descent}")
    print(
        f"ink     : y {min(ink_bots)}..{max(ink_tops)}, x {min(ink_lefts)}..{max(ink_rights)}"
    )
    print(
        f"box     : W={box_w} H={box_h} at SCALE={SCALE} costume units per font unit, "
        f"margin {MARGIN} units ({MARGIN / weights['cap'].upm:.3f} em) a side"
    )
    print(
        f"          the margin is the deepest negative left bearing plus slack: "
        + ", ".join(f"{c!r} {b} ({n.replace('Montserrat-', '').replace('.ttf', '')})"
                    for b, n, c in leftmost[:3])
    )
    print(
        f"          H = ceil(({ascent} + {MARGIN} - ({descent}) + {MARGIN}) * {SCALE}) "
        f"= {box_h}: the full ascent+descent plus a margin, one box for every glyph"
    )
    print(
        f"          W = ceil(({pitch_units} + {2 * MARGIN}) * {SCALE}) = {box_w}: one pitch "
        f"plus the same margin, the pitch being the widest advance ({max(all_advances)} units)"
    )
    print(
        f"baseline: y={baseline} in the box; glyph x origin at x={margin_x}, "
        f"pitch {advance} costume units"
    )
    print(
        f"advance : {advance} = max advance over both weights; mean advance "
        f"{sum(all_advances) / len(all_advances) * SCALE:.2f}, so narrow glyphs sit loose -- "
        f"a pitch on the mean would overlap the wide ones"
    )

    # the box has to hold the ink, or a glyph is clipped
    for w in weights.values():
        for ch in CHARS:
            b = w.ink(ch)
            if b is None:
                continue
            if (
                b[0] * SCALE + margin_x < 0
                or b[2] * SCALE + margin_x > box_w
                or -b[3] * SCALE + baseline < 0
                or -b[1] * SCALE + baseline > box_h
            ):
                raise SystemExit(f"{w.path.name} {ch!r}: ink {b} leaves the box")

    # --- the costumes ------------------------------------------------------------
    ASSETS.mkdir(parents=True, exist_ok=True)
    # This tool owns every g*.svg in the directory -- no other asset in it has that
    # prefix -- so a set from an earlier scheme is cleared rather than left behind
    # to answer for a character whose own file failed to be written.
    for stale in sorted(ASSETS.glob("g*.svg")):
        stale.unlink()
    ink_box: dict[str, dict[str, tuple[float, float, float, float] | None]] = {}
    for key, w in weights.items():
        xform = Transform(SCALE, 0, 0, -SCALE, margin_x, baseline)
        color = BOLD_COLOR if w.bold else CAP_COLOR
        ink_box[key] = {}
        written = 0
        for ch in CHARS:
            code = code_of(ch)
            glyph = w.glyph_set[w.glyph(ch)]

            svg_pen = SVGPathPen(w.glyph_set, ntos=num)
            glyph.draw(TransformPen(svg_pen, xform))
            d = svg_pen.getCommands()

            bounds_pen = BoundsPen(w.glyph_set)
            glyph.draw(TransformPen(bounds_pen, xform))
            ink_box[key][ch] = bounds_pen.bounds

            head = (
                f'<svg xmlns="http://www.w3.org/2000/svg" width="{box_w}" '
                f'height="{box_h}" viewBox="0 0 {box_w} {box_h}">'
            )
            if d:
                body = f'<path d="{d}" fill="{color}" fill-rule="nonzero"/>'
            else:
                body = f'<rect width="{box_w}" height="{box_h}" fill="{color}" fill-opacity="0"/>'
            (ASSETS / f"{stem(ch, w.bold)}.svg").write_text(
                head + body + "</svg>\n", encoding="utf-8"
            )
            written += 1
        print(f"wrote   : {written} costumes for the {key} set ({w.path.name})")

    # --- glyphs.json -------------------------------------------------------------
    doc = {
        "box": {"w": box_w, "h": box_h},
        "advance": advance,
        "baseline": jsnum(baseline),
        "advances": cap_advances,
        "cap": {ch: stem(ch, False) for ch in CHARS},
        "bold": {ch: stem(ch, True) for ch in CHARS},
    }
    (ASSETS / "glyphs.json").write_text(
        json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    # --- src/sprites/glyphs.rav --------------------------------------------------
    write_rav(doc)
    splice_costumes(costume_block())
    print(
        f"raven   : {len(CHARS)} characters, box {box_w}x{box_h}, baseline "
        f"{jsnum(baseline)}, {len(CHARS) * 2} costumes -> {RAV.relative_to(ROOT)} "
        f"and the costume block of board.rav and hud.rav"
    )

    # --- self-check --------------------------------------------------------------
    print("check   :")
    problems: list[str] = []
    stems = {
        key: {ch: stem(ch, key == "bold") for ch in CHARS}
        for key in ("cap", "bold")
    }

    # Two characters whose stems differ only in case would be one file on a
    # case-folding filesystem, and one of them would answer for the other.
    seen: dict[str, str] = {}
    for key in ("cap", "bold"):
        for ch in CHARS:
            folded = stems[key][ch].casefold()
            if folded in seen and seen[folded] != f"{key}:{ch}":
                problems.append(
                    f"stems {stems[key][ch]!r} ({key} {ch!r}) and {seen[folded]!r} "
                    f"differ only in case"
                )
            seen[folded] = f"{key}:{ch}"

    for key, w in weights.items():
        for ch in CHARS:
            f = ASSETS / f"{stems[key][ch]}.svg"
            if not f.is_file():
                problems.append(f"missing file {f.name} for {ch!r}")
                continue
            # the name on disk, spelled out again from the documented scheme
            klass = (
                "u"
                if "A" <= ch <= "Z"
                else "l"
                if "a" <= ch <= "z"
                else "n"
                if "0" <= ch <= "9"
                else "s"
            )
            want_name = ("gb" if key == "bold" else "g") + klass + code_of(ch) + ".svg"
            if f.name != want_name:
                problems.append(f"{f.name} does not follow the scheme: wanted {want_name}")
            text = f.read_text(encoding="utf-8")
            want = (
                f'width="{box_w}" height="{box_h}" viewBox="0 0 {box_w} {box_h}"'
            )
            if want not in text:
                problems.append(f"{f.name}: viewBox is not {want}")
            if text.count("<path ") > 1 or text.count("<rect ") > 1:
                problems.append(f"{f.name}: more than one shape")
            if "<path " in text:
                d = text.split('d="', 1)[1].split('"', 1)[0]
                if not d:
                    problems.append(f"{f.name}: empty path for {ch!r}")
                elif d.count("M") != d.count("Z"):
                    problems.append(f"{f.name}: {d.count('M')} contours, {d.count('Z')} closed")
                elif not d.endswith("Z"):
                    problems.append(f"{f.name}: last contour not closed")
            elif ch != " ":
                problems.append(f"{f.name}: no path and {ch!r} is not the space")

    cap = ink_box["cap"]
    h_A = cap["A"][3] - cap["A"][1]
    assert abs(h_A - CAP_INK) < 0.5, (
        f"capital ink height is {h_A}, not the {CAP_INK} GCAP declares"
    )
    h_a = cap["a"][3] - cap["a"][1]
    w_i = cap["i"][2] - cap["i"][0]
    w_m = cap["m"][2] - cap["m"][0]
    print(f"          ink height of A = {h_A:.2f}, of a = {h_a:.2f} (A taller: {h_A > h_a})")
    print(f"          ink width  of i = {w_i:.2f}, of m = {w_m:.2f} (i narrower: {w_i < w_m})")
    if not h_A > h_a:
        problems.append(f"capital A ink {h_A:.2f} is not taller than a ink {h_a:.2f}")
    if not w_i < w_m:
        problems.append(f"i ink {w_i:.2f} is not narrower than m ink {w_m:.2f}")
    # upright, and on the baseline: g descends well below it, b only overshoots it
    # the way a round letter does.  An ink box in SVG coordinates is
    # (xMin, yMin, xMax, yMax) with y growing downward, so the bottom of the ink is
    # index 3 and "below the baseline" means a y greater than the baseline's.
    g_drop = cap["g"][3] - baseline
    b_drop = cap["b"][3] - baseline
    print(
        f"          baseline y = {baseline:.2f}; g ink reaches {g_drop:.2f} below it "
        f"(descends: {g_drop > 10}), b ink {b_drop:.2f} (overshoot only: {b_drop < 10})"
    )
    if not g_drop > 10:
        problems.append("g does not descend below the baseline: the y flip is wrong")
    if not b_drop < 10:
        problems.append("b descends below the baseline: the y flip is wrong")

    if problems:
        print("FAILED  :")
        for p in problems:
            print(f"          {p}")
        return 1

    on_disk = sorted(p.name for p in ASSETS.glob("g*.svg"))
    if len(on_disk) != 2 * len(CHARS):
        print(
            f"FAILED  : {len(on_disk)} g*.svg on disk, expected {2 * len(CHARS)} -- "
            f"names are colliding on this filesystem"
        )
        return 1
    print(
        f"          {len(CHARS)} characters x 2 weights = {2 * len(CHARS)} distinct files "
        f"(all stems distinct, case folded too); every file's viewBox is {box_w}x{box_h}; "
        f"no empty path but the space"
    )

    render_png(weights["cap"], doc)
    return 0


def render_png(weight: Weight, doc: dict) -> None:
    """Stamp AZaz09.,:;!?-+/ at the declared pitch into one PNG, to be looked at."""
    row = "AZaz09.,:;!?-+/"
    box_w, box_h = doc["box"]["w"], doc["box"]["h"]
    advance = doc["advance"]
    imgs = []
    for i, ch in enumerate(row):
        svg = (ASSETS / f"{stem(ch, False)}.svg").read_text(encoding="utf-8")
        b64 = base64.b64encode(svg.encode("utf-8")).decode("ascii")
        imgs.append(
            f'<img style="position:absolute;left:{i * advance:.2f}px;top:0;'
            f'width:{box_w}px;height:{box_h}px" '
            f'src="data:image/svg+xml;base64,{b64}">'
        )
    pad = 24
    width = math.ceil(advance * (len(row) - 1) + box_w) + 2 * pad
    height = box_h + 2 * pad
    html = (
        '<!doctype html><meta charset="utf-8">'
        f'<body style="margin:0;background:#14161a">'
        f'<div style="position:relative;margin:{pad}px;width:{width}px;height:{box_h}px">'
        + "".join(imgs)
        + "</div></body>"
    )
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("png     : skipped, playwright is not importable")
        return
    PNG.parent.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        # The headless shell is not always installed next to the full chromium;
        # the full browser renders the same SVG.
        exe = p.chromium.executable_path
        browser = p.chromium.launch(
            executable_path=exe if exe and Path(exe).is_file() else None
        )
        page = browser.new_page(viewport={"width": width, "height": height})
        page.set_content(html)
        page.wait_for_timeout(200)
        page.screenshot(path=str(PNG))
        browser.close()
    print(f"png     : {PNG.relative_to(ROOT)} ({width}x{height}), row {row!r}")


if __name__ == "__main__":
    sys.exit(main())
