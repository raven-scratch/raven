# raven and raven-asm

This site documents two languages that share one target and one repository.

| | raven-asm | raven |
| --- | --- | --- |
| Source extension | `.rasm` | `.rav` |
| Manifest | `raven-asm.toml` | `raven.toml` |
| Binary | `raven-asm` | `raven` |
| Rule | one statement is exactly one Scratch block | every convenience is a macro or a written lowering, and all of it prints |
| Compiles to | `.sb3` | raven-asm, then `.sb3` |
| Status | working | working |

If you want to know precisely what a project contains, write raven-asm. If you want
to write it comfortably, write raven — and read the raven-asm it produces.

The arrow also runs backwards: `raven-re` reads a vanilla Scratch 3 `.sb3` and
writes the raven-asm that reproduces it, because one statement is one block means
one block is one statement.

## Why two languages instead of one

Most "a language for Scratch" projects are one language with two personalities: a
comfortable surface and a hidden lowering. The comfortable part is pleasant and
the hidden part is where the trouble starts — the `.sb3` stops resembling the
source, error messages point at generated code, and each new convenience has to
know about every other one.

Splitting the job makes the contract explicit:

* **raven-asm** is small enough to read in an afternoon and boring enough to
  trust. It never rewrites anything, so its output *is* its input, block for
  block.
* **raven** is where every convenience lives, and each one is a macro or a
  keyword with a written lowering. Either way it becomes raven-asm, so the worst a
  convenience can do is write other raven-asm — which is still code you can read.

That is the whole arrangement: **the interesting layer is allowed to exist
because the boring layer underneath cannot be surprised by it.**

## The pieces

The repository is a Cargo workspace with four crates:

| Crate | What it holds |
| --- | --- |
| `raven-scratch` | The Scratch 3 domain model: the block catalog, the `.sb3` container, deterministic ids, asset hashing, diagnostics. Neither language owns it. |
| `raven-asm` | The assembly-level language and its compiler. Depends on `raven-scratch`. |
| `raven` | The high-level language and its compiler. Depends on `raven-scratch` and on `raven-asm`. |
| `raven-re` | The decompiler: a vanilla Scratch 3 `.sb3` back into raven-asm source. Depends on the same two. |

The dependency edges only ever point in that direction. `raven-asm` does not know
that `raven` exists, and `raven-scratch` does not know that either of them exists.

## Getting started

```sh
git clone https://github.com/raven-scratch/raven
cd raven
cargo install --path crates/raven-asm
cargo install --path crates/raven
cargo install --path crates/raven-re
```

The [getting started guide](/guide/getting-started) walks through a first project
from there.

## Where each language is described

* [What is raven-asm?](/raven-asm/) — the one-block rule, the manifest, the CLI.
* [What is raven?](/raven/) — the surface syntax, the type system, the macro
  system, and the exact lowering.
* [What is raven-re?](/raven-re/) — the reverse direction, what it refuses, and
  what raven-asm has no syntax for.
* [Design laws](/raven/design) — what raven is not allowed to add over
  raven-asm, and why.
* [Block reference](/reference/blocks) — the generated list of every block both
  languages can reach.
* [For LLMs](/guide/for-llms) — `raven explain`, and the loop to run when a model
  writes the raven.