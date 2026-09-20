# raven-asm

The assembly-level language and compiler: one statement is exactly one Scratch
block, and the compiler never rewrites anything.

Its command line tool, its manifest format and its source extension are all
documented in the [guide](https://raven-scratch.github.io/raven/raven-asm/).

```sh
cargo install --path .

raven-asm new hello
cd hello
raven-asm build
```
