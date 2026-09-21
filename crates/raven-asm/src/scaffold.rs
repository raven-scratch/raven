//! Project scaffolding for `raven-asm new` and `raven-asm init`.
//!
//! The default project is deliberately tiny: a stage with a backdrop, and a sprite
//! wearing the raven-asm badge and running no script at all. It is meant to be
//! read in full, then given its first block.

use raven_scratch::diag::{Error, Result};
use std::path::{Path, PathBuf};

/// Files that make up a freshly created project, as (relative path, contents).
pub struct Scaffold {
    pub files: Vec<(PathBuf, Vec<u8>)>,
}

impl Scaffold {
    pub fn write(&self, root: &Path) -> Result<()> {
        for (rel, data) in &self.files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    Error::msg(format!("cannot create `{}`: {e}", parent.display()))
                })?;
            }
            std::fs::write(&path, data)
                .map_err(|e| Error::msg(format!("cannot write `{}`: {e}", path.display())))?;
        }
        Ok(())
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|(p, _)| p.clone()).collect()
    }
}

/// The default project `raven-asm new` produces: a stage with a backdrop and a
/// sprite wearing the raven-asm badge, with no blocks yet.
pub fn default_project(name: &str) -> Scaffold {
    let files: Vec<(PathBuf, Vec<u8>)> = vec![
        (
            PathBuf::from(crate::manifest::MANIFEST_NAME),
            manifest(name).into_bytes(),
        ),
        (PathBuf::from(".gitignore"), GITIGNORE.as_bytes().to_vec()),
        (PathBuf::from("README.md"), readme(name).into_bytes()),
        (PathBuf::from("src/stage.rasm"), STAGE.to_vec()),
        (PathBuf::from("src/sprites/sprite1.rasm"), SPRITE.to_vec()),
        (
            PathBuf::from("assets/backdrop1.svg"),
            backdrop_svg().into_bytes(),
        ),
        (
            PathBuf::from("assets/logo.svg"),
            logo_costume_svg().into_bytes(),
        ),
    ];

    Scaffold { files }
}

/// The extra module file added by `raven-asm new --with-module`, showing how `use`
/// shares procedures between targets.
pub fn example_module() -> (PathBuf, Vec<u8>) {
    (PathBuf::from("src/lib/shapes.rasm"), MODULE.to_vec())
}

/// The default project plus an included module and a script that calls into it.
pub fn default_project_with_module(name: &str) -> Scaffold {
    let mut scaffold = default_project(name);
    scaffold.files.push(example_module());
    for (path, data) in scaffold.files.iter_mut() {
        if path == Path::new("src/sprites/sprite1.rasm") {
            *data = sprite_with_module();
        }
    }
    scaffold
}

/// `SPRITE` with a `use` line and a script that calls the included procedure.
fn sprite_with_module() -> Vec<u8> {
    let base = String::from_utf8_lossy(SPRITE);
    let closing = base
        .rfind('}')
        .expect("the sprite source ends with a brace");
    let mut out = String::with_capacity(base.len() + MODULE_CALL_SCRIPT.len() + 64);
    out.push_str("use \"../lib/shapes.rasm\";\n\n");
    out.push_str(&base[..closing]);
    out.push_str(MODULE_CALL_SCRIPT);
    out.push_str(&base[closing..]);
    out.into_bytes()
}

const MODULE_CALL_SCRIPT: &str = r#"
    // `draw_square` comes from src/lib/shapes.rasm, pulled in with `use` above.
    event_whenkeypressed("space") {
        motion_gotoxy(0, 0);
        motion_pointindirection(90);
        draw_square(80);
    }
"#;

fn manifest(name: &str) -> String {
    format!(
        r#"# The raven-asm project manifest.
#
#   raven-asm build     compile this project into dist/{name}.sb3
#   raven-asm check     parse and validate without writing anything
#
# Paths in this file are relative to it.

[project]
name = "{name}"
# Directory the .sb3 is written to. Defaults to "dist".
output = "dist"
# Scratch extensions this project uses. raven-asm bundles "pen" and "music".
extensions = []

[targets]
# Exactly one file declares `stage {{ ... }}`.
stage = "src/stage.rasm"
# Every other file declares `sprite "Name" {{ ... }}`.
sprites = ["src/sprites/sprite1.rasm"]
"#
    )
}

fn readme(name: &str) -> String {
    format!(
        r#"# {name}

A Scratch 3 project written in [raven-asm]({repository}).

## Layout

```
src/stage.rasm              the stage: backdrops, global variables, broadcasts
src/sprites/sprite1.rasm    one file per sprite
assets/                      costumes and sounds, referenced by path
raven-asm.toml                   which files are targets
```

## Build

```sh
raven-asm check           # parse and validate
raven-asm build           # write dist/{name}.sb3
raven-asm build --debug   # also write dist/project.json to inspect
```

Open `dist/{name}.sb3` in the Scratch editor, TurboWarp, or any compatible
player. The project is empty: a stage, and a sprite wearing the raven-asm logo.

## Syntax in one screen

Every raven-asm statement is exactly one Scratch block, and every argument is either
a literal or another block. There is no sugar to learn — the script below really is
one hat block and one `say for seconds` block:

```rasm
sprite "Sprite1" {{
    costume "logo" = "assets/logo.svg";

    event_whenflagclicked {{
        looks_sayforsecs("Hello, world!", 2);
    }}
}}
```

To read a variable you use the variable reporter block, `data_variable("score")`.
To write one you use `data_setvariableto("score", 0)`. A procedure parameter
always carries its type:

```rasm
proc hop(height: num, once: bool) warp {{
    control_if(argument_reporter_boolean("once")) {{
        motion_changeyby(argument_reporter_string_number("height"));
    }};
}}
```

Run `raven-asm catalog` for the full block list, or read the guide at
{docs}.
"#,
        name = name,
        repository = crate::identity::REPOSITORY,
        docs = crate::identity::DOCS,
    )
}

