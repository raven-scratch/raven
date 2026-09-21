# Blocks and Scratch

This page explains what the compiler does with each statement, in the order it
does it. It is the reference for *why* a generated `.sb3` looks the way it does.

## Statements become blocks

A statement produces exactly one block, plus one block per reporter nested in
its arguments.

```rasm
data_setvariableto("score", operator_add(data_variable("score"), 1));
```

```text
data_setvariableto  ── fields: VARIABLE = ["score", <id>]
                    └─ inputs: VALUE = [3, <operator_add>, [10, ""]]

operator_add        ── inputs: NUM1 = [3, <data_variable>, [4, ""]]
                    │           NUM2 = [1, [4, "1"]]
                    └─ parent: data_setvariableto

data_variable       ── fields: VARIABLE = ["score", <id>]
                    └─ parent: operator_add
```

Three blocks, three statements in the Scratch editor. No block is invented and
none is inlined. Here it is both ways — the source on the left, what the editor
draws on the right:

<div class="cmp">
<div class="cmp-col">
<h4>raven-asm</h4>

```rasm
event_whenflagclicked {
    data_setvariableto("health", operator_add(data_variable("health"), 1));
    control_if(operator_gt(data_variable("health"), 3)) {
        looks_say("Feeling better.");
    };
}
```

</div>
<div class="cmp-col">
<h4>Scratch</h4>

<pre class="blocks" v-pre>
when green flag clicked
set [health v] to ((health) + (1))
if <(health) > (3)> then
    say [Feeling better.]
</pre>

</div>
</div>

The nesting is visible in both directions: `operator_add` sits inside the
`set` block's input, exactly as `health + 1` sits inside the square socket on
the canvas, and `control_if`'s brace body becomes the blocks under its mouth.

## Inputs and fields

Every argument in the [block reference](/reference/blocks) is either an **input**
or a **field**. The difference is Scratch's, not raven-asm's:

| | Scratch name | Written in raven-asm as | Example |
| --- | --- | --- | --- |
| input | a slot that can hold a block or a value | a literal or a reporter | `motion_movesteps(10)` |
| field | a dropdown baked into the block | a string literal | `motion_setrotationstyle("left-right")` |

The compiler reads the argument list positionally, using the order shown in the
reference. `data_addtolist("thing", "list")` is item first, list second, because
that is how the Scratch block reads.

## Dropdowns

A dropdown is always written as a string, and its value is checked while
compiling:

```rasm
looks_seteffectto("ghost", 50);        // ok, matched case-insensitively
looks_seteffectto("sparkle", 50);      // error: not a valid value for this dropdown
```

```
error: `sparkle` is not a valid value for this dropdown
   --> src/sprites/player.rasm:12:21
    |
 12 |     looks_seteffectto("sparkle", 50);
    |                     ^^^^^^^^^
    = note: accepted values: COLOR, FISHEYE, WHIRL, PIXELATE, MOSAIC, BRIGHTNESS, GHOST
```

Values are matched case-insensitively and written into the project in the
canonical spelling — for **closed** dropdowns, the ones whose whole value set is
fixed. A dropdown that names something in your project is matched **exactly**,
because that is how the block resolves it at run time:

* `looks_costume` → costumes declared by this target
* `looks_backdrops` → backdrops declared by the stage
* `sound_sounds_menu` → sounds declared by this target
* `motion_goto_menu`, `sensing_touchingobjectmenu`, `control_create_clone_of_menu`, …
  → sprite names, plus their built-in options such as `"_mouse_"`, `"_edge_"`,
  `"_random_"`, `"_myself_"` and `"_stage_"`

Because the domain is known, a typo in a costume name is a compile error rather
than a blank dropdown in the editor.

Some menus also accept a **reporter**, which is what Scratch's own
`acceptReporters` flag means: `looks_switchcostumeto(data_variable("x"))`,
`motion_goto(data_variable("x"))` and `sound_play(data_variable("x"))` all
compile, and the value is checked at run time rather than at compile time.

## Substacks

Blocks that take a `SUBSTACK` are written with braces. `control_if_else` takes
`else { … }` for its second branch.

| raven-asm | Scratch |
| --- | --- |
| `control_forever { … }` | `forever` with a substack |
| `control_if(cond) { … }` | `if … then` |
| `control_if_else(cond) { … } else { … }` | `if … then … else` |
| `control_repeat(10) { … }` | `repeat 10` |
| `control_repeat_until(cond) { … }` | `repeat until` |
| `event_whenflagclicked { … }` | `when green flag clicked`, body attached to its `next` |

An empty body is fine and simply leaves the substack empty:

```rasm
control_if(sensing_mousedown()) { };
```

## Hats, caps and script order

* A statement at the **top level** of a target starts a script and must be a hat
  block: `event_whenflagclicked`, `event_whenkeypressed("space")`,
  `event_whenbroadcastreceived("reset")`, `control_start_as_clone`,
  `event_whengreaterthan`, …
* A hat written inside another block is an error, because Scratch has no nested
  hats.
* A **cap** block has no bottom notch. `control_forever`,
  `control_delete_this_clone` and `control_stop("all")` end their script; the
  compiler reports anything written after them.
  `control_stop("other scripts in sprite")` and
  `control_stop("other scripts in stage")` *do* leave a notch, and their
  `hasnext` mutation is written automatically.
* Reporters are never scripts. A top-level reporter is an error.

## Procedures

`proc` and calls to it are the one place where one raven-asm declaration maps to the
standard *two-block* Scratch representation, because that is how Scratch stores
custom blocks: a `procedures_definition` holding a `procedures_prototype` with a
mutation, and a `procedures_call` block per call site. See
[Procedures](/raven-asm/procedures).

## Monitors

Scratch keeps a *monitor record* for every variable and list: it is what the
checkbox next to a variable controls. raven-asm writes one record per declared
variable and list, with the `mode`, slider bounds and `visible` flag the
declaration asked for. A monitor with no `at X Y` is stacked down the left edge
of the stage, one row each — a readout every 38 stage pixels, a list every 205 —
so no two of them ever land on top of each other. See
[Variables](/raven-asm/variables#monitors). Top-level scripts, in turn, are laid
out the way the editor's own **Clean up Blocks** lays them out: one column at
`x = 0`, each script below the last with the renderer's `MIN_BLOCK_HEIGHT`
between them.

They are hidden by default so a project does not open covered in readouts. To
show one, use the block — which is also how Scratch does it:

```rasm
data_showvariable("score");    // make the readout appear
data_hidelist("trail");        // hide the list
```

## Extensions

raven-asm bundles the **Pen** and **Music** extensions. Using a `pen_*` or `music_*`
block adds the extension to the project automatically; you can also list it in
`raven-asm.toml` to enable it before anything uses it.

A few blocks in the reference are marked *extended*: they exist in TurboWarp and
in the extended Scratch VM that raven-asm is verified against, but not in the
vanilla Scratch editor (`control_while`, `control_for_each`,
`control_all_at_once`, the counter blocks, and `sensing_online`). Using one
produces a warning so you know the project needs an extended player.

## Identity and reproducibility

Block, variable, broadcast and costume identifiers are derived deterministically
from the project structure, so building the same source twice produces the same
bytes and `git diff` stays quiet on a rebuild.
