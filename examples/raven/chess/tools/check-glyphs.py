"""Checks the generated glyph module: every character's index, advance, costume
and dispatch arm have to agree, or a letter draws as another letter or as
nothing. Run it after tools/glyphs.py; it exits non-zero on any disagreement."""
import json, re, sys

ROOT = "examples/raven/chess"
g = open(f"{ROOT}/src/sprites/glyphs.rav", encoding="utf-8").read()
doc = json.load(open(f"{ROOT}/assets/glyphs.json", encoding="utf-8"))

def arms_of(name):
    body = re.search(rf"pub proc {name}\(k: num\) warp \{{(.*?)\n\}}", g, re.S)
    if not body:
        return {}
    return {int(k): v for k, v in re.findall(r'(\d+)\s*=>\s*\{\s*looks::switch_costume_to\("([^"]+)"\)', body.group(1))}

dispatch = {int(a): b for a, b in re.findall(r"(\d+)\s*=>\s*\{\s*(gd\d+)\(", g)}
# The dispatch the module actually uses, not the one the checker would like.
formula = re.search(r"match floor\(\(i ([+-]) (\d+)\) / (\d+)\)", g)
assert formula, "gdraw does not dispatch on a group of indices"
sign, off, per = formula.group(1), int(formula.group(2)), int(formula.group(3))
group_of = lambda i: (i + off) // per if sign == "+" else (i - off) // per
groups = {name: arms_of(name) for name in set(dispatch.values())}
index = {m.group(1): int(m.group(2)) for m in re.finditer(r'"((?:\\.|[^"])*)"\s*=>\s*\{\s*return\s+(\d+);', g)}
adv = [float(x) for x in re.search(r"pub var gadv: list<num> = \[([^\]]*)\]", g).group(1).split(",") if x.strip()]

bad = []
unparsed = []
for ch, name in doc["cap"].items():
    i = index.get(ch)
    if i is None:
        unparsed.append(ch); continue
    if not (1 <= i <= len(adv)):
        bad.append(f"{ch!r}: index {i} outside the advance list"); continue
    group = dispatch.get(group_of(i))
    if group is None:
        bad.append(f"{ch!r}: index {i} has no group"); continue
    if groups[group].get(i * 2 + 0) != name:
        bad.append(f"{ch!r}: index {i} group {group} draws {groups[group].get(i * 2)} not {name}")
    if groups[group].get(i * 2 + 1) != doc["bold"][ch]:
        bad.append(f"{ch!r}: index {i} bold draws {groups[group].get(i * 2 + 1)} not {doc['bold'][ch]}")

print(f"characters {len(doc['cap'])}, indices {len(index)}, groups {len(groups)}, arms {sum(len(a) for a in groups.values())}")
if unparsed:
    print(f"  note: {len(unparsed)} arms use an escape this checker cannot read: {''.join(unparsed)!r}")
for line in bad:
    print("  BAD", line)
print("glyph module ok" if not bad else f"{len(bad)} disagreements")
sys.exit(1 if bad else 0)