const GITIGNORE: &str = "/dist\n";

const STAGE: &[u8] = br#"// The stage file: backdrops, global variables, lists and broadcasts.
//
// The stage is a target like any other. Anything declared here is visible to
// every sprite, so globals belong on the stage. For example:
//
//     var best = 0;
//     list log = [];
//     broadcast "reset";
//
// Run `raven-asm catalog` to see every block raven-asm understands.

stage {
    costume "backdrop1" = "assets/backdrop1.svg";
}
"#;

const SPRITE: &[u8] = br#"// One file per sprite. Everything inside the braces belongs to Sprite1.
//
// This project is deliberately empty: the sprite wears the raven-asm logo and does
// nothing yet. A first script looks like this:
//
//     event_whenflagclicked {
//         looks_sayforsecs("Hello, world!", 2);
//     }
//
// A variable declared here is private to this sprite. `global var` puts it on
// the stage instead, where every sprite can see it. `raven-asm catalog` lists
// every block you can write, and the guide at the project's documentation site
// describes the whole language.

sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";
}
"#;

const MODULE: &[u8] =
    br#"// An included module. Modules may only contain `proc` definitions; they are
// copied into every target that uses them, which is the only way Scratch can
// share a custom block between sprites.
//
// Reach it from a sprite with:  use "../lib/shapes.rasm";

proc draw_square(size: num) warp {
    pen_clear;
    pen_penDown;
    control_repeat(4) {
        motion_movesteps(argument_reporter_string_number("size"));
        motion_turnright(90);
    }
    pen_penUp;
}
"#;

fn backdrop_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="480" height="360" viewBox="0 0 480 360">
  <rect width="480" height="360" fill="#ffffff"/>
</svg>
"##
    .to_string()
}

/// The raven-asm badge, exactly as the documentation ships it.
///
/// This is the real logo file, not a copy: if the artwork changes, the costume
/// changes with it. The wordmark is vector outlines rather than a `<text>`
/// element because Scratch's SVG loader deletes the `x`/`y` attributes of every
/// text element and re-anchors it, which would move the letters.
const LOGO_SVG: &str = include_str!("../../../docs/public/logo.svg");

/// The costume: the logo verbatim, with only its canvas size rewritten.
///
/// A sprite's size on the stage comes from its costume's intrinsic width and
/// height, and the documentation canvas is 512 units — wider than the 480-unit
/// stage. One attribute decides that, so one attribute is substituted; nothing
/// else about the file is touched.
const LOGO_CANVAS: usize = 128;

fn logo_costume_svg() -> String {
    let from = format!("width=\"{LOGO_W}\" height=\"{LOGO_W}\"");
    let to = format!("width=\"{LOGO_CANVAS}\" height=\"{LOGO_CANVAS}\"");
    assert!(
        LOGO_SVG.contains(&from),
        "docs/public/logo.svg no longer declares {from}; the costume cannot be sized"
    );
    LOGO_SVG.replacen(&from, &to, 1)
}

/// The documentation canvas size, which the costume overrides.
const LOGO_W: u32 = 512;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_is_complete() {
        let scaffold = default_project("demo");
        let paths = scaffold.paths();
        for expected in [
            "raven-asm.toml",
            "src/stage.rasm",
            "src/sprites/sprite1.rasm",
            "assets/backdrop1.svg",
            "assets/logo.svg",
            ".gitignore",
            "README.md",
        ] {
            assert!(
                paths.iter().any(|p| p == Path::new(expected)),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn the_logo_costume_is_the_real_logo() {
        let costume = logo_costume_svg();
        assert!(costume.starts_with("<svg"));
        // Cosmetic, and `core.autocrlf` is not this test's business.
        assert!(costume.trim_end().ends_with("</svg>"));
        // Resized for the stage...
        assert!(costume.contains("width=\"128\" height=\"128\""));
        assert!(!costume.contains("width=\"512\""));
        // ...and otherwise identical to the documented logo, wordmark included.
        assert!(
            costume.contains(&wordmark_of(LOGO_SVG)),
            "the wordmark is missing"
        );
        assert!(
            costume.matches("<path ").count() >= 8,
            "bands or wordmark missing"
        );
        assert!(
            !costume.contains("<text"),
            "text would be re-anchored by Scratch"
        );
    }

    /// The `d` attribute of the wordmark path: the last path in the file.
    fn wordmark_of(svg: &str) -> String {
        let start = svg.rfind("<path d=\"").expect("a wordmark path") + "<path d=\"".len();
        let rest = &svg[start..];
        let end = rest.find('"').expect("a closed attribute");
        rest[..end].to_string()
    }

    #[test]
    fn the_default_project_is_empty() {
        let source = String::from_utf8_lossy(SPRITE);
        assert!(source.contains("costume \"logo\" = \"assets/logo.svg\";"));

        // Ignore the commented example: what matters is that there is no code.
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!code.contains("event_whenflagclicked"));
        assert!(!code.contains("proc "));
        assert!(!code.contains("var "));
        // The only declaration is the costume; every other line is a brace.
        for line in code.lines().map(str::trim).filter(|l| !l.is_empty()) {
            assert!(
                !line.ends_with(';') || line.starts_with("costume"),
                "the default sprite should declare nothing but its costume: {line}"
            );
        }
    }
}
