# Coming from Scrust

[Scrust](https://github.com/DilemmaGX/Scrust) was the earlier attempt at this
idea. It is worth reading its report card, because raven is defined by the answers.

## What Scrust did well

* It proved a Rust-flavoured surface over Scratch is *legible* — `if`, `repeat`,
  `match`, `proc`, `fn` all read as well as you would hope.
* It built a working list-backed allocator (`_RAM`, `sys_alloc`, `sys_free`) that
  gave procedures local variables, shadowing and recursion on vanilla Scratch.
  That is a real result and it is still interesting.
* Its `sb3.rs` and its declarative extension tables (`extensions/pen.toml`) are
  the right shape, and raven-asm's catalog is a descendant of that idea.

## What went wrong

| Problem | Where it showed | raven's answer |
| --- | --- | --- |
| Sugar lived in the compiler, in four separate passes | `match`, `join`, imports and `let` each rewrote the AST differently | One mechanism: [macros](/raven/macros), all of them printable |
| `return` did not stop execution | the lowering set a return variable and fell through | `return` is `control_stop` on the custom block: it stops |
| Every `let`, `return` and value-returning call silently became a list allocator and a script split | generated projects had `_RAM`, `_FREE_PAGES`, `_HIGH_WATER` in them, and nothing in the source said so | Two published arenas, `_vms` and `_gvm`, indexed by constants; every access is in `raven expand` |
| No type checking | `var` was `Type::Unknown`; a string in a numeric slot was found at run time | [Declared types](/raven/types), checked across the program |
| Hard panics instead of diagnostics | `panic!` on an unknown block | Diagnostics only; a panic is a bug report |
| Every script laid out at `(0, 0)` | `// TODO: layout` | Deterministic grid layout, inherited from raven-asm |
| List literals silently became `[10, ""]` | an unsupported construct was approximated | Unsupported constructs are errors |
| No macros | none, only a fixed `#[…]` attribute set | The macro system is the language's centre |
| No monitors | `monitors: Vec::new()` | A monitor for every declared list, emitted by raven-asm; a scalar has no monitor unless it is `watch`ed, which declares the one Scratch variable raven ever writes |
| Every variable was a Scratch variable | variables came and went with the project's data model | No nameable Scratch variable exists: the name-at-run-time blocks are refused, and every value is a cell |

## Translation

| Scrust | raven |
| --- | --- |
| `package foo { … }` | just a file: `src/foo.rav` |
| `use pkg::thing;` | `use lib::thing;` — see [modules](/raven/modules) |
| `#[on_flag_clicked] fn main() { }` | `on flag_clicked { }` |
| `#[warp] proc p() { }` | `proc p() warp { }` |
| `public var SCORE = 0;` / `private var HP = 100;` | `pub var SCORE: num = 0;` (a cell of `_gvm`) / `var HP: num = 100;` (a cell of the target's `_vms`) |
| `x = 1` | `x = 1;` — a `var` is declared first; a `let` is not |
| `let y = 10;` | `let y = 10;` — a mutable cell, scoped to its block |
| `x += 1` | `x += 1;` |
| `proc add(a: number, b: number) -> number { return a + b; }` | the same, with `num`: `proc add(a: num, b: num) -> num { return a + b; }` |
| `return v` | `return v;` — and it stops |
| `if c { } else if d { }` | `if c { } else { if d { } }` — or a `match` |
| `match x { 1 => … }` | `match x { 1 => { … }, _ => { … } }` |
| `join("a", "b", "c")` | `f"a{b}{c}"` — or nested `operators::join` |
| `"…"` with no escapes | `"…"` with `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\{`, `\}`, `\u{…}` |

## What raven deliberately did not keep

* **The allocator as a black box.** Scrust's `_RAM` grew and shrank at run time,
  so the blocks in the project no longer told you what the program did; the cost
  of a `let` depended on how many had come before it. raven's
  [virtual memory system](/raven/design#the-virtual-memory-system) keeps the part
  that mattered — scoped, shadowing, mutable locals on vanilla Scratch — and drops
  the part that did not: cells are fixed at compile time, so nothing is allocated,
  freed or indexed dynamically. `_vms` is declared in the project and every access
  to it is a printed block.
* **Attributes.** `#[on_flag_clicked]` hides an event binding in a place that is
  hard to grep and impossible to type-check without a second grammar. raven uses
  keywords.
* **`return` that falls through.** A language whose `return` does not return is
  worse than a language with no `return`, because the failure is silent.
* **Precedence tables with silent fallbacks.** Scrust's parser had match arms
  that dropped duplicate cases with the comment *"in a real compiler we should
  report error"*. A real compiler reports the error.
