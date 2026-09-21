# AGENTS.md — working in this repository

Instructions for a coding agent (or a human who wants the short version). The
language guide is in `docs/`; this file is about the *code*.

## What this is

Two languages and one target: Scratch 3.

```
raven (.rav)  ->  raven-asm (.rasm)  ->  project.json  ->  .sb3
sugar, types       one statement,          Scratch 3
macros             one block               file format
```

| Crate | What it holds | Depends on |
| --- | --- | --- |
| `crates/raven-scratch` | The Scratch 3 domain model: the 150-block catalog, `.sb3` container, ZIP writer, deterministic ids, assets, diagnostics. | — |
| `crates/raven-asm` | The assembly-level language, its compiler, CLI, and the generated block reference. | `raven-scratch` |
| `crates/raven` | The high-level language and compiler. | `raven-scratch`, `raven-asm` |

Dependencies only point right. A front end may never be surprised by the layer
above it.

## The two laws

1. **raven-asm never rewrites.** One statement is one Scratch block. A feature
   that needs several blocks belongs in raven, as a macro.
2. **raven never hides.** Every convenience is a macro or a keyword with a
   written lowering, and `raven expand` prints it. A convenience whose shape is
   fixed belongs in `crates/raven/src/prelude.rav`, not as a compiler special
   case.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p raven -- explain rules        # the language, for a machine reader
cargo run -p raven-asm -- catalog --markdown > docs/reference/blocks.md   # regenerate
cd docs && npm install && npm run build                                   # docs site
node tools/validate-sb3.js <file.sb3> --steps 1500   # real Scratch VM (needs SCRATCH_VM_ROOT)
```

`tools/validate-sb3.js` needs a checkout of `scratch-editor`'s `scratch-vm`; set
`SCRATCH_VM_ROOT` to `packages/scratch-vm`. It is the only end-to-end runtime
check — use it whenever a change alters emitted blocks.

## Releasing and docs

Releases are manual: the `Release` workflow takes a version, and two flags.

```sh
# from anywhere, with a token that has the workflow scope
pwsh ./tools/release.ps1 0.1.0                    # a release, notes from commits
pwsh ./tools/release.ps1 0.2.0 -Prerelease        # a beta
pwsh ./tools/release.ps1 0.2.0 -Draft -Notes "…"  # a draft, with written notes
```

The workflow refuses to run unless `[workspace.package] version` in the root
`Cargo.toml` already equals the version you pass. It builds a standalone binary
for each platform — `+crt-static`, so the Windows `.exe` needs no runtime
installed — and attaches them to the release as

```
raven-v<version>-windows-x86_64.exe        raven-asm-v<version>-windows-x86_64.exe
raven-v<version>-linux-x86_64              raven-asm-v<version>-linux-x86_64
raven-v<version>-macos-aarch64             raven-asm-v<version>-macos-aarch64
```

Either binary can be carried to another machine and run as it is; `raven`
compiles raven to raven-asm itself and does not need `raven-asm` on the `PATH`.
Re-running the workflow for a version that already has a release replaces the
binaries in it instead of failing, which is how to rebuild them for an existing
tag.

Docs deploy themselves: `.github/workflows/docs.yml` regenerates the block
reference, builds `docs/` with VitePress and publishes to GitHub Pages on every
push to `main` that touches `docs/`. The repository's Pages source has to be set
to **GitHub Actions** once, in Settings ▸ Pages.

## Examples

`examples/raven/tetris` is the worked example: a complete game, with its own
README explaining the shape of the code. It is not wired into CI or the test
suite — build it by hand when a change touches the emitted blocks:

```sh
cargo run -p raven -- check  -m examples/raven/tetris/raven.toml
cargo run -p raven -- build  -m examples/raven/tetris/raven.toml --debug
node tools/validate-sb3.js examples/raven/tetris/dist/tetris.sb3 --steps 1500
```

## Where things live

| Need | File |
| --- | --- |
| A block's opcode, inputs, fields, shape, stability | `crates/raven-scratch/src/catalog.rs` |
| The `.sb3` format and ZIP writing | `crates/raven-scratch/src/sb3.rs`, `zipw.rs` |
| Asset loading, rotation centres, md5 ids | `crates/raven-scratch/src/assets.rs`, `ids.rs` |
| raven-asm grammar | `crates/raven-asm/src/parser.rs`, `lexer.rs` |
| raven-asm semantics | `crates/raven-asm/src/compile.rs` |
| The block reference generator | `crates/raven-asm/src/docs_gen.rs` |
| raven grammar | `crates/raven/src/parser.rs`, `lexer.rs` |
| raven semantics: names, types, macros, cells | `crates/raven/src/lower.rs` |
| The raven name of every block | `crates/raven/src/stdlib.rs` |
| Dropdowns as enum types | `crates/raven/src/menu.rs` |
| Pure/sampled/effectful per block | `crates/raven/src/purity.rs` |
| The machine-readable language reference | `crates/raven/src/explain.rs` |
| The prelude, in raven | `crates/raven/src/prelude.rav` |

## Adding a block

1. One row in `catalog.rs`. It compiles, validates and appears in the generated
   reference immediately.
2. Run `cargo test`: `crates/raven/src/stdlib.rs` has a totality test that fails
   and names the block until it is bound to a raven name (a callable, a syntax
   spelling, a hat, or a deliberate refusal with a reason).
3. If it is not a one-to-one block, bind it as `Syntax` and implement the
   lowering in `lower.rs`, and say what it costs in `docs/raven/lowering.md`.

The same shape holds for everything else that must not drift: the block
reference is generated from the catalog, `explain`'s stdlib and menu sections are
generated from the binding table, and the docs are checked against the code.

## What a change must not break

* `cargo test --workspace` — includes the headline memory law: a built project
  declares no Scratch variable no `watch` asked for.
* `docs/reference/blocks.md` is generated: regenerate it rather than editing it.
* The `identity` modules are the single place a name is declared. Read names from
  them; never hardcode a crate name, manifest filename, source extension, or the
  repository/docs URL in a new string.
* Diagnostics, not panics. A malformed input is a rendered error with a span, a
  note and the closest match when there is one.

## Conventions

* Comments say what the code does now; no history, no "used to". Prefer a short
  paragraph that explains a non-obvious rule over a line per function.
* Rust: 2021 edition, `rust-version = 1.82`, no new dependencies without a
  reason a reader can check.
* Never write a Scratch variable from generated code unless it is a `watch`
  mirror; the five `data_*` blocks stay refused.
* `docs/` prose is written for humans, in complete sentences. Explanations of
  *why* belong there, not in the code.

## Known warts (do not "fix" by documentation drift)

* `for x in items` takes the list's *name*, not an expression: the macro reads the
  list's length through an `ident` parameter, so `items.at(2)` is refused.
* A `costume`, a `sound` or a non-`pub` `var` written in a *module* file is
  accepted and then ignored. Modules export items, not target content.
* `control_while`, `control_for_each`, the counter blocks and `sensing_online`
  are extended (TurboWarp-only): reachable, warned about, refused under
  `--strict`.
