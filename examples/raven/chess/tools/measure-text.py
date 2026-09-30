# Prints where each character of a run lands, straight out of the penfont table
# the HUD draws from, so the spacing can be seen without a Scratch player.
#
#   python examples/raven/chess/tools/measure-text.py
#
# The sizes below are what the HUD asks for: a capital `size` stage units tall.
# `FONT_CAP` is the capital's ink height in the table's own units and
# `FONT_ROWS` is the rows to the em, so the em a size means is
# `size * FONT_ROWS / FONT_CAP`, and an advance is that many rows over.

import re

ROOT = "examples/raven/chess"
rav = open(f"{ROOT}/src/penfont/font.rav", encoding="utf-8").read()


def const(name):
    return float(re.search(rf"pub const {name}: num = ([\d.]+);", rav).group(1))


def list_of(name):
    body = re.search(rf"pub var {name}: list<\w+> = \[(.*?)\n\];", rav, re.S).group(1)
    return re.findall(r'"(?:\\.|[^"\\])*"|[-\d.]+', body)


def unhex(literal):
    return "".join(chr(int(h, 16)) for h in re.findall(r"\\u\{([0-9A-Fa-f]+)\}", literal))


ROWS, CAP = const("FONT_ROWS"), const("FONT_CAP")
chars = [unhex(x) for x in list_of("font_chars")]
adv = [float(x) for x in list_of("font_adv")]
index = {c.lower(): i + 1 for i, c in enumerate(chars)}

print(f"cap {CAP} rows {ROWS}  ->  a capital `size` tall is an em of size * {ROWS / CAP:.4f}")
bold_keys = sum(1 for c in chars if c.startswith(chr(92) + "b"))
print(f"keys {len(chars)}, advances {len(adv)}, of which bold {bold_keys}")


def at(ch, caps, bold):
    """The key the HUD builds for a character, and the glyph it finds."""
    key = ch
    if caps and ch.lower() in "abcdefghijklmnopqrstuvwxyz":
        key = "\\c" + ch
    if bold:
        key = "\\b" + key
    return index.get(key.lower(), index.get(" ", 1))


for text, size, caps, bold in [
    ("CHESS", 29.6, True, False),
    ("SETTINGS", 14.3, True, False),
    ("PLAY THE MAIA NETWORKS", 7.1, True, False),
    ("maia 1900", 10.3, False, True),
    ("Nf3", 9.2, True, False),
]:
    em = size * ROWS / CAP
    widths = [adv[at(c, caps, bold) - 1] * em / ROWS for c in text]
    total = sum(widths)
    xs, run = [], -total / 2
    for w in widths:
        xs.append(round(run, 1))
        run += w
    print(f"{text!r:24} cap {size:5.1f} em {em:5.1f} width {total:6.1f}u "
          f"pitch {total / len(text):5.2f}u  origins {xs}")
