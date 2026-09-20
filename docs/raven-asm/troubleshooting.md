# Troubleshooting

raven-asm tries to make every failure point at something you can change. This page
collects the messages you are most likely to meet.

## `unknown block`

```
error: unknown block `motion_movestep`
   --> src/sprites/player.rasm:12:9
    |
 12 |         motion_movestep(10);
    |         ^^^^^^^^^^^^^^
    = note: raven-asm block names are the exact Scratch opcodes
    = note: did you mean `motion_movesteps`?
    = note: run `raven-asm catalog` to list every block
```

raven-asm uses Scratch's own opcodes, not English or Rust-like names. `motion_movesteps`,
`control_repeat_until`, `looks_say`, `pen_penDown`. Search the
[block reference](/reference/blocks) or run `raven-asm catalog -c motion`.

## `takes N arguments`

```
error: `motion_movesteps` takes 1 argument, but 2 were given
    = note: signature: motion_movesteps(steps);
    = note: you wrote: 10, 20
```

Arguments are positional, so the signature note is the thing to read. The
reference lists them in order for every block.

## `is not a valid value for this dropdown`

```
error: `sparkle` is not a valid value for this dropdown
    = note: accepted values: COLOR, FISHEYE, WHIRL, PIXELATE, MOSAIC, BRIGHTNESS, GHOST
```

Dropdowns are checked while compiling. Values are matched ignoring case, so
`"ghost"` and `"GHOST"` are the same.

## `unknown variable` / `unknown list`

```
error: unknown variable `score`
  = note: declare it with `var score = 0;`
```

Declare the variable in the same file, or — if you meant the global one — in the
stage file. A sprite can read and write the stage's variables but not another
sprite's.

## `unknown broadcast message`

Declare it with `broadcast "name";` anywhere in the project. Messages are
project-wide, so a sprite can declare one too.

## `nothing can follow`

```
error: nothing can follow `control_forever` from line 8
  = note: `control_forever` is a cap block with no bottom notch
```

`forever`, `delete this clone` and `stop all` end their script. Move the
following blocks inside the loop, or use a different block.

## `needs a body`

```rasm
control_repeat(4);        // error: control_repeat needs a body
control_repeat(4) { };    // fine: an empty body
```

Blocks that take a `SUBSTACK` are written with braces. Blocks that do not take
one reject braces in the same way.

## `expected a condition`

A hexagonal input needs a boolean block:

```rasm
control_if(1) { };                                        // error
control_if(operator_gt(data_variable("score"), 1)) { };   // fine
control_if(sensing_mousedown()) { };                      // fine
```

`operator_lt`, `operator_gt`, `operator_equals`, `operator_and`, `operator_or`,
`operator_not`, `operator_contains`, `sensing_touchingobject`, `sensing_keypressed`,
`data_listcontainsitem` and the `argument_reporter_boolean` blocks are all
boolean.

## `expected a literal value`

```rasm
var score = operator_add(1, 2);   // error: initial values must be literals
var score = 0;                    // fine
```

Set computed values with `data_setvariableto` inside a script instead.

## `cannot find module` / `circular module inclusion`

`use` paths are relative to the file that writes them, and the `.rasm`
extension is optional. A cycle is reported with the chain that closes it.

## `a module cannot declare a …`

A module has no target of its own, so it cannot own a variable, list, costume,
sound or script. Two ways out:

* if the state should be shared by everyone, write `global var` / `global list`;
* if it should be per sprite, declare it in the sprite that uses the module.

## `is already a global variable, so this one would shadow it`

A sprite is declaring a variable whose name the stage already uses. Scratch would
let both exist; raven-asm refuses, because every later reference would be ambiguous.
Rename one, or drop the local declaration and use the global.

## `duplicate global …`

Project-wide names must be unique across every file. Two files declaring
`global var score` is an error rather than a silent merge; rename one, or delete
one and let the other stand.

## `parameter ... needs a type`

```
error: parameter `height` needs a type, found `)`
  = note: write `: str`, `: num` or `: bool` after every parameter name
  = note: `str` and `num` are read with `argument_reporter_string_number`; `bool` is read with `argument_reporter_boolean`
```

There is no default parameter type. An implied type is one nobody can see, so
every parameter states its own:

```rasm
proc hop(height) { }              // error
proc hop(height: num) { }         // fine
```

## `is declared ..., so ... cannot read it`

The argument-reporter block has to match the parameter's type: `: bool` is read
with `argument_reporter_boolean`, and `: str` / `: num` with
`argument_reporter_string_number`. Mixing them would emit a block of the wrong
shape, so it is refused rather than quietly compiled.

## `is a Scratch block, so it cannot also be a procedure`

A procedure call is a bare name, so a procedure that reuses a block's name would
silently take over that block in every script of the target. Rename it;
`raven-asm catalog` lists every name that is already taken.

## `cannot work out the size of \`…\`, so its rotation centre is unknown`

The image header or the SVG has no usable dimensions, and raven-asm refuses to
guess: the rotation centre is what every motion block turns around. Give the image
a `width` and `height`, or a `viewBox`, or add an explicit centre:

```rasm
costume "weird" = "assets/weird.svg" center 64 64;
```

## `declares no costumes`

Every target in a Scratch project needs at least one costume: the editor records
the current costume as an index, and a target with an empty costume list has no
valid index to record, so the project loads into a sprite that cannot be drawn.

Add one — a `costume` declaration is enough, the file does not have to be drawn
by hand:

```rasm
sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";
}
```

## `a sprite cannot be named \`Stage\``

`Stage` is the stage's name and it is fixed. Two targets with the same name make
the editor resolve a monitor against whichever it finds first, so raven-asm
refuses the collision rather than writing a project that behaves differently than
it reads.

## `is a PNG image but its extension says it is a JPG image`

Assets are checked against their extension when they are loaded, because a project
that builds and then fails to open is worse than one that does not build. Rename
the file to match what it actually is, or declare the right path.

## The project opens, but a block does nothing

Check whether the block is marked **extended** in the reference. `control_while`,
`control_for_each`, `control_all_at_once`, the counter blocks and
`sensing_online` do not exist in the vanilla Scratch editor; they run on
TurboWarp and on the extended VM raven-asm is tested against. raven-asm warns at
build time when you use one; `raven-asm build --strict` turns that warning into an
error, so a build that must run in the vanilla editor cannot be produced by
accident.

If a block is missing a dropdown value, the costume or backdrop it names may
have been moved. raven-asm checks those names at compile time, so a successful build
means they resolved.

## Still stuck?

Run `raven-asm check` and read the note lines — they usually name the fix — then
`raven-asm catalog` to confirm the exact signature. The
[block reference](/reference/blocks) is generated from the same table, so it is
always current.
