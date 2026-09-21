# Variables, lists and broadcasts

A declaration is `var name = value;` or `list name = [value, ...];`. What the
editor shows for it is described by the same words the editor's own menu uses,
written after the declaration's semicolon:

```rasm
visible var score = 0;          // the monitor starts visible, on the stage
visible list trail = [];        // the same, for a list
var speed = 10; at 5 30         // the monitor is placed at (5, 30)
visible var big = 0; large      // the large readout: one number, no label
visible var power = 0; slider 0 10          // a slider, stepping by 1
visible var fine = 0; slider 0 1 continuous // a slider, stepping by 0.01
```

`visible` comes before `var` or `list`, and after `global` when both are
written; it makes the monitor *start* shown rather than hidden. Everything after
the semicolon describes that monitor:

| Clause | What it sets |
| --- | --- |
| `at X Y` | Where the monitor sits, in stage pixels. |
| `large` | The large readout, `mode: "large"`. |
| `slider MIN MAX` | A slider between two bounds, `mode: "slider"`. |
| `continuous` | A slider that steps by 0.01 instead of 1. |
| `default` | The ordinary readout, when a later clause would override it. |

A declaration that says none of them still gets a position, chosen by the
compiler: monitors are stacked down the left edge of the stage, one row each — a
readout every 38 stage pixels, a list every 205 — so two of them never land on
top of each other. `at X Y` overrides that one monitor.

## Scope: who owns the variable

Scratch has exactly two scopes, and raven-asm writes the second one explicitly:

| Declaration | Scratch's label | Where it lives |
| --- | --- | --- |
| `var score = 0;` inside a sprite file | *For this sprite only* | on that sprite |
| `var best = 0;` inside the stage file | *For all sprites* | on the stage |
| `global var best = 0;` in **any** file | *For all sprites* | on the stage |

```rasm
stage {
    var best = 0;          // the stage's own variable: every sprite can use it
}

sprite "Player" {
    var score = 0;         // this sprite only
    global var games = 0;  // declared here, but owned by the stage
}
```

The rule is about **ownership**, not about which file you are in. A `global var`
is placed on the stage wherever it is written — including in a sprite, and
including in a [module](/raven-asm/multi-file) — so there is exactly one of it for
the whole project. That matters most for modules, where a spriteless file would
otherwise have no way to say what it means.

A sprite that mentions a variable neither it nor the stage declares is a compile
error:

```
error: unknown variable `secret`
  = note: declare it with `var secret = 0;`
```

**Shadowing is rejected.** A sprite may not declare a variable with the same name
as a global, even though Scratch permits it — two variables with one name is a
bug waiting to happen:

```
error: `shared` is already a global variable, so this one would shadow it
  = note: the stage declares a variable named `shared`
  = note: rename one of them, or drop `var` here and use the global
```

Project-wide names must also be unique across files, so `global var dup` in two
files is an error rather than a silent merge.

## Variables

Declare a variable inside a target and give it a starting value:

```rasm
stage {
    var best = 0;          // global: every sprite can use it
}

sprite "Player" {
    var score = 0;         // local: only Player can use it
}
```

A variable declared in `stage` is **global** in Scratch terms: it appears in
every sprite's palette and any sprite can read or write it. A variable declared
in a sprite file belongs to that sprite alone, unless you write `global` in front
of it — then it belongs to the stage instead, and the sprite simply reaches it
the way it reaches any other global.

Values start from the declaration and are only ever changed by blocks:

```rasm
event_whenflagclicked {
    data_setvariableto("score", 0);                              // set score to 0
    data_changevariableby("score", 1);                           // change score by 1
    data_setvariableto("score", operator_add(data_variable("score"), 10));  // add 10
}
```

To read a variable you use the variable reporter block,
`data_variable("score")`, exactly as in the editor.

## Lists

```rasm
sprite "Player" {
    list trail = [];                 // empty
    list options = ["a", "b", "c"];  // three starting items
}
```

Lists share the same scoping rules as variables: declared on the stage means
global, declared in a sprite means local. Starting items must be literals.

The list blocks all take the list name:

```rasm
data_addtolist(motion_xposition(), "trail");      // add (x) to trail
data_insertatlist("x", 1, "trail");               // insert "x" at 1 of trail
data_replaceitemoflist(1, "trail", "y");          // replace item 1 of trail with "y"
data_deleteoflist(1, "trail");                    // delete 1 of trail
data_deletealloflist("trail");                    // delete all of trail

data_setvariableto("first", data_itemoflist(1, "trail"));
control_if(data_listcontainsitem("trail", "x")) { };
control_if_else(operator_gt(data_lengthoflist("trail"), 3)) { } else { };
```

Note the argument order follows the Scratch block: `data_addtolist` is
*item, then list*, because the block reads "add (thing) to (list)".

## Broadcast messages

Broadcast messages are project-wide. Declare one anywhere — the compiler stores
it on the stage, which is where Scratch keeps them:

```rasm
stage {
    broadcast "reset";
}
```

Then send and receive it:

```rasm
event_whenbroadcastreceived("reset") {
    data_setvariableto("score", 0);
}

event_whenkeypressed("r") {
    event_broadcast("reset");            // fire and forget
    event_broadcastandwait("reset");     // wait for every receiver to finish
}
```

Using a message you never declared is an error, with a suggestion if you
mistyped an existing one:

```
error: unknown broadcast message `rest`
  = note: declare it with `broadcast "rest";`
  = note: did you mean `reset`?
```

## Monitors

raven-asm writes a monitor record for every variable and list, hidden by default.
Use the blocks to show and hide them, as you would in Scratch:

```rasm
data_showvariable("score");
data_hidevariable("score");
data_showlist("trail");
data_hidelist("trail");
```

A hidden monitor still exists, so toggling the checkbox in the editor, or
running `data_showvariable`, works as expected.

## Cloud variables

Scratch cloud variables are ordinary variables whose name starts with `☁`.
raven-asm does not treat them specially — declaring one works, and the project file
records it like any other variable. Whether a cloud variable actually syncs
depends on the player you load the project into.
