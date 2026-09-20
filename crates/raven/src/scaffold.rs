//! `raven new` and `raven init`: writing a project that builds.
//!
//! The project is a hello world, and nothing more: a stage with a backdrop, a
//! sprite wearing the logo, and one script that says hello. Everything else the
//! language can do is in the documentation, out of the way of the first thing a
//! new project has to do — compile.

use std::path::{Path, PathBuf};

use raven_scratch::diag::{Error, Result};

use crate::identity;

/// The logo, exactly as the documentation ships it.
///
/// This is the real file, not a copy: if the artwork changes, the costume
/// changes with it. The *size* is not the documentation's, though — see
/// [`logo_costume_svg`].
const LOGO_SVG: &str = include_str!("../../../docs/public/logo.svg");

/// The size the documentation canvas declares.
const LOGO_CANVAS: u32 = 512;

/// The size the costume is written at.
///
/// A sprite's size on the stage comes from its costume's intrinsic width and
/// height, and the documentation canvas is 512 units — wider than the whole
/// 480-unit stage, so the sprite starts life too big to see. 64 units is a
/// comfortable size for a logo sitting in the middle of the stage.
const COSTUME_SIZE: u32 = 64;

/// A plain backdrop, so a new project is never missing an asset.
///
/// It is plain white on purpose: a project's own artwork is the thing worth
/// looking at, and a coloured default fights every costume put on top of it. The
/// rectangle also gives the costume an intrinsic 480×360 size, which is Scratch's
/// stage.
const BACKDROP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360" viewBox="0 0 480 360">
  <rect width="480" height="360" fill="#ffffff"/>
</svg>
"##;

/// The logo at costume size.
///
/// One attribute decides a costume's intrinsic size, so one attribute is
/// substituted and nothing else about the file is touched.
fn logo_costume_svg() -> String {
    let from = format!("width=\"{LOGO_CANVAS}\" height=\"{LOGO_CANVAS}\"");
    let to = format!("width=\"{COSTUME_SIZE}\" height=\"{COSTUME_SIZE}\"");
    assert!(
        LOGO_SVG.contains(&from),
        "docs/public/logo.svg no longer declares {from}; the costume cannot be sized"
    );
    LOGO_SVG.replacen(&from, &to, 1)
}

/// A project's files, as (path relative to the project root, contents).
#[must_use]
pub fn default_project(name: &str, with_module: bool) -> Vec<(PathBuf, String)> {
    let mut files = vec![
        (PathBuf::from(identity::MANIFEST), manifest(name)),
        (PathBuf::from(".gitignore"), gitignore()),
        (PathBuf::from("README.md"), readme(name)),
        (PathBuf::from("src/stage.rav"), stage()),
        (
            PathBuf::from("src/sprites/sprite1.rav"),
            sprite1(with_module),
        ),
        (PathBuf::from("assets/logo.svg"), logo_costume_svg()),
        (
            PathBuf::from("assets/backdrop1.svg"),
            BACKDROP_SVG.to_string(),
        ),
    ];
    if with_module {
        files.push((PathBuf::from("src/lib/greeting.rav"), module()));
    }
    files
}

/// Write `files` into `root`, refusing to clobber unless `force`.
pub fn write(root: &Path, files: &[(PathBuf, String)], force: bool) -> Result<()> {
    if root.exists() && !force {
        let existing = std::fs::read_dir(root)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
        if existing {
            return Err(Error::msg(format!(
                "`{}` already exists and is not empty",
                root.display()
            ))
            .note("pass `--force` to write into it anyway"));
        }
    }
    std::fs::create_dir_all(root)?;
    for (path, contents) in files {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if full.exists() && !force {
            return Err(Error::msg(format!("`{}` already exists", full.display()))
                .note("pass `--force` to overwrite it"));
        }
        std::fs::write(&full, contents)?;
    }
    Ok(())
}

fn manifest(name: &str) -> String {
    format!(
        r#"# raven.toml — the only file raven looks for by name.
#
# See the guide: https://raven-scratch.github.io/raven/guide/getting-started

[project]
name = "{name}"
output = "dist"        # a directory, or an explicit `something.sb3`
extensions = []        # "pen" and/or "music"

[targets]
stage = "src/stage.rav"
sprites = ["src/sprites/sprite1.rav"]
"#
    )
}

fn gitignore() -> String {
    "dist/\n".to_string()
}

fn readme(name: &str) -> String {
    format!(
        r#"# {name}

A raven project: a stage, a sprite, and one script that says hello.

```sh
raven check           # parse, resolve, type check and expand; write nothing
raven build           # compile to dist/{name}.sb3, and nothing else
raven build --debug   # also dist/asm/ (the raven-asm) and dist/project.json
raven expand          # print the raven-asm this lowers to
```

Open `dist/{name}.sb3` in the Scratch editor or TurboWarp and click the green
flag. The guide is at https://raven-scratch.github.io/raven/raven/.

There is no Scratch variable in this project, and no way to make one: a `var` or
a `let` is a cell of the target's `_vms` list, addressed by a constant index that
`raven expand` prints.
"#
    )
}

