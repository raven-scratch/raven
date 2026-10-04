# AGENTS.md — working in this repository

Instructions for a coding agent (or a human who wants the short version). The
language guide is in `docs/`; this file is about the *code*.

## What this is

Two languages and one target: Scratch 3, plus the decompiler that walks back.

```
raven (.rav)  ->  raven-asm (.rasm)  ->  project.json  ->  .sb3
sugar, types       one statement,          Scratch 3
macros             one block               file format
                              <- raven-re
```

| Crate | What it holds | Depends on |
| --- | --- | --- |
| `crates/raven-scratch` | The Scratch 3 domain model: the 150-block catalog, `.sb3` container, ZIP writer, deterministic ids, assets, diagnostics. | — |
| `crates/raven-asm` | The assembly-level language, its compiler, CLI, and the generated block reference. | `raven-scratch` |
| `crates/raven` | The high-level language and compiler. | `raven-scratch`, `raven-asm` |
| `crates/raven-re` | The decompiler: a vanilla Scratch 3 `.sb3` back into raven-asm source. | `raven-scratch`, `raven-asm` |

Dependencies only point right. A front end may never be surprised by the layer
above it.

## The three laws

1. **raven-asm never rewrites.** One statement is one Scratch block. A feature
   that needs several blocks belongs in raven, as a macro.
2. **raven never hides.** Every convenience is a macro or a keyword with a
   written lowering, and `raven expand` prints it. A convenience whose shape is
   fixed belongs in `crates/raven/src/prelude.rav`, not as a compiler special
   case.
3. **raven-re never guesses.** A reversal writes one statement per block, reads
   names from the project rather than inventing them, and refuses a project whose
   blocks raven-asm cannot spell — TurboWarp and every other edit of Scratch are
   out of scope.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p raven -- explain rules        # the language, for a machine reader
cargo run -p raven-asm -- catalog --markdown > docs/reference/blocks.md   # regenerate
cd docs && npm install && npm run build                                   # docs site
node tools/validate-sb3.js <file.sb3> --steps 1500   # real Scratch VM (needs SCRATCH_VM_ROOT)
node tools/check-audio.mjs                 # the generated WAVs, against the tunes
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
raven-re-v<version>-windows-x86_64.exe
raven-v<version>-linux-x86_64              raven-asm-v<version>-linux-x86_64
raven-re-v<version>-linux-x86_64
raven-v<version>-macos-aarch64             raven-asm-v<version>-macos-aarch64
raven-re-v<version>-macos-aarch64
```

Every binary can be carried to another machine and run as it is; `raven`
compiles raven to raven-asm itself, and `raven-re` hands its result to the
raven-asm library rather than to an installed `raven-asm`, so neither needs the
other on the `PATH`.
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
node examples/raven/tetris/tools/music.mjs         # the theme
cargo run -p raven -- check  -m examples/raven/tetris/raven.toml
cargo run -p raven -- build  -m examples/raven/tetris/raven.toml --debug
node tools/validate-sb3.js examples/raven/tetris/dist/tetris.sb3 --steps 1500
```

`examples/raven/sudoku` is the second one, and the one to read for a generator: it
builds every puzzle in the project, and the construction it uses is what makes a
puzzle solvable without a guess. Its costumes are generated, so regenerate them
rather than editing them, and use its own check rather than playing it by hand —
that check solves the puzzles the project deals with a solver written outside it:

```sh
node examples/raven/sudoku/tools/assets.mjs       # the costumes
node examples/raven/sudoku/tools/sounds.mjs       # the loop and the effects
cargo run -p raven -- check -m examples/raven/sudoku/raven.toml
cargo run -p raven -- build -m examples/raven/sudoku/raven.toml --debug
SCRATCH_VM_ROOT=../scratch-vm node examples/raven/sudoku/tools/check.mjs
node tools/validate-sb3.js examples/raven/sudoku/dist/sudoku.sb3 --steps 300
```

`examples/raven/chess` is the third, and the one to read when a program has to
agree with something outside it. It is a game against the nine `maia` networks —
1100 to 1900, each trained to predict what a human of that rating would play. The
menu offers all nine. `tools/maia.py` is the engine in Python: lc0 weights in,
the classical 112 input planes, six SE residual blocks, both heads, and the legal
move the policy likes best. `src/engine.rav` is the same engine again in raven,
and a game plays that same move, which is how maia is meant to be played: lc0 with
`go nodes 1` and no temperature answers the same position with the same move every
time.

Nine networks are 7.8 million weights, which is 104 MiB written as decimals. They
fit in 42 MiB because lc0 stores each weight as two bytes read as a fraction of a
layer's range, three of those go in one Scratch number exactly, and `load_bot`
unpacks the one being played and folds its batch norm in. The 863,616 long
working list is written out as a literal rather than appended to, because Scratch
refuses to `add to list` past 200,000 items.

