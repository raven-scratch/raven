# Standard library

raven's standard library is not a separate body of code. It is the block catalog,
given names and types.

That is the whole design: Scratch's 150 blocks are the vocabulary, the catalog
already records each one's opcode, category, shape and dropdowns, and raven derives
its API from that table. A block added to the catalog arrives with a typed,
namespaced raven name, or the build fails.

## The rules

1. **One module per catalog category**, lowercased: `motion`, `looks`, `sound`,
   `events`, `control`, `sensing`, `operators`, `data`, and the extension modules
   `pen` and `music`.
2. **One function per block**, named in `snake_case` from the opcode with the
   category prefix removed. The mapping is a curated table in the compiler, not a
   derivation, because `looks_sayforsecs` does not split itself into
   `say_for_secs` and a guess would be worse than a row.
3. **Parameters are the block's inputs, in the block's order.** An `Input`-wired
   parameter takes an expression. A `Field`-wired parameter takes a **name** —
   a variable, a list, a broadcast or a dropdown value — which the checker
   resolves against what the target declares.
4. **Dropdowns become menu types**, so a typo in a dropdown is a type error
   rather than a warning from the runtime.
5. **Hat blocks have no function.** They are written as [`on` scripts](/raven/syntax#scripts).

```rav
motion::move_steps(10);
looks::say_for_secs("go!", 2);
control::wait_until(sensing::key_pressed(Key::Space));
let n = trail.len();
trail.push(5);
trail.remove(1);
sound::set_effect_to(SoundEffect::Pitch, 10);
pen::set_pen_size_to(3);
music::play_drum_for_beats(Drum::Snare, 0.25);
```

## The console

`console` is not a Scratch block and not in the catalog: it is three functions
over one list, `_console`, which the stage declares **only if something in the
project logs**. Nothing a program does not use is emitted.

| written | blocks | meaning |
| --- | --- | --- |
| `console::log(x)` | one `add x to _console` | one line per call, like `console.log` in JavaScript |
| `console::clear()` | one `delete all of _console` | empty the log |
| `console::count()` | one `length of _console` | how many lines are in it |

Its monitor starts **hidden**. Showing the log is the developer's decision: tick
`_console` in the editor's variable list and it is on the stage. A log line is
the value as it would be printed, so `console::log(f"score {score}")` is one
item holding `score 3`.

## Menu types

Every dropdown is a type, and its variants are the values Scratch accepts. The
type name is the catalog's menu id in `PascalCase`, with the short names in the
table below; variants are the Scratch value in `PascalCase`, with the special
targets named.

| Menu | raven type | Variants (excerpt) |
| --- | --- | --- |
| `motion_goto`, `motion_glideto` | `Goto` | `RandomPosition`, `MousePointer`, or a sprite name |
| `motion_pointtowards` | `PointTowards` | `MousePointer`, `RandomPosition`, or a sprite name |
| `sensing_touchingobject` | `TouchingObject` | `MousePointer`, `EdgeOfStage`, or a sprite name |
| `sensing_keyoptions` | `Key` | `Space`, `UpArrow`, `A`, `Digit1`, … |
| `looks_costume` | `Costume` | the target's costume names |
| `looks_backdrops` | `Backdrop` | the stage's backdrop names |
| `sound_sounds` | `Sound` | the target's sound names |
| `looks_effect` | `Effect` | `Color`, `Fisheye`, `Whirl`, `Pixelate`, `Mosaic`, `Brightness`, `Ghost` |
| `rotation_style` | `RotationStyle` | `AllAround`, `LeftRight`, `DontRotate` |
| `front_back` | `FrontBack` | `Front`, `Back` |
| `number_name` | `NumberName` | `Number`, `Name` |
| `stop_option` | `StopOption` | `All`, `ThisScript`, `OtherScriptsInSprite`, `OtherScriptsInStage` |
| `drag_mode` | `DragMode` | `Draggable`, `NotDraggable` |
| `math_op` | `MathOp` | `Abs`, `Floor`, `Ceiling`, `Sqrt`, `Sin`, … |
| `greater_than` | `GreaterThan` | `Timer`, `Loudness` |
| `sensing_of_object` | `OfObject` | `Stage`, or a sprite name |

A variant that names a project entity — a costume, a backdrop, a sound, a sprite —
is checked against the project, exactly as raven-asm checks the dropdown. A typo
in a sprite name is a compile error listing the names that do exist.

```rav
motion::goto(Goto::MousePointer);
looks::switch_costume_to(Costume::Idle);
looks::switch_backdrop_to(Backdrop::Sky);
control::stop(StopOption::OtherScriptsInSprite);
sensing::of("x position", OfObject::Stage);
sensing::current(Current::Year);
```

`sensing_of_property` is an **open** menu — Scratch accepts any property name
there, so raven takes a string literal rather than inventing an enum. Every other
menu in the table is closed, and a value outside it is a compile error.

## Coverage: which blocks need raven syntax

Most blocks map one-to-one onto a function. A handful are more useful as core
syntax, and a few are hats. Both are still *covered*; there is no block without a
spelling.

| Catalog block | raven spelling |
| --- | --- |
| `data_variable` | *refused* — `score` is a `_vms` cell, so reading it is a cell read |
| `data_setvariableto` | *refused* — `score = 0;` |
| `data_changevariableby` | *refused* — `score += 1;` |
| `data_showvariable` | *refused* — a cell has no monitor |
| `data_hidevariable` | *refused* — a cell has no monitor |
| `data_itemoflist` | `trail[i]` or `trail.at(i)` (or `data::item_of_list(i, trail)`) |
| `data_replaceitemoflist` | `trail[i] = v;` (or `data::replace_item_of_list(i, trail, v)`) |
| `data_addtolist` | `trail.push(v)` |
| `data_insertatlist` | `trail.insert(i, v)` |
| `data_deleteoflist` | `trail.remove(i)` |
| `data_deletealloflist` | `trail.clear()` |
| `data_lengthoflist` | `trail.len()` |
| `data_listcontainsitem` | `trail.contains(v)` |
| `data_itemnumoflist` | `trail.index_of(v)` |
| `control_repeat` | `repeat n { … }` |
| `control_forever` | `forever { … }` |
| `control_if`, `control_if_else` | `if c { … }`, `if c { … } else { … }` |
| `control_repeat_until` | `repeat_until c { … }` |
| `event_whenflagclicked` … | `on flag_clicked { … }` and the other hats |
| `operator_*` arithmetic and comparison | `+ - * / % == != < <= > >= && || !` |
| `argument_reporter_*` | `proc` parameters, emitted from the declaration |

Everything else is a function. A name-taking form takes the **name** because the
block itself does — `on greater_than(score, 5) { … }` — and the checker resolves it
against the target's declarations.

`control::for_each`, `control::while`, `control::all_at_once` and the counter
blocks (`control::counter`, `control::incr_counter`, `control::clear_counter`) are
in the catalog and are reachable, but they are **TurboWarp-only**: vanilla Scratch
has no such blocks, so `raven check` reports them as warnings and `--strict`
refuses them. The prelude's `for` and a list's methods are the portable way to
count.

### The five refused blocks

`data_variable`, `data_setvariableto`, `data_changevariableby`,
`data_showvariable` and `data_hidevariable` are in the catalog and in raven's
binding table, and calling one is an error that says why:

```text
error: `data::set_variable_to` is not available in raven
  = note: variables are VMS cells; write `n = value;`
  = note: raw Scratch variables and lists are the layer raven compiles *to*,
          not the layer a program writes
```

They are the name-at-run-time interface, and raven's whole claim is that a
program cannot reach it: a raven project declares no Scratch variable a program can
name, and the only one it ever declares is the mirror `watch` asks for. See
[the memory system](/raven/design#_7-there-is-no-raw-variable-access-only-the-virtual-memory-system).

## Prelude

`std::prelude` is a separate, much smaller thing: the macros that make the core
grammar comfortable. It is a normal raven module —
`crates/raven/src/prelude.rav` — imported into every file, and every item in it
can be read or redefined.

| Item | What it does |
| --- | --- |
| `while c { … }` | `repeat_until` on the negated condition |
| `for i in a..b { … }` | a `_stackN` cell for the counter, then `repeat_until` |

`num(x)` and `str(x)` are not prelude items either: they are core syntax, because
a keyword is what tells the parser a conversion is meant. See
[conversions](/raven/types#conversions).

Lists and maps are not functions in the standard library: they are **methods** on
the value, because the receiver's type is what decides which block a call is.
Each method is one or the other of two things — a **value**, which can be used
wherever an expression goes, or a **statement**, which is used for its effect and
has no result:

| type | methods that are values | methods that are statements |
| --- | --- | --- |
| `list<T>` | `len`, `is_empty`, `at`, `first`, `last`, `text`, `contains`, `index_of` | `push`, `insert`, `remove`, `pop`, `clear` |
| `map<K, V>` | `len`, `is_empty`, `get`, `has` | `set`, `remove`, `clear` |

That split is enforced rather than implied: `trail.pop()` is a statement, so
`let v = trail.pop();` is an error, and the diagnostic lists only the methods
that produce a value. Reading the top of a stack is therefore `let v =
trail.last(); trail.pop();` — two blocks, both visible in `raven expand`.

`text()` is worth calling out: it is `data_listcontents`, so the whole list as
one string costs **one block**, not a loop. `data::contents_of_list(l)` is the
same block by its library name.

Three of a list's methods answer a question rather than a value — `contains`,
`is_empty` and `has` — and the answer is a boolean block, which can be a
condition, an argument to a `bool` parameter, or a value kept in a cell and read
back as `<cell = "true">`. See
[booleans](/raven/types#booleans-stored-as-a-value-converted-on-the-way-out).

`match`, `f"…"`, compound assignment and `return` are keywords rather than
macros, because their shape is repetition or depends on where a name lives; their
lowerings are one or two lines each and are on
[the cost table](/raven/lowering#the-cost-table).

## Extensions

Pen and Music are ordinary modules: `pen` and `music`. Using any of their blocks
enables the extension in the generated project, exactly as raven-asm does, so a
project never declares an extension it does not use.

```rav
use std::pen;

on flag_clicked {
    pen::clear();
    pen::pen_down();
    forever {
        pen::set_pen_color_to_color("#ff8800");
        motion::move_steps(1);
    }
}
```

## Why the std is generated and not written

A hand-written standard library would drift from the catalog the first time
Scratch gained a block, and the drift would be invisible until someone needed the
block. Generating the binding, and testing that the binding is total, makes the
drift impossible: the test fails, names the missing block, and points at the row
that has to be added.
