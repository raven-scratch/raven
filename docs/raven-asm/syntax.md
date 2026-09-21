# Syntax

The whole grammar fits on one page. raven-asm deliberately has no more than this.

## Lexical rules

* **Comments** — `// to end of line` and `/* block comments */` (they do not
  nest).
* **Identifiers** — `[A-Za-z_][A-Za-z0-9_]*`. Block names are the exact Scratch
  opcodes, so they may contain underscores and capitals (`pen_penDown`).
* **Numbers** — `10`, `-3.5`, `1e3`. A leading `-` is only valid directly before
  a number, because Scratch has no negation operator.
* **Strings** — `"…"` with `\\`, `\"`, `\n`, `\r`, `\t`, `\0` and `\u{1F600}`.
* **Booleans** — `true`, `false`.
* **Punctuation** — `( ) { } [ ] , ; = :`.
* **Whitespace** — insignificant. Statements are separated by `;`, not newlines.

## File

```text
file        = { use } { item }
use         = "use" STRING ";"
```

A file with no `stage`/`sprite` declaration is a [module](/raven-asm/multi-file);
a file with one is a [target](/raven-asm/project-structure).

## Target

```text
target      = "stage" block
            | "sprite" STRING block
```

`stage` takes no name (Scratch calls it `Stage`). Everything inside the braces
belongs to that target.

## Declarations

```text
costume     = "costume" STRING "=" STRING [ "center" NUMBER NUMBER ] ";"
sound       = "sound" STRING "=" STRING ";"
variable    = [ "global" ] [ "visible" ] "var" IDENT "=" literal ";" [ monitor ]
list        = [ "global" ] [ "visible" ] "list" IDENT "=" "[" [ literal { "," literal } ] "]" ";" [ monitor ]
monitor     = { "at" NUMBER NUMBER | "large" | "slider" NUMBER NUMBER | "continuous" | "default" }
broadcast   = "broadcast" STRING ";"
procedure   = "proc" IDENT "(" [ params ] ")" [ "warp" ] block
params      = param { "," param }
param       = IDENT ":" ( "str" | "num" | "bool" )
literal     = STRING | NUMBER | "true" | "false"
```

```rasm
costume "costume1" = "assets/costume1.svg" center 32 32;
sound "beep" = "assets/beep.wav";
var score = 0;
visible var shown = 0;         // the monitor starts visible
var speed = 10; at 5 30        // the monitor is placed at (5, 30)
list trail = ["start", 2];
global var best = 0;
global list board = [];
broadcast "reset";
proc hop(height: num, loud: bool) warp { … }
```

`global` decides which target **owns** the variable. Without it a declaration
belongs to the target whose file declares it: a sprite-local variable, or, in the
stage file, one that every sprite can see. With it the variable is placed on the
stage whichever file declares it, so it has exactly one instance for the whole
project. [Variables](/raven-asm/variables) explains why that distinction carries
weight.

Variables may be initialised with a literal only. There is no expression
evaluation at compile time — if you want a computed starting value, write the
`data_setvariableto` block yourself.

## Statements

```text
stmt        = opcode [ "(" [ expr { "," expr } ] ")" ]
              ( ";" | block [ "else" block ] [ ";" ] )
block       = "{" { stmt } "}"
expr        = literal | reporter
reporter    = opcode "(" [ expr { "," expr } ] ")"
```

Three shapes, and that is all:

```rasm
looks_hide;                             // no arguments: the parentheses are optional
motion_movesteps(10);                   // arguments, then a semicolon
control_repeat(10) { … }                // a substack in braces
control_if_else(cond) { … } else { … }  // two substacks
```

A statement with braces attaches its body to the block's `SUBSTACK` (or, for a
hat block, to its `next`). `else` is only accepted by `control_if_else`.

Reporters nest inside argument lists:

```rasm
data_setvariableto("score", operator_add(data_variable("score"), 1));

control_if(operator_and(sensing_keypressed("space"), operator_not(sensing_mousedown()))) {
    looks_say("jump!");
};
```

## Reporters

A reporter is written as a call and always needs its parentheses, even with no
arguments:

```rasm
motion_xposition()
sensing_timer()
operator_join("a", data_variable("name"))
```

Only blocks with a round or hexagonal shape may be used as values. A command
block in argument position is an error, and so is a reporter used as a
statement.

## Literals

| Written | Scratch keeps | Notes |
| --- | --- | --- |
| `10`, `-2.5`, `1e3` | `"10"`, `"-2.5"`, `"1e3"` | The number is stored exactly as you typed it. |
| `"hello"` | `"hello"` | |
| `true` / `false` | `"true"` / `"false"` | For boolean **inputs**, use a boolean reporter instead. |
| `"#ff8800"` | `"#ff8800"` | Only where a colour is expected. |

## What is *not* in the grammar

raven-asm has no operators, no statement keywords and no expressions beyond the
list above. These are all compile errors:

```rasm
if (x > 1) { }          // there is no `if` keyword — use control_if(operator_gt(…)) { }
x = x + 1;              // no assignment — use data_setvariableto("x", operator_add(…));
let y = 2;              // no `let` — use var, or data_setvariableto
for i in 0..10 { }      // no `for` — use control_repeat and a counter variable, or control_for_each
"score: " + score       // no concatenation — use operator_join
```

The [design notes](/raven-asm/design) explain why, and what to write instead.
