# case — a costume that remembers whether a letter was a capital

Scratch compares two strings after lowercasing both of them, so `"Raven"` and
`"raven"` are **one string** to `=`, to `<`, to `contains` and to a `switch`
statement, and a table cannot be keyed on a character. This library makes the
difference visible again: `cs_eq` tells the two apart, `cs_find` finds a word
only in the case it was written in, and `cs_fold` turns a text into a key that
Scratch's own `=` compares case-sensitively.

## How it works

One lookup in Scratch does **not** lowercase: a *name* the runtime resolves is
matched with `===` on the exact string (`rendered-target.js`'s
`getCostumeIndexByName` for a costume, `runtime.js`'s `getSpriteTargetByName` for
a sprite). So a costume named `cs_a` and a costume named `cs_A` are two different
costumes, and `switch costume to` picks one of them.

This module declares 53 of them, and a `switch` is the whole lookup:

| | |
| --- | --- |
| `cs_none` | the place a lookup starts from |
| `cs_A` … `cs_Z` | a capital, in order |
| `cs_a` … `cs_z` | and the lowercase ones |

`cs_code` switches to `cs_none`, reads `costume #`, switches to `cs_` + the
character, reads `costume #` again and puts back what the sprite was wearing. The
**difference** between the two numbers is the code: 1 to 26 for `A` to `Z`, 27 to
52 for `a` to `z`. A name Scratch cannot resolve is a `switch` that does nothing
— that is Scratch's own `acceptReporters` dropdown, not an error — so a digit, a
space or an accented letter lands back on `cs_none` and answers **0**, which
means "this character has no case" and falls back to Scratch's answer.

The sprite's own costume is put back *by name* before the macro returns, and a
frame is drawn after the script that runs in it, so the switching is invisible:
what a sprite is wearing at the end of a script is what it wore at the start.

## Installing it

Two files and one `use`:

```sh
cp lib/case/engine.rav project/src/case/engine.rav
cp lib/case/blank.svg  project/assets/cs_blank.svg
```

```rav
use case::engine;
```

That is the whole install, because a **module's costumes are worn by every
target that uses the module**: the 53 arrive in the sprite that compares, after
its own. `engine.rav` is a module, not a target, so nothing is added to
`targets.sprites` in the manifest.

## What you get

The API is five **macros**, and a macro is inlined into the script that calls it.
Each takes the cell to leave its answer in — a macro that *returns* a value can
contain only one expression, and the switch is a statement:

```rav
use case::engine;

on flag_clicked {
    sensing::ask_and_wait("Type Raven");
    let given = sensing::answer();
    let exact = false;
    cs_eq(given, "Raven", exact);
    if exact { looks::say("the capital is where it should be"); }
}
```

| | |
| --- | --- |
| `cs_code(char, code)` | 1…26 for `A`…`Z`, 27…52 for `a`…`z`, 0 for a character with no case |
| `cs_same(a, b, same)` | are two *characters* the same character in the same case |
| `cs_eq(a, b, same)` | are two *texts* the same text in the same case |
| `cs_fold(text, key)` | the case-sensitive key of a text |
| `cs_find(text, needle, at)` | where `needle` first appears in `text` in that case, from 1, or 0 |
| `CS_PREFIX`, `CS_UPPER` | what a costume name is built from, and where uppercase stops |

The cell is written, not declared: `let code = 0;` above the call.

`cs_fold` is the one to reach for beyond the comparisons: `cs_fold("Raven")` is
`RUalvlelnl` — the text with each letter's case written in after it — a string
Scratch compares case-sensitively, so a list of keys, a `match` on a key, or a `<`
between two keys is a case-sensitive table, which is the thing Scratch does not
have. `cs_eq` is two keys and an `=`.

## What it costs

Three `switch` blocks and three reads per character, inlined at the call site:
about eight blocks a character, and a `cs_fold` of a 100-character line is about
a thousand. That is the price of asking a question Scratch's `=` cannot answer,
and it is cheap next to what the alternative costs — `lib/penfont` marks a
capital in the text as `\cH` because it has no costume to spend, and a project
that has no such marker has to write one by hand.

## What it does not do

* **ASCII letters only.** `a`/`A` are told apart; `é`/`É` are not, and fall back
  to Scratch's own answer. Add a costume pair to `engine.rav` for another
  character — `cs_none` first, then the capitals, then the lowercase ones — and
  `cs_code` answers for it with no other change.
* **A costume pair the project drops is a lost case.** Remove `cs_q` and `q`/`Q`
  compare equal again. The library cannot notice: a name Scratch cannot resolve
  and a character with no case are the same 0.
* **The costumes are shared, the answer is not.** Every target that uses the
  module wears all 53 — that is the install — and they sit in its costume list in
  the editor. Nothing else about the sprite changes.
* **Case, not collation.** `cs_fold` says which letters are capitalized; it does
  not put two texts in a case-sensitive *order*.

## The demo

`examples/raven/case` is the smallest project that shows it: it asks for
`Raven`, and says which of "exact", "the right word, in the wrong case" and "not
the word" you typed. The middle answer is the one Scratch cannot give.

```sh
cargo run -p raven -- check -m examples/raven/case/raven.toml
cargo run -p raven -- build -m examples/raven/case/raven.toml --debug
node tools/validate-sb3.js examples/raven/case/dist/case.sb3 --steps 300
```

The runtime side of the mechanism — a name matched with `===`, a miss that does
nothing, and a costume number read back — was read from `scratch-vm`'s
`rendered-target.js` and `scratch3_looks.js`. The last command is what checks it
against a real VM.
