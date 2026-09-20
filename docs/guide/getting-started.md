# Getting started

The repository is a Cargo workspace with three crates. `raven-asm` is the
assembly-level compiler; `raven` is the high-level language on top of it. Both
work today — the difference is how much you want to write by hand.

## Install

```sh
git clone https://github.com/raven-scratch/raven
cd raven
cargo install --path crates/raven-asm
cargo install --path crates/raven
```

That puts `raven-asm` and `raven` on your `PATH`. During development you can also
run either straight from the checkout with `cargo run -p raven-asm -- <command>`
or `cargo run -p raven -- <command>`.

## Create a project

```sh
raven-asm new hello
cd hello
```

`raven-asm new` writes a complete, working project. It has no code in it yet:

```
hello/
├── raven-asm.toml             the project manifest
├── .gitignore                 ignores dist/
├── README.md
├── src/
│   ├── stage.rasm             the stage: backdrops, globals, broadcasts
│   └── sprites/
│       └── sprite1.rasm       one file per sprite
└── assets/
    ├── backdrop1.svg
    └── logo.svg               the raven-asm logo, worn by the sprite
```

## Build it

```sh
raven-asm build
```

```
    Finished dist/hello.sb3 (3404 bytes)
             2 target(s), 0 block(s), 2 asset(s), 0 monitor(s)
```

Open `dist/hello.sb3` in the [Scratch editor](https://scratch.mit.edu/projects/editor/)
(File ▸ Load from your computer), in TurboWarp, or in any other compatible
player. There is a stage with a backdrop, and a sprite wearing the logo.

Useful variations:

```sh
raven-asm check                      # parse and validate, write nothing
raven-asm build --debug              # also write dist/project.json to read
raven-asm clean                      # delete dist/
```

## Your first edit

Open `src/sprites/sprite1.rasm`. It declares one thing and nothing else:

```rasm
sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";
}
```

Add a script inside the braces. This one is a hat and a `say for seconds`, which
is two Scratch blocks and nothing else:

<div class="cmp">
<div class="cmp-col">
<h4>raven-asm</h4>

```rasm
sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    event_whenflagclicked {
        looks_sayforsecs("Hello, world!", 2);
    }
}
```

</div>
<div class="cmp-col">
<h4>Scratch</h4>

<pre class="blocks" v-pre>
when green flag clicked
say [Hello, world!] for (2) seconds
</pre>

</div>
</div>

Four things to notice:

1. Every statement ends with `;` and names a Scratch block by its **opcode**.
   `event_whenflagclicked` is the "when green flag clicked" hat, and
   `looks_sayforsecs` is "say … for … seconds".
2. Arguments are positional and stay in the order the Scratch block reads them:
   the message first, then the seconds.
3. Strings, numbers and `true`/`false` are written literally. Numbers keep the
   spelling you give them, so `2`, `2.0` and `2.00` all stay as written.
4. There is no `fn`, no `let`, no `if`, no `+`. Every line is one block. See
   [Why no syntax sugar](/raven-asm/design) for what that buys you, and
   [Syntax](/raven-asm/syntax) for the full grammar.

Try changing the message and the duration, then rebuild:

```rasm
looks_sayforsecs("Hello, Scratch!", 3);
```

Run `raven-asm check` first if you want to catch typos without writing a file.

## Making it move

Two more blocks give the sprite something to do. Add them inside the hat, and
give the sprite a starting position and direction:

<div class="cmp">
<div class="cmp-col">
<h4>raven-asm</h4>

```rasm
sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    event_whenflagclicked {
        motion_gotoxy(-100, 0);
        motion_pointindirection(90);
        control_repeat(10) {
            motion_movesteps(20);
            looks_sayforsecs("Wheee!", 0.2);
        }
    }
}
```

</div>
<div class="cmp-col">
<h4>Scratch</h4>

<pre class="blocks" v-pre>
when green flag clicked
go to x: (-100) y: (0)
point in direction (90)
repeat (10)
    move (20) steps
    say [Wheee!] for (0.2) seconds
</pre>

</div>
</div>

Note the two shapes of statement: `motion_gotoxy(-100, 0);` ends in a semicolon,
while `control_repeat(10) { … }` takes a body in braces because the Scratch block
has a `SUBSTACK` input. `repeat`, `forever`, `if` and `if … else` are the blocks
that do; nothing else is.

## The same project in raven

Once writing `control_if(operator_gt(motion_xposition(), 100)) { … }` gets tiring,
the other half of the repository exists. `raven new` writes a project that says
hello, which is the smallest thing that proves the whole chain works:

```rav
// src/sprites/sprite1.rav
sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    on flag_clicked {
        let greeting = "Hello, world!";
        looks::say_for_secs(greeting, 2);
    }
}
```

```sh
raven new hello    # a project in raven
cd hello
raven check
raven build        # dist/hello.sb3
raven expand       # the raven-asm it became
```

And `raven new hello --with-module` puts the greeting in a module, so the two
files show how `use` and `pub` work:

```rav
// src/lib/greeting.rav
pub const GREETING: str = "Hello, world!";

pub proc greet(message: str) {
    looks::say_for_secs(message, 1);
    control::wait(0.5);
    looks::say_for_secs(message, 1);
}
```

```rav
// src/sprites/sprite1.rav
use lib::greeting::{greet, GREETING};

sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    on flag_clicked {
        greet(GREETING);
    }
}
```

`raven expand` is the part worth looking at first: it prints exactly the
raven-asm those programs lower to, so you never have to take the sugar on faith.

Everything else the language can do — the standard library, lists and maps,
structs, `watch`, the console, `warp` — is described in the rest of this guide,
and `raven expand` prints the cost of any construct you write. For a language
reference written for a machine reader, run `raven explain`; see
[For LLMs](/guide/for-llms).

## Where to go next

* [Project structure](/raven-asm/project-structure) — what each file is for.
* [Syntax](/raven-asm/syntax) — the complete grammar on one page.
* [Blocks and Scratch](/raven-asm/blocks) — how statements map onto Scratch.
* [What is raven?](/raven/) — the other language, and what it adds.
* [Design laws](/raven/design) — what that language is not allowed to do.
* [From raven to Scratch](/raven/lowering) — the cost of every construct.
* [Block reference](/reference/blocks) — everything you can write.
