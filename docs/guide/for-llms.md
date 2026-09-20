# For LLMs

A model writing raven has no REPL and cannot open the editor, so everything it
needs has to be printable by the compiler itself. Two commands exist for that:

```sh
raven explain all          # the language, written for a machine reader
raven-asm catalog --json   # every block of the layer below, with its arguments
```

`raven explain` is not the human guide with the prose removed. It is the facts a
generator needs, in the order it needs them: the rules that reject code, the
grammar, the exact block cost of every construct, the memory model, the full
standard library — generated from the same binding table the compiler checks
against — the menu types with their variants, the prelude source, and the CLI.
It is deterministic, so it is safe to cache, diff or paste into a context
window.

```sh
raven explain rules        # the hard rules that reject code, first
raven explain grammar      # EBNF, including all keywords
raven explain costs        # blocks emitted per construct
raven explain memory       # _vms, _gvm, _stackN, and what a cell is
raven explain stdlib       # one line per catalog block: opcode, spelling, args, result
raven explain menus        # <MenuType>::<Variant> for every dropdown
raven explain prelude      # crates/raven/src/prelude.rav, verbatim
raven explain cli          # every command, and what it writes
```

## The loop that works

1. Read `raven explain rules grammar stdlib` once and keep it in context. The
   standard library section is what stops a model inventing a block: a name that
   is not in it does not exist.
2. Write `.rav` files.
3. `raven check` — parse, resolve, type check and expand, without writing. Every
   diagnostic names the line and, for generated code, the macro that produced it.
4. `raven expand` — the raven-asm the program became. This is how a model checks
   its own cost claims and how it sees that `score += 1` really is three blocks
   on a cell rather than one Scratch variable.
5. `raven build` — the `.sb3`. `tools/validate-sb3.js` loads it into the real
   Scratch VM and runs it, which is the only end-to-end check available outside
   the editor.

## What a generated program must respect

* **No Scratch variables.** Everything is a cell of `_vms`, `_gvm` or the
  script's `_stackN`; `watch` is the one exception, and it exists to be looked
  at. The five blocks that name a Scratch variable are refused.
* **`raven-asm` never rewrites.** One statement below raven is one block. If a
  feature seems to need several blocks, it is a raven macro with a printed
  lowering, not a new block.
* **Every convenience is printable.** A construct whose expansion `raven expand`
  cannot print is not part of the language.
* **A macro may not nest inside itself**, so never write a `for` inside a `for`;
  the outer loop is `repeat n { … }`. Expansion stops at 32 levels.
* **`&&` and `||` evaluate both sides**, comparisons do not chain, and
  `pop`/`push`/`insert`/`remove`/`clear`/`set` are statements, not values.

## Teaching the model the codebase

`AGENTS.md` at the repository root is the other half: where the compiler's parts
are, which tests fail when a table drifts, and how to add a block or a keyword
without breaking the two rules above. Read it before editing the compiler; read
`raven explain` before writing raven.
