# Prints where each character of a run lands, straight out of the generated
# glyph module, so the spacing can be seen without a Scratch player.

import json
import re

ROOT = "examples/raven/chess"
g = open(f"{ROOT}/src/sprites/glyphs.rav", encoding="utf-8").read()
doc = json.load(open(f"{ROOT}/assets/glyphs.json", encoding="utf-8"))

const = {k: float(v) for k, v in re.findall(r"pub const (GBOX|GBOXH|GBASE|GSCALE|GCAP|GORIGIN): num = ([\d.]+);", g)}
adv = [float(x) for x in re.search(r"pub var gadv: list<num> = \[([^\]]*)\]", g).group(1).split(",") if x.strip()]
index = {}
for m in re.finditer(r'"((?:\\.|[^"\\])*)"\s*=>\s*\{\s*return\s+(\d+);', g):
    index[m.group(1)] = int(m.group(2))

print("box %.1f x %.1f  baseline %.2f  scale %.5f  cap %.2f  origin %.2f"
      % (const["GBOX"], const["GBOXH"], const["GBASE"], const["GSCALE"], const["GCAP"], const["GORIGIN"]))
print("index arms %d, advances %d, characters %d" % (len(index), len(adv), len(doc["cap"])))

folded = [c for c in doc["cap"] if c.islower() and c.upper() in doc["cap"]]
print("lowercase reached by +26, not by an arm:", "".join(folded))


def at(ch):
    """The index the module gives a character, folds and all."""
    if ch in index:
        return index[ch]
    for key, value in index.items():
        if key.lower() == ch.lower():
            return value
    return index.get(" ", 1)


for text, cap in [("CHESS", 29.6), ("PLAY", 7.1), ("SETTINGS", 7.1), ("WHITE", 7.1), ("YOU", 7.1), ("1900", 7.1)]:
    sz = cap * 100 / const["GCAP"]
    scale = sz / 100
    total = sum(adv[at(c) - 1] for c in text) * const["GSCALE"] * scale
    xs = []
    run = 0.0
    for c in text:
        a = adv[at(c) - 1] * const["GSCALE"] * scale
        xs.append(run + a / 2)
        run += a
    left = -total / 2
    print(f"{text:9} cap {cap:5.1f} sz {sz:5.1f}% width {total:6.1f}u  pitch {total / len(text):5.1f}u  "
          f"x {[round(left + x, 1) for x in xs]}")
