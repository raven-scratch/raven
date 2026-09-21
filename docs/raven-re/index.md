# What is raven-re?

raven-re is the other direction. It reads a Scratch 3 `.sb3` file and writes the
raven-asm project that reproduces it:

```
.rav  ──▶  .rasm  ──▶  project.json  ──▶  .sb3
                                  ◀──  raven-re
```

The arrow is reversible because of the rule the whole workspace is built on: in
raven-asm one statement is exactly one Scratch block. Nothing has to be guessed
at or reconstructed, because nothing was ever collapsed. raven-re walks a
project's blocks and writes one line for each of them.

What comes out is an ordinary raven-asm project — `raven-asm.toml`,
`src/stage.rasm`, one file per sprite, and every costume and sound under
`assets/` — so `raven-asm check` validates it and `raven-asm build` writes an
`.sb3` again. `raven-re` compiles its own result before it exits, so a run that
succeeds is a run whose output builds.

```sh
raven-re my-game.sb3            # writes ./my-game/
cd my-game && raven-asm build   # writes ./dist/my-game.sb3
```

## Vanilla Scratch 3 only

raven-asm targets vanilla Scratch 3, and so does raven-re. A block that vanilla
Scratch does not have has no raven-asm spelling at all, so a project that uses
one is refused before anything is written, naming what gave it away:

```
error: `control_while` is not a vanilla Scratch 3 block
  = note: only TurboWarp and other edited Scratch runtimes provide it
  = note: raven reverses vanilla Scratch 3 projects only: TurboWarp and any
          other edit of Scratch or TurboWarp are not supported
```

The check is on what the project *contains*, not on who saved it. A project
TurboWarp saved stays reversible as long as every block in it is one vanilla
Scratch has; a project the Scratch editor saved is refused if it carries an
extension raven-asm does not bundle. Concretely, a project is refused when it

* uses a block only an extended runtime provides — `control_while`,
  `control_for_each`, `control_all_at_once`, the counter blocks, `sensing_online`;
* uses a block no Scratch has at all, which is every TurboWarp, Penguinet or
  fork-specific opcode;
* lists an extension other than `pen` and `music`, or loads one from a URL;
* says `TurboWarp` in `meta.agent`;
* holds a shadow block that is neither a dropdown, a custom block prototype nor
  one of its parameter reporters;
* is missing an asset it refers to, or was zipped as a folder rather than as an
  `.sb3` (its files sit under a directory instead of at the root).

Being refused is a hard stop: nothing is written to disk.

## The names Scratch allows and raven-asm does not

Scratch lets a variable, a list, a custom block or a parameter be called
anything at all. Real projects in the wild hold names like `"foo`, `< Perfect`
and `a&b`. A raven-asm name is an identifier, so a name that cannot be written
as one is replaced by an encoding of itself:

```
    // the project calls this `my score`; raven-asm cannot write that as an identifier
    var re_6d792073636f7265 = 0;
```

The encoding is `re_` followed by the name's UTF-8 bytes in hex. It is a pure
function of the name, so reversing the same file twice writes the same source —
the same guarantee a raven-asm build gives for the identifiers it derives. An
encoded name is itself a legal identifier, so reversing a project twice through
`raven-asm` and back is stable.

A name is moved aside rather than encoded only when two names in one target would
collide, or when a sprite's own variable would shadow a stage variable, which
raven-asm refuses. Custom block parameter names follow the same rule.

## What has no raven-asm syntax

raven-asm is a small language on purpose, so a handful of things a `.sb3` can
hold have no spelling in it. Each is reported as a warning and dropped rather
than approximated:

| What | Why |
| --- | --- |
| Scratch comments | raven-asm has no comment blocks. |
| A costume's `bitmapResolution` | raven-asm always writes `1`. A costume drawn at resolution `2` therefore changes size in the rebuilt project. |
| Monitors that watch a reporter | raven-asm writes a monitor for a variable and for a list and for nothing else. |
| Blocks loose in the workspace | A block that is not attached to a script never runs, and raven-asm has no way to write one. |
| Volume, layer order, tempo, draggable, rotation style, the current costume | raven-asm's target defaults. A costume or sound's own data is kept; these fields are not. |

Warnings go to standard error and do not stop the reversal:

```
warning: 1 Scratch comment(s) were dropped
  = note: raven-asm has no syntax for it
  = note: raven-asm has no comment blocks
```

Two further differences are the compiler's own normalisations rather than
dropped data, and both are invisible to the running project:

* An input's literal is written as text, so a `10` that was stored as a JSON
  number comes back as `"10"`. Scratch casts either way.
* The spelling of a fixed dropdown is canonicalised: `"color"` becomes
  `"COLOR"`, `"TIMER"` becomes `"timer"`.

Three more come from raven-asm's shape rather than from anything dropped, and
they are worth knowing when you read the output:

* **A custom block's label is re-spelled.** Scratch puts each `%s`, `%n` or `%b`
  wherever the parameter goes — `找字 %s 大写: %s` is a real proccode — and
  raven-asm can only write a parameter at the end. The label is what remains once
  the placeholders are taken out, so the rebuilt block reads
  `re_... 大写: %s %s`. Every call is rewritten with it, so the block still means
  what it meant; only its spelling moves.
* **A reporter standing alone in an input is written out again.** The Scratch
  serializer compresses a lone variable, list or broadcast reporter into
  `[12, "score", "id"]` inside the input. raven-asm writes it as the block it
  is, `data_variable("score")`, which is what Scratch expands it back to.
* **A long list is wrapped.** A list with 40,000 items is written a few items per
  line, because a five-megabyte line is a file no editor or diff can open.
  Whitespace between items means nothing, so this changes nothing but the shape.

## Determinism

Everything raven-re decides is a function of the file it read: names are encoded
from their own text, collisions are broken with a hash of the id the project
already gave the declaration, and the output directory is written in a fixed
order. Two runs over the same bytes write the same bytes, and the `.sb3`
`raven-asm build` produces from them is reproducible in the usual way.

## Where to go next

* [Command line](/raven-re/cli) — the tool, its options and its exit codes.
* [What is raven-asm?](/raven-asm/) — the language that is written back out.
* [Block reference](/reference/blocks) — every block a reversal can meet.
