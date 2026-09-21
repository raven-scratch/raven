# zhcn — a 40,000-character pen-drawn Chinese engine

A pure-pen Chinese text engine: 40,000 glyphs, drawn as pen strokes, with
bold/italic/underline/code runs, line wrapping and paging. The whole thing is
Scratch blocks — there is no costume per character, only a list of stroke data
and the custom blocks that read it.

This project is not written by hand. It is `raven-re`'s reversal of
`test/zhcn/A7 四万字纯画笔中文引擎.sb3`, a vanilla Scratch 3 project, and it is an
example of what a `.sb3` becomes when one statement really is one Scratch block.
It is raven-asm rather than raven, so `raven-asm build` compiles it.

```
raven-asm.toml                    the manifest, and the one extension the project uses
src/stage.rasm                    the stage: the font, the backdrop, four broadcasts
src/sprites/画字.rasm               drawing: the glyph renderer and its layout engine
src/sprites/字库.rasm               the library: the glyph table and its lookup
src/sprites/范围编辑.rasm            range editing: what the page shows
src/sprites/角色1.rasm              the invisible role the engine clones
assets/14e46ec3e2ba471c2adfe8f119052307.svg   the one costume every role wears
```

## Build

```sh
raven-asm check          # parse and validate, write nothing
raven-asm build          # dist/A7 四万字纯画笔中文引擎.sb3
```

## What the reversal had to do

* **The names are encoded.** `画字`, `字库` and the rest are not ASCII
  identifiers, and a raven-asm name is an identifier, so every one of them is
  written as `re_` followed by its UTF-8 bytes in hex. The comment above each
  declaration in `src/stage.rasm` says what the project called it. The custom
  blocks keep their Scratch proccodes the same way.
* **The font is the data.** `src/stage.rasm` is one list of glyphs and one list
  of 40,000 indexes, so the file is large and mostly numerals. It is wrapped to
  a column so it stays readable and diffable, which is cosmetic: the parser does
  not care where an item ends and the next begins.
* **Two things were dropped**, each with a warning when the project was
  reversed: two blocks that sit loose in the workspace and are not part of any
  script, and one Scratch comment.
