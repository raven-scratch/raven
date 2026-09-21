# Command line

raven-asm works like Cargo: one binary, a handful of subcommands, all of them
manifest-driven.

```
raven-asm <COMMAND> [OPTIONS]
```

| Command | What it does |
| --- | --- |
| `raven-asm new <name>` | Create a project directory with the default scaffold (`--here`, `--force`, `--with-module`). |
| `raven-asm init [path]` | Add a project to a directory you already have (`--force`). |
| `raven-asm build` | Compile into `dist/<project>.sb3`. |
| `raven-asm check` | Parse, resolve and validate; write nothing. |
| `raven-asm clean` | Delete the output directory. |
| `raven-asm catalog` | Print the block catalog. |

Every command that reads a project accepts `-m` / `--manifest-path` and defaults
to `raven-asm.toml` in the current directory.

## `raven-asm new`

```sh
raven-asm new my-game
raven-asm new . --here           # scaffold into the current directory
raven-asm new my-game --force    # write into a directory that already has files
raven-asm new my-game --with-module   # also write src/lib/shapes.rasm
```

## `raven-asm build`

```sh
raven-asm build                       # dist/my-game.sb3
raven-asm build --debug               # also dist/project.json, pretty-printed
raven-asm build --strict              # refuse extended blocks
raven-asm build -m path/to/raven-asm.toml
```

```
    Finished dist/hello.sb3 (21986 bytes)
             2 target(s), 36 block(s), 3 asset(s), 3 monitor(s)
```

Warnings are printed to standard error and do not stop the build:

```
warning: this project uses 2 block(s) that vanilla Scratch does not have
  = note: control_while, sensing_online
  = note: these run on TurboWarp and on other extended Scratch runtimes
```

`--strict` turns that warning into an error, so a project that must open in the
vanilla editor cannot pick up an extended block by accident. `check` accepts the
same flag.

## `raven-asm check`

Exactly the same work as `build`, minus the file writes. Use it in an editor
hook or CI:

```sh
raven-asm check || echo "not shipping that"
```

## `raven-asm clean`

```sh
raven-asm clean
#    Removed dist
```

It reads the manifest for the output path and deletes the directory, without
compiling anything — so a project whose sources no longer build can still be
cleaned.

## `raven-asm catalog`

The catalog is the same table the compiler uses, so this is the authoritative
answer to "what can I write?".

```sh
raven-asm catalog                       # grouped by category, with signatures
raven-asm catalog -c motion             # only categories matching "motion"
raven-asm catalog --category motion    # the same, spelled out
raven-asm catalog --json                # machine readable
raven-asm catalog --markdown > docs/reference/blocks.md
```

```
Motion
------
  motion_movesteps(steps);
      move %1 steps
  motion_turnright(degrees);
      turn right %1 degrees
  motion_goto("to");
      go to %1
  …
```

The [block reference](/reference/blocks) on this site is generated with
`raven-asm catalog --markdown`, so it can never drift from the compiler.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success, possibly with warnings. |
| `1` | A compile error, a bad manifest, or a failed file operation. |
| `2` | Bad command line usage (from the argument parser). |

## Using it from a script

```sh
# fail the build if anything is wrong, then package
raven-asm check && raven-asm build && cp dist/my-game.sb3 public/
```

Because identifiers are derived deterministically, an unchanged project
produces a byte-identical `.sb3`, which makes it safe to commit or diff the
output if you want to.

## The other direction

raven-asm is reversible, because one statement is one block. `raven-re` reads a
vanilla Scratch 3 `.sb3` and writes the raven-asm project that reproduces it, then
compiles it to prove the reversal holds:

```sh
raven-re dist/my-game.sb3 --output my-game-asm
```

See [What is raven-re?](/raven-re/) for what it refuses and what raven-asm has no
syntax for.