Either side of the board can be you or any of the nine bots, which is what makes
bot against bot a mode rather than a second program. A piece is moved by dragging
it or by clicking it and then its square, and either way it travels on a second
order spring — the state is the position and the velocity, so it can be
retargeted mid-flight, and a frame too long to be stable is subdivided — rather
than on a curve. The pen has one layer for the whole stage and the panel is
stamped on the same one, so a piece in flight is the sprite itself rather than a
stamp: a frame of it costs no pen work, and the board is restamped once, when it
lands.

The Python engine checks its own move generator against the published perft
counts of six standard positions, and `tools/check.mjs` is what keeps the raven
port honest: it loads the built project into a real Scratch VM and compares the
legal move list, every perft count, every one of the nine bots' weights, the
input planes, the whole policy head, the value, the move it picks, and three
games moved by the mouse, against `maia.py --dump`. Its HUD used to stamp one
costume per character per weight, with `tools/hud.mjs` recording every stamp to
check the pitch of each run; that is now `lib/penfont` — Montserrat's two weights
converted into one glyph table and drawn as pen lines — so the costumes, the
generator and the stamp harness went with it, and `src/penfont/` is installed
into the project the same way the penfont example installs it. Its costumes and
its sounds are generated, its board and its panel are laid out from the numbers
in `src/layout.rav` rather than tuned by eye, and the board is drawn with the pen
rather than with clones.

```sh
python examples/raven/chess/tools/maia.py          # the engine, and its perft checks
python examples/raven/chess/tools/maia.py --dump   # what check.mjs compares against
python examples/raven/chess/tools/maia.py --export # regenerate src/net.rav
node examples/raven/chess/tools/assets.mjs         # the pieces (PNGs, through a browser) and the tiles
node examples/raven/chess/tools/sounds.mjs         # the effects
cargo run -p raven -- check -m examples/raven/chess/raven.toml
cargo run -p raven -- build -m examples/raven/chess/raven.toml --debug
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm node examples/raven/chess/tools/check.mjs
```

`lib/penfont` is a library rather than an example, and the one to read when a
project has to stand in for something Scratch does not have. Scratch's pen draws
a line, has no fill block, and will not let a sprite leave the stage — it moves it
back rather than clipping it — so text is a page of pre-computed scanline runs,
and the whole job is proving they land where the font says. `font2vm.py` converts
a font set into runs once — Maple Mono NF CN by default, Microsoft YaHei under
`--set yahei`, and either way Malgun Gothic behind it, which is where the Hangul
is because neither has any — and one run installs both halves into a project:

```sh
python lib/penfont/font2vm.py --project examples/raven/penfont \
  --charset chinese,japanese,korean,nf-dev \
  --chars-file examples/raven/penfont/icons.txt   # the tables and the engine
python lib/penfont/font2vm.py --project examples/raven/penfont --list-charsets
python lib/penfont/font2vm.py --project examples/raven/penfont --stats
cargo run -p raven -- check -m examples/raven/penfont/raven.toml
cargo run -p raven -- build -m examples/raven/penfont/raven.toml --debug
python lib/penfont/font2vm.py --project examples/raven/penfont \
  --stage examples/raven/penfont/dist/page
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm node examples/raven/penfont/tools/check.mjs
```

`--charset` is how a project pays for what it sets and nothing else: twenty named
sets, from `ascii` at 95 glyphs to `ideographs` at every CJK ideograph the font
has, thirteen more for the Nerd Font icon families (`nf-md` at 6,880 down to
`nf-iec` at 5), and bundles over them (`basic`, `latin`, `chinese`, `hanzi`,
`cjk`, `icons`, `all`). The demo takes `chinese,japanese,korean,nf-dev` and its
own `icons.txt`, which is 11,587 glyphs and 15 MB where `all` is 35,592 and 41 MB.
`--stats` and `--stage` write nothing, so the install is the run without either
of them. `engine.rav` is a binary search over the keys, the rows as pen lines cut
at the box the pen may move inside, and the layout; Scratch compares two strings
case-insensitively, so a capital is marked in the text as `\cH` and keyed that
way in the table, and the table is sorted in the order that comparison puts it
in, which is what makes the search valid. A Scratch string is UTF-16 code units,
so the one icon family above the basic plane arrives as two and `draw_text` and
`glyph_of` join the pair before looking it up. The table is also an inventory: B
turns to the sheet, every glyph twelve by eight to a page and 121 pages of them,
drawn by index and not looked up at all, and a find sends one character through
the same search and turns to its page. The order of the checks is the order that
finds things: `--stage` renders the page twice, from the tables and through
FreeType, and refuses to pass if the ink leaves the box; `tools/check.mjs` then
drives the built project through every mode with the renderer's own fence rule,
rasterises the strokes the VM actually made, and holds the stage against that page
— which is 1.000 when every link in the chain agrees — checks the sheet's
ninety-six cells and the ring a find draws, and fails if the sprite stamped.
`examples/raven/penfont` is only the demo: a stage, a mode switch and some keys
over that engine.

