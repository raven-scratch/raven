# Procedures

Scratch's *My Blocks* are custom blocks. raven-asm calls them procedures and writes
them with `proc`.

```rasm
sprite "Player" {
    proc zigzag(degrees: num, steps: num) warp {
        motion_turnright(argument_reporter_string_number("degrees"));
        motion_movesteps(argument_reporter_string_number("steps"));
    }

    event_whenflagclicked {
        control_repeat(10) {
            zigzag(15, 10);
        }
    }
}
```

## Parameters

**Every parameter carries its type.** There is no default: a parameter without
one is a compile error, because a type that is implied by absence is a type
nobody can see.

```rasm
proc hop(height: num, message: str, loud: bool) { }
```

| Written | Scratch's input | proccode | Read it with |
| --- | --- | --- | --- |
| `name: str` | string | `%s` | `argument_reporter_string_number("name")` |
| `count: num` | number | `%n` | `argument_reporter_string_number("count")` |
| `flag: bool` | boolean | `%b` | `argument_reporter_boolean("flag")` |

The type selects the *shape of the slot*, which is what a reader sees on the
canvas: `str` and `num` are the rounded slot, `bool` is the hexagonal one.
`str` and `num` are separate because Scratch's editor separates them — the
`proccode` records which — but both are read with the same reporter block,
because that is the block Scratch provides.

The reader is checked against the declaration, so the two can never drift:

```rasm
proc gate(open: bool) {
    control_if(argument_reporter_boolean("open")) {   // ok
        …
    };
}
```

```
error: `name` is declared `: str`, so `argument_reporter_boolean` cannot read it
  = note: read it with `argument_reporter_string_number("name")`
```

```
error: `flag` is not a parameter of this procedure
  = note: its parameters are: open: bool
```

## What a call site looks like

Arguments are positional, and the compiler checks each one against the declared
type — a `: bool` slot takes a boolean reporter and nothing else, exactly as the
editor would refuse anything but a hexagon.

```rasm
zigzag(15, 10);
gate(operator_lt(motion_xposition(), 100));   // any boolean reporter fits
```

Because a call is a bare name, it cannot reuse a block's name: `proc
motion_movesteps(n: num)` is rejected, since it would silently take over
`move (n) steps` in every script of that target.

## Procedures do not return values

Scratch custom blocks have no return value, so a procedure call is a statement,
never an expression, and a result has to travel through a variable:

```rasm
proc double(n: num) warp {
    data_setvariableto("result", operator_multiply(argument_reporter_string_number("n"), 2));
}

event_whenflagclicked {
    double(21);
    looks_say(operator_join("result = ", data_variable("result")));
};
```

## `warp` — run without screen refresh

`proc name(…) warp { … }` sets the `warp` flag on the custom block, which is
Scratch's *Run without screen refresh* checkbox. A warped block finishes in a
single frame, which is what you want for tight drawing loops:

```rasm
proc square(size: num) warp {
    control_repeat(4) {
        motion_movesteps(argument_reporter_string_number("size"));
        motion_turnright(90);
    }
}
```

`warp` goes between the parameter list and the body, and applies to the whole
custom block.

## What ends up in the file

One `proc` declaration becomes the two blocks Scratch uses to store a custom
block:

```
procedures_definition            topLevel, holds the prototype
└── procedures_prototype         mutation: proccode, argumentids, argumentnames,
    │                                     argumentdefaults, warp
    ├── argument_reporter_string_number("height")    shadow
    └── argument_reporter_boolean("loud")            shadow
    … and the body follows the definition
```

Each call site becomes one `procedures_call` block whose mutation carries the
same `proccode` and the argument ids, with your arguments plugged into those
slots. The `proccode` (`zigzag %n %n`) is what the runtime uses to match a call
to its definition, so it is identical on both sides.

Because Scratch resolves calls by `proccode`, two procedures in the same target
with the same name and the same parameter types would be indistinguishable —
raven-asm rejects duplicate procedure names in a target for that reason.

## Sharing procedures between sprites

Scratch custom blocks live inside one target, and so do raven-asm's. To use the same
procedure in several sprites, put it in a module and `use` it — the definition is
copied into each target that asks for it. See
[Multiple files](/raven-asm/multi-file).