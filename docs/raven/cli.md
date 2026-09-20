# Command line

The `raven` binary has one job per stage of the compiler, and each stage has a
command that prints what it produced. Nothing is a black box.

```sh
raven new hello          # a project with a working example
cd hello
raven check              # parse, resolve, type check, expand; write nothing
raven build              # write dist/hello.sb3
raven expand             # print the raven-asm this program becomes
```

## Commands

| Command | Purpose |
| --- | --- |
| `raven new <name>` | Create a project (`--here`, `--force`, `--with-module`). |
| `raven init [path]` | Scaffold into an existing directory (`--force`). |
| `raven check` | Run every front-end stage and report diagnostics without writing. |
| `raven build` | Compile to `dist/<name>.sb3` (`--debug`, `-m <manifest>`). |
| `raven expand` | Print the raven-asm for the whole project. |
| `raven explain [section]` | Print the language reference written for a machine reader. |
| `raven fmt` | Canonical indentation and blank lines (`--check`, paths). |
| `raven clean` | Remove the output directory. |

Every command that needs a project takes `-m/--manifest`, defaulting to
`raven.toml` in the current directory. The manifest has the same shape as
raven-asm's.

## The stage commands

### `raven check`

Runs lexing, parsing, module resolution, name resolution, type checking, macro
expansion and lowering, and stops before anything is written. It reports:

* syntax errors, with a line, a column and a caret;
* unresolved names, with the closest match when there is one;
* type errors, naming both the expected and the found type;
* macro errors: a wrong argument kind, a wrong argument type, a substitution that
  would be evaluated twice, an expansion cycle;
* shape errors: a value where a name is wanted, a name where a value is wanted.

`check` never writes and never panics. A malformed file is a diagnostic, not a
stack trace.

### `raven expand`

Prints the raven-asm source the project lowered to, one section per target file.
That is the same text `raven build --debug` writes, which makes it the fastest
way to answer "what did that cost?".

```sh
raven expand                     # the whole project
raven expand -m other/raven.toml # another project
```

A `for` loop, a `match`, an `f"…"` string and a `let` are all visible in the
result, because by the time this command runs there is no such thing as any of
them: they are `repeat_until`, `control_if_else`, `operator_join` and
`data_replaceitemoflist` into `_vms`. A `return` prints the `control_stop` it
became, and a call whose value is used prints the cell it read. A variable is not
a Scratch variable at all, so `n += 1` prints a read, an `operator_add` and a
write.

### `raven build --debug`

A plain `raven build` writes one file: `dist/<name>.sb3`. It also takes back the
`dist/asm/` tree a previous `--debug` left behind, so that tree cannot go stale —
but `dist/project.json` is not a file it manages, and stays where it is until you
delete it.

`--debug` is the switch that turns a build into something you can inspect. It
writes two more things beside the archive:

* `dist/asm/` — the raven-asm program raven lowered to, complete with its own
  `raven-asm.toml`, `src/**/*.rasm` and relative asset paths, so it can be built
  on its own. This is also how you check that raven did not cheat.
* `dist/project.json` — the uncompressed Scratch project, exactly as
  `raven-asm build --debug` writes it.

```sh
raven build --debug
cd dist/asm && raven-asm build   # the same project raven built
```

The next plain build removes `dist/asm/` again — a file of your own in it keeps
the directory alive — and leaves `dist/project.json` alone.

### `raven explain`

The same facts the rest of this site describes, written for a machine reader —
a model that has to emit raven source and cannot ask questions:

```sh
raven explain              # every section
raven explain rules        # the hard rules that reject code
raven explain grammar      # EBNF, with every keyword
raven explain costs        # blocks emitted per construct
raven explain memory       # _vms, _gvm, _stackN, and what a cell is
raven explain stdlib       # one line per catalog block: opcode, spelling, args, result
raven explain menus        # <MenuType>::<Variant> for every dropdown
raven explain prelude      # crates/raven/src/prelude.rav, verbatim
raven explain cli          # the commands and what they write
```

It writes to standard output and needs no project, so it can be cached or pasted
into a context window. The `stdlib` and `menus` sections are generated from the
binding table rather than retyped, so a block cannot be described here and
missing from the compiler. See [For LLMs](/guide/for-llms) for how to use it.

### `raven fmt`

Re-indents from the brace depth, trims trailing whitespace and collapses runs of
blank lines. It is a line formatter, not a pretty-printer: it never reflows code
and never drops a comment, because comments are not part of the syntax and a
formatter that rebuilt the file from the syntax tree would delete every one of
them.

```sh
raven fmt                 # every .rav file under src/
raven fmt src/sprites     # one directory
raven fmt --check         # report, and write nothing
```

With no paths it formats the project the current directory's `raven.toml`
describes, so it needs a manifest to be there; paths may be given from anywhere.

## Diagnostics

raven reports the way `raven-asm` does — a line-oriented format that reads the
same in a terminal, an editor and a CI log:

```text
error: `score` must be `num`, found `str`
  --> src/sprites/player.rav:18:23
   |
18 |                 score += 1;
   |                       ^^^^^
   = note: convert it with `num()`; the two directions Scratch coerces are free
```

Errors inside generated code report the line *you* wrote and add what the macro
was and where it is defined:

```text
   = note: it is declared as count_up($times: expr<num>, $body: block) -> stmts
   = note: it is defined in src/lib/shapes.rav:32:12
```

There is no output in which the caret points at text you did not type.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | A diagnostic. The message says which. |
| `2` | Bad command line usage, from the argument parser. |

An internal panic would print a Rust backtrace instead, which is always a bug:
the compiler is expected to fail with a diagnostic.

## Relation to `raven-asm`

`raven` deliberately shares conventions with `raven-asm` rather than inventing
its own: the same manifest shape, the same error format, the same `dist/`
layout, the same `--debug` file. Learning one tool's flags is learning the
other's.