`lib/case` is the same shape and much smaller, and it is the one to read when a
program has to ask a question Scratch's `=` cannot answer. Scratch lowercases
both sides of a string comparison, but a *name* the runtime resolves — a costume,
a sprite, a backdrop — is matched with `===`, so `cs_a` and `cs_A` are two
different costumes and the number one of them answers with is a number the case
of the character decided. The library is one module: 53 costumes (`cs_none`, then
`cs_A`…`cs_Z`, then `cs_a`…`cs_z`) that every target using it wears, and macros
`cs_code`, `cs_fold`, `cs_same`, `cs_eq` and `cs_find` that switch a costume,
read `costume #`, and put the sprite's own costume back — a macro because the
switch is statements, and each takes the cell to answer in:

```rav
let exact = false;
cs_eq(sensing::answer(), "Raven", exact);
```

```sh
cargo run -p raven -- check -m examples/raven/case/raven.toml
cargo run -p raven -- build -m examples/raven/case/raven.toml --debug
node tools/validate-sb3.js examples/raven/case/dist/case.sb3 --steps 300
```

`examples/raven/rv32ima` is the fourth, and the one to read when a program has to
be something outside it rather than something of its own: a port of the Scratch
project that runs [mini-rv32ima](https://github.com/cnlohr/mini-rv32ima), the 32
bit RISC-V hart with no MMU, with the terminal redrawn by `lib/penfont`. It runs
three guests from two places — `baremetal.bin` and a Linux 6.1.14 image from
[`bjoernQ/mini-rv32ima-rs`](https://github.com/bjoernQ/mini-rv32ima-rs), and the
Linux image the Scratch project itself carries, cut out of its 162 MB
`project.json` — one `.sb3` each, and `tools/check.mjs` boots all three in a real
Scratch VM and reads the login prompt off the screen. [Read its
README](examples/raven/rv32ima/README.md)
for the four places it follows the Rust rather than the Scratch project, and for
the two it does not.

```sh
git clone https://github.com/bjoernQ/mini-rv32ima-rs ref/mini-rv32ima-rs
node examples/raven/rv32ima/tools/build.mjs
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm \
  node examples/raven/rv32ima/tools/check.mjs
```

`examples/raven-asm/zhcn` is not written by hand: it is `raven-re`'s reversal of a
vanilla Scratch 3 project, a 40,000-glyph pen-drawn Chinese engine. It is
raven-asm, not raven, and it is the example to read when a reversal has to encode
names, expand a compressed reporter or wrap a 40,000-item list. Regenerate it
rather than editing it, and read its README for what the reversal had to say:

```sh
cargo run -p raven-re -- "test/zhcn/A7 四万字纯画笔中文引擎.sb3" --output examples/raven-asm/zhcn --force
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
| Writing raven-asm text: the escapes | `crates/raven-asm/src/source.rs` |
| Reading a `.sb3`: the ZIP reader | `crates/raven-re/src/zipr.rs` |
| The reversal: the vanilla gate and the block walk | `crates/raven-re/src/reverse.rs` |
| Deterministic encodings of names Scratch allows | `crates/raven-re/src/names.rs` |
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
* `crates/raven-re/tests/roundtrip.rs` compiles a project, reverses it, compiles
  it again and compares the two through a fingerprint of everything Scratch can
  observe. A change that alters emitted blocks breaks it.
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
* A `sound` or a non-`pub` `var` written in a *module* file is accepted and then
  ignored. So is a non-`pub` `proc`, `fn`, `const` or `macro`, with no diagnostic
  at all — `lib/penfont/engine.rav` is `pub` everywhere for that reason. A
  `costume` is the exception: every target that uses the module wears it, which is
  how `lib/case` installs 53 costumes with one `use`.
* A module is compiled into each target that uses it, and a procedure parameter
  does not survive that as a local: if the target has a variable of the same
  name, the *target's* wins inside the procedure, silently. `lib/penfont` spells
  every parameter `pf_something` rather than find out what its callers called
  things.
* `control_while`, `control_for_each`, the counter blocks and `sensing_online`
  are extended (TurboWarp-only): reachable, warned about, refused under
  `--strict`.
* A `let` or a `for` in a *stage* script emits a stack list named `_stack1`, and
  so does one in any sprite — but a stage's lists are project-wide and a sprite's
  are its own, so the second is refused with "`_stack1` is already a global list".
  A stage script that needs a block-scoped cell has to be moved to a sprite until
  the naming is fixed; the diagnostic already says it is a bug in raven.