fn stage() -> String {
    r#"// The stage: the backdrop, and anything that belongs to the whole project.
//
// There are no Scratch variables in a raven project. A `var` declared here is
// one cell of `_gvm`, the arena the stage declares, and every sprite reads the
// same cell; a `var` in a sprite is a cell of that sprite's own `_vms`.

stage {
    costume "backdrop1" = "assets/backdrop1.svg";
}
"#
    .to_string()
}

fn sprite1(with_module: bool) -> String {
    if with_module {
        r#"// One file per sprite. Everything declared here belongs to this sprite.

use lib::greeting::{greet, GREETING};

sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    on flag_clicked {
        greet(GREETING);
    }
}
"#
        .to_string()
    } else {
        r#"// One file per sprite. Everything declared here belongs to this sprite.
//
// `let` is a cell in the target's `_vms` list: one `data_itemoflist` per read,
// one `data_replaceitemoflist` per write, and no Scratch variable behind it.

sprite "Sprite1" {
    costume "logo" = "assets/logo.svg";

    on flag_clicked {
        let greeting = "Hello, world!";
        looks::say_for_secs(greeting, 2);
    }
}
"#
        .to_string()
    }
}

fn module() -> String {
    r#"// A module: no `stage`, no `sprite`, just items other files can use.
//
// `pub` is what makes an item visible to a file that `use`s this one.

/// What the project says.
pub const GREETING: str = "Hello, world!";

/// Say `message`, pause, and say it again.
pub proc greet(message: str) {
    looks::say_for_secs(message, 1);
    control::wait(0.5);
    looks::say_for_secs(message, 1);
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slash(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    fn contents<'a>(files: &'a [(PathBuf, String)], path: &str) -> &'a str {
        files
            .iter()
            .find(|(p, _)| slash(p) == path)
            .map(|(_, text)| text.as_str())
            .unwrap_or_else(|| panic!("`{path}` is missing from the scaffold"))
    }

    #[test]
    fn the_scaffold_has_the_files_a_project_needs() {
        let files = default_project("hello", false);
        let paths: Vec<String> = files.iter().map(|(p, _)| slash(p)).collect();
        assert!(paths.contains(&"raven.toml".to_string()));
        assert!(paths.contains(&"src/stage.rav".to_string()));
        assert!(paths.contains(&"src/sprites/sprite1.rav".to_string()));
        assert!(paths.contains(&"assets/logo.svg".to_string()));
        assert!(!paths.iter().any(|p| p.contains("lib/")));
    }

    #[test]
    fn the_default_project_says_hello() {
        let files = default_project("hello", false);
        let sprite = contents(&files, "src/sprites/sprite1.rav");
        assert!(sprite.contains("on flag_clicked"));
        assert!(sprite.contains("looks::say_for_secs(greeting, 2);"));
        assert!(sprite.contains("let greeting = \"Hello, world!\";"));
        // A hello world is a hat, one cell and one block.
        assert_eq!(sprite.matches('{').count(), 2, "{sprite}");
    }

    #[test]
    fn the_costume_is_written_at_a_size_that_fits_the_stage() {
        let files = default_project("hello", false);
        let logo = contents(&files, "assets/logo.svg");
        assert!(logo.contains("width=\"64\" height=\"64\""), "{logo}");
        assert!(
            !logo.contains("width=\"512\""),
            "the 512 canvas leaked through"
        );
    }

    #[test]
    fn the_costume_is_the_real_logo_otherwise() {
        let files = default_project("hello", false);
        let logo = contents(&files, "assets/logo.svg");
        // Same artwork, same viewBox, one attribute pair different: `512` is
        // three characters and `64` is two, and it appears twice.
        assert!(logo.contains("viewBox=\"48 48 416 416\""));
        assert_eq!(LOGO_SVG.len() - logo.len(), 2);
        assert_eq!(
            logo.matches("width=").count(),
            LOGO_SVG.matches("width=").count()
        );
    }

    #[test]
    fn the_scaffold_can_include_a_module() {
        let files = default_project("hello", true);
        let sprite = contents(&files, "src/sprites/sprite1.rav");
        assert!(sprite.contains("use lib::greeting::{greet, GREETING};"));
        assert!(sprite.contains("greet(GREETING);"));
        let module = contents(&files, "src/lib/greeting.rav");
        assert!(module.contains("pub proc greet(message: str)"));
        assert!(module.contains("pub const GREETING"));
    }

    #[test]
    fn the_manifest_names_the_project() {
        assert!(manifest("hello").contains("name = \"hello\""));
    }
}
