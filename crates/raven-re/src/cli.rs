//! The `raven-re` command line interface.
//!
//! One command, because a decompiler does one thing: read an `.sb3`, write the
//! raven-asm project that reproduces it. The result is verified before the
//! command succeeds, so a run that exits zero has a project that compiles.

use crate::error::{Error, Result};
use crate::identity;
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = identity::CRATE,
    version,
    about = "Reverse a vanilla Scratch 3 .sb3 project into raven-asm source",
    long_about = "raven-re decompiles a .sb3 file into a raven-asm project. \
                  It reverses vanilla Scratch 3 only: TurboWarp and any other \
                  edit of Scratch are refused, and so is any block, extension \
                  or agent string they add.",
    arg_required_else_help = true
)]
pub struct Cli {
    /// The .sb3 file to reverse
    pub input: PathBuf,
    /// Directory to write the project into
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Write into an output directory that already holds files
    #[arg(long)]
    pub force: bool,
}

pub fn run(cli: Cli) -> Result<()> {
    let bytes = std::fs::read(&cli.input)
        .map_err(|e| Error::msg(format!("cannot read `{}`: {e}", cli.input.display())))?;
    let name = project_name(&cli.input);
    let root = cli.output.clone().unwrap_or_else(|| PathBuf::from(&name));
    guard(&root, cli.force)?;

    let project = crate::decompile(&bytes, &name)?;
    for warning in &project.warnings {
        eprint!("{}", warning.render());
    }
    project.write(&root)?;

    let manifest = root.join(identity::MANIFEST);
    let built = match raven_asm::compile::build(&manifest) {
        Ok(output) => output,
        Err(error) => {
            // The compiler's own report names the generated line that is wrong,
            // which is what makes a bug in raven-re findable.
            eprint!("{}", error.render());
            return Err(
                Error::msg("the project raven-re wrote does not compile").note(format!(
                    "the sources are in `{}`, so the report above can be read there",
                    root.display()
                )),
            );
        }
    };

    println!("   Reversed {} -> {}", cli.input.display(), root.display());
    println!(
        "             {} target(s), {} block(s), {} asset(s), {} monitor(s)",
        project.targets, project.blocks, project.assets, project.monitors
    );
    let rebuilt: usize = built.project.targets.iter().map(|t| t.blocks.len()).sum();
    println!(
        "     Verified {} rebuilds ({rebuilt} block(s))",
        manifest.display()
    );
    println!();
    println!("   Next: cd {} && raven-asm build", root.display());
    Ok(())
}

/// The project name, given the file that was passed on the command line.
///
/// It becomes the raven-asm project name and the default output directory, so it
/// is the file's stem with anything that would not survive a TOML string or a
/// path replaced.
fn project_name(input: &Path) -> String {
    let stem = input
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c == '"' || c == '\\' || c.is_control() || c == '/' {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned
    }
}

/// Refuse to scatter a reversal over a directory that already has files.
fn guard(root: &Path, force: bool) -> Result<()> {
    if force || !root.exists() {
        return Ok(());
    }
    let empty = std::fs::read_dir(root)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    if empty {
        return Ok(());
    }
    Err(Error::msg(format!(
        "`{}` already exists and is not empty",
        root.display()
    ))
    .note("pass `--force` to write into it anyway, or `--output` to pick another directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_name_is_the_file_stem() {
        assert_eq!(project_name(Path::new("out/game.sb3")), "game");
        assert_eq!(project_name(Path::new("my game.sb3")), "my game");
        // A stem that cannot go into a TOML string is repaired.
        assert_eq!(project_name(Path::new("bad\"name.SB3")), "bad_name");
        assert_eq!(project_name(Path::new("")), "project");
    }

    #[test]
    fn a_project_name_keeps_a_windows_path_as_one_name_off_windows() {
        // `Path` splitting is the platform's, so this asserts what `file_stem`
        // does on the host rather than a separator raven-re chose.
        let expected = if cfg!(windows) { "my game" } else { "C:_work_my game" };
        assert_eq!(project_name(Path::new(r"C:\work\my game.sb3")), expected);
    }

    #[test]
    fn an_output_directory_with_files_needs_force() {
        let dir = std::env::temp_dir().join(format!("raven-re-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        assert!(guard(&dir, false).is_ok(), "an empty directory is fine");
        std::fs::write(dir.join("keep.txt"), b"x").expect("write");
        assert!(guard(&dir, false).is_err());
        assert!(guard(&dir, true).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
