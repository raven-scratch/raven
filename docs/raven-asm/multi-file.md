# Multiple files

A raven-asm project is a directory of files, and each target lives in its own. This
page covers how they connect.

## Targets

One file declares one target. The manifest lists them:

```toml
[targets]
stage = "src/stage.rasm"
sprites = ["src/sprites/player.rasm", "src/sprites/enemy.rasm"]
```

Targets are independent: they have their own variables, costumes, sounds,
procedures and scripts. The only things shared between them are the stage's
global variables and the project's broadcast messages.

## Modules

Scratch custom blocks belong to one target, so the same procedure cannot be
"shared" at runtime. raven-asm solves this the only way a compiler can: it copies
the definition into every target that asks for it.

A module is a file with no target:

```rasm
// src/lib/shapes.rasm

proc square(size: num) warp {
    control_repeat(4) {
        motion_movesteps(argument_reporter_string_number("size"));
        motion_turnright(90);
    }
}

proc polygon(sides: num, size: num) warp {
    control_repeat(argument_reporter_string_number("sides")) {
        motion_movesteps(argument_reporter_string_number("size"));
        motion_turnright(operator_divide(360, argument_reporter_string_number("sides")));
    }
}
```

A sprite opts in with `use`:

```rasm
// src/sprites/player.rasm
use "../lib/shapes.rasm";

sprite "Player" {
    costume "costume1" = "assets/costume1.svg";

    event_whenflagclicked {
        square(40);
    }
}
```

`Player` now has its own copy of `square` and `polygon`. If two sprites both
`use` the same module, each gets its own copy — which is exactly what the
Scratch editor would contain if you had dragged the blocks into both.

::: warning `use` is inclusion, not import
It is worth being precise, because the syntax is Rust's but the semantics are
not. Rust's `use` binds a *name* to an item that exists once, in one place.
raven-asm's `use` **copies** the module's procedures into each target, because a
Scratch custom block belongs to exactly one target and there is no way to share
one. Think `#include`, or a template instantiation, rather than `use crate::…`.

That is the entire reason the next section exists.
:::

## Module state

A module has no target of its own, so it cannot own a variable — a plain `var`
in a module is an error. What it *can* declare is state that has exactly one
instance for the whole project, which is a `global`:

```rasm
// src/lib/stats.rasm

global var runs = 0;
global list run_log = [];

proc record(turns: num) {
    data_changevariableby("runs", 1);
    data_addtolist(argument_reporter_string_number("turns"), "run_log");
}
```

`runs` and `run_log` land on the stage, and every sprite that uses `stats.rasm`
binds to the same two variables. One module, one meaning.

Compare this with a procedure that reaches for a name it does not declare:

```rasm
proc note_local() {
    data_changevariableby("per_painter", 1);
}
```

`per_painter` is not declared here. It resolves against whichever target pulled
the module in, so it is `painter.rasm`'s business to declare it. If a different
sprite used this module without declaring the name, that sprite would get an
error pointing at the line above.

The two halves compose into a simple rule:

> A module **declares** the state it wants to share, and **borrows** the state it
> wants per sprite — and the sprite, not the module, decides what that borrowed
> state is.

## Rules

* **`use` paths are relative to the file that writes them.** A sprite in
  `src/sprites/` reaches a module in `src/lib/` with `use "../lib/shapes.rasm";`.
* **The `.rasm` extension is optional** — `use "../lib/shapes"` works too.
* **Modules may contain `proc` definitions, `broadcast` declarations, and
  `global var` / `global list`.** Everything else belongs to a target: a plain
  `var`, a `costume`, a `sound` or a script in a module is an error, because a
  module has no target to hang them on.
* **Modules may `use` other modules**, which is how a library is assembled.
* **A module is included once per target.** `use`-ing the same file twice from
  two paths does not duplicate its procedures.
* **Cycles are rejected**, with the chain printed:

  ```
  error: circular module inclusion
    = note: a.rasm -> b.rasm -> a.rasm
  ```

* **Included procedures are emitted first**, in the order the module was pulled
  in, followed by the target's own items. That order is stable, so rebuilds are
  reproducible.

## A larger layout

```
my-game/
├── raven-asm.toml
├── src/
│   ├── stage.rasm
│   ├── lib/
│   │   ├── shapes.rasm         drawing helpers
│   │   ├── motion.rasm         movement helpers, uses shapes.rasm
│   │   └── ui.rasm             say/think helpers
│   └── sprites/
│       ├── player.rasm         use "../lib/motion.rasm";
│       ├── enemy.rasm          use "../lib/motion.rasm";
│       └── hud.rasm            use "../lib/ui.rasm";
└── assets/
```

`player` and `enemy` each end up with their own copy of the movement helpers,
and `hud` with its own copy of the UI helpers. The source stays small; the
generated project stays exactly what Scratch expects.
