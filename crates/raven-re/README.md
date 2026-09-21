# raven-re

The decompiler: a vanilla Scratch 3 `.sb3` back into raven-asm source.

It exists because raven-asm is reversible by design — one statement is exactly
one Scratch block — so a project can be read back as the code that would have
written it.

```sh
cargo install --path .

raven-re my-game.sb3        # writes ./my-game/
cd my-game && raven-asm build
```

The tool, its options and what it refuses are documented in the
[guide](https://raven-scratch.github.io/raven/raven-re/).
