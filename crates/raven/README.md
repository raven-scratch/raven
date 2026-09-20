# raven

The high-level language. It compiles to [`raven-asm`](../raven-asm), which
compiles to a Scratch 3 `.sb3`.

The language is specified in the documentation under `docs/raven/`, and the
specification is the contract. This crate is the implementation.

```sh
cargo install --path .

raven new hello
cd hello
raven check           # parse, resolve, type check and expand; write nothing
raven build           # dist/hello.sb3, and nothing else
raven build --debug   # also dist/asm/ and dist/project.json
raven expand          # the raven-asm this program became
raven fmt --check     # canonical indentation, without writing
raven clean           # remove dist/
```

A raven project declares no Scratch variables: every `var`, `let`, `for` counter
and returned value is a cell of `_vms` (the target's arena) or `_gvm` (the
project's), addressed by a constant index. The five blocks that name a Scratch
variable are refused with an explanation. `list<T>` and `map<K, V>` are Scratch
lists reached through raven's checked methods, and a `struct` is a fixed frame of
cells — a place, not a value. A `bool` can be stored anywhere a value can, because
reading one back is a single comparison: `<value = "true">`.

Where the pieces live:

| Module | Role |
| --- | --- |
| `lexer.rs`, `parser.rs` | The grammar. |
| `ast.rs` | The surface syntax, with every convenience already a macro call or a keyword. |
| `module.rs` | The manifest, the sources, and the module graph. |
| `lower.rs` | Names, types, macro expansion, the memory system's layout, and the descent to raven-asm, in one traversal. |
| `stdlib.rs` | Every catalog block bound to its raven name: callable, covered by syntax, or refused — with a totality test. |
| `menu.rs`, `ty.rs`, `purity.rs` | Dropdowns as types, the type model (scalars, lists, maps, structs), and the pure/sampled classification. |
| `rasm.rs` | Prints raven-asm source. |
| `driver.rs` | Writes the staging project, or the debug tree, and hands it to `raven-asm`. |
| `cli.rs` | The command line, and what each subcommand reports. |
| `fmt.rs` | `raven fmt`: canonical indentation and blank lines, line by line. |
| `scaffold.rs` | What `raven new` and `raven init` write. |
| `diag.rs` | Turning a compiler error into the rendered diagnostic a user reads. |
| `identity.rs` | The names this tool answers to: crate, binary, manifest, source extension. |
| `prelude.rav` | The prelude, written in raven. |
