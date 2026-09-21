# Command line

raven-re is one command, because a decompiler does one thing: read an `.sb3`,
write the raven-asm project that reproduces it.

```
raven-re [OPTIONS] <INPUT>
```

| Argument | What it is |
| --- | --- |
| `<INPUT>` | The `.sb3` file to reverse. |

| Option | What it does |
| --- | --- |
| `-o`, `--output <DIR>` | Directory to write the project into. Defaults to the input's file name in the current directory. |
| `--force` | Write into an output directory that already holds files. |
| `-h`, `--help` | Usage. |
| `-V`, `--version` | Version. |

## Reversing a project

```sh
raven-re examples/tetris.sb3
```

```
   Reversed examples/tetris.sb3 -> tetris
             3 target(s), 386 block(s), 18 asset(s), 14 monitor(s)
     Verified tetris/raven-asm.toml rebuilds (386 block(s))

   Next: cd tetris && raven-asm build
```

The output directory is a raven-asm project, laid out exactly as `raven-asm new`
lays one out:

```
raven-asm.toml              which files are targets, and which extensions are used
src/stage.rasm              the stage
src/sprites/<name>.rasm     one file per sprite, named after the sprite
assets/<md5>.<ext>          every costume and sound, under the name the .sb3 used
```

Nothing else is written. There is no `README.md` and no `.gitignore`, because
raven-re writes what the project needs to compile and nothing else.

## Naming the output directory

`-o` decides where the project goes. Without it, the file name of the input names
both the project and its directory:

```sh
raven-re my-game.sb3                  # ./my-game/raven-asm.toml
raven-re my-game.sb3 -o work/game     # ./work/game/raven-asm.toml
raven-re my-game.sb3 -o .             # the current directory
```

A directory that already holds files is refused unless `--force` is passed, so a
reversal cannot scatter itself over an unrelated project:

```
error: `my-game` already exists and is not empty
  = note: pass `--force` to write into it anyway, or `--output` to pick another directory
```

## The result is verified

After writing, raven-re hands the project straight back to raven-asm's compiler.
A reversed project that does not build is a bug in raven-re, and the report names
the generated line that is wrong:

```
error: unknown variable `score`
   --> my-game/src/sprites/player.rasm:14:13
...
error: the project raven-re wrote does not compile
  = note: the sources are in `my-game`, so the report above can be read there
```

That check is why the command exits zero only when the project it wrote really
rebuilds. The sources are left on disk either way, so a failure can be read
before it is reported.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | The project was reversed, written and rebuilt successfully. |
| `1` | A refusal, a bad file, or a failure to write. |
| `2` | Bad command line usage (from the argument parser). |

## Reading the warnings

Warnings describe what raven-asm cannot write, not what went wrong. They are
printed to standard error before the summary, and the reversal succeeds:

```sh
raven-re old-project.sb3 2>warnings.txt
```

See [What has no raven-asm syntax](/raven-re/#what-has-no-raven-asm-syntax) for
the list and what each one costs.

## Round-tripping a project

A reversal followed by a build is the identity on everything Scratch runs, so it
is a safe way to read a project you did not write:

```sh
raven-re my-game.sb3 --output my-game-asm
cd my-game-asm
raven-asm check                    # the source is valid raven-asm
raven-asm build                    # dist/my-game.sb3
```

The rebuilt project is not byte-identical to the one you fed in — identifiers are
derived from the source, monitors are rewritten, and the normalisations above
apply — but every block, variable, list, broadcast, costume, sound, custom block
and monitor that Scratch can observe is the same. That equality is what
raven-re's own test suite checks, by compiling a project, reversing it, compiling
it again, and comparing the two.
