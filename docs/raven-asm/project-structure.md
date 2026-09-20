# Project structure

## The manifest

`raven-asm.toml` is the only file raven-asm looks for by name. Everything else is
discovered from it.

```toml
[project]
name = "hello"          # used for the output file name
output = "dist"         # directory (or .sb3 path) to write into; default "dist"
extensions = []         # "pen" and/or "music"

[targets]
stage = "src/stage.rasm"
sprites = ["src/sprites/sprite1.rasm", "src/sprites/sprite2.rasm"]
```

| Key | Required | Meaning |
| --- | --- | --- |
| `project.name` | yes | Project name. Sanitised for the file name; `hello` becomes `hello.sb3`. |
| `project.output` | no | Directory to write into, or an explicit `something.sb3` path. Defaults to `dist`. |
| `project.extensions` | no | Extensions to enable even if no block uses them. raven-asm bundles `pen` and `music`. |
| `targets.stage` | yes | The file that declares `stage { … }`. |
| `targets.sprites` | no | The files that declare `sprite "Name" { … }`. |

Paths in the manifest are relative to the manifest itself.

## Files

A `.rasm` file is one of two things:

**A target file** declares exactly one target and holds its code.

```rasm
// src/sprites/player.rasm
sprite "Player" {
    …
}
```

```rasm
// src/stage.rasm
stage {
    …
}
```

The stage is always named `Stage`; sprites take the name you give them. Every
sprite file must be listed in `targets.sprites`, and the sprite name must be
unique across the project.

**A module file** declares no target. It holds procedures, project-wide state and
broadcasts, and is pulled in by other files with [`use`](/raven-asm/multi-file):

```rasm
// src/lib/shapes.rasm
proc square(size: num) warp {
    control_repeat(4) {
        motion_movesteps(argument_reporter_string_number("size"));
        motion_turnright(90);
    }
}
```

A module that also declares a `var`, a costume or anything else is rejected —
declarations belong to the target that owns them.

## Inside a target

A target body accepts these items, in any order:

| Item | Example | Effect |
| --- | --- | --- |
| Costume | `costume "costume1" = "assets/costume1.svg" center 32 32;` | Adds a costume to the target. |
| Sound | `sound "beep" = "assets/beep.wav";` | Adds a sound to the target. |
| Variable | `var score = 0;` | Declares a variable and its starting value. |
| List | `list trail = [1, 2, 3];` | Declares a list and its starting items. |
| Broadcast | `broadcast "reset";` | Registers a project-wide message. |
| Procedure | `proc hop(height: num) warp { … }` | Defines a custom block. |
| Statement | `event_whenflagclicked { … }` | Emits a block. Top-level statements start a script. |

Order matters in one respect only: the first costume and sound become the
target's costume `#1` and its default. Everything else is emitted in the order
you write it.

## Paths

There are two path rules, and they are both "obvious" once you have seen them:

* Paths in `raven-asm.toml` are relative to `raven-asm.toml`.
* Asset paths in `costume` and `sound` declarations are relative to the
  **project root**, the directory holding `raven-asm.toml`. So `assets/costume1.svg`
  means the same thing in `src/stage.rasm` and `src/sprites/player.rasm`.
* Paths in `use` are relative to the **file that writes them**.

## Build output

```
dist/
├── hello.sb3          the project archive
└── project.json       only with `raven-asm build --debug`
```

`raven-asm new` also writes a `README.md` describing the project layout and a
`.gitignore` holding `dist/`, so the build artifacts are ignored from the first
commit. The `.sb3` is a plain ZIP containing `project.json` and one file per
asset named `<md5>.<format>` — exactly what Scratch itself produces.
