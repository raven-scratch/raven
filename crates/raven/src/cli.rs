//! The `raven` command line front end.
//!
//! The commands mirror the stages of the compiler: `check` runs everything and
//! writes nothing, `expand` stops after lowering and prints the raven-asm, and
//! `build` goes all the way to a `.sb3`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use raven_scratch::diag::{render_diag, Result};

use crate::driver::{self, Options};
use crate::identity;
use crate::scaffold;

/// Where the manifest lives when the option is not given.
pub const DEFAULT_MANIFEST: &str = identity::MANIFEST;

#[derive(Parser, Debug)]
#[command(
    name = identity::CRATE,
    version,
    about = "raven: a Rust-inspired language with macros that compiles to raven-asm and on to Scratch 3",
    long_about = "raven compiles .rav sources to raven-asm, and raven-asm compiles to a Scratch 3 \
                  .sb3 project. Every convenience raven adds over raven-asm is a macro, and \
                  `raven expand` prints the raven-asm it became."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a project with a complete working example.
    New {
        /// The project name, and the directory to create.
        name: String,
        /// Write into the current directory instead of a new one.
        #[arg(long)]
        here: bool,
        /// Overwrite an existing directory.
        #[arg(long)]
        force: bool,
        /// Also write a shared module and import it.
        #[arg(long)]
        with_module: bool,
    },

    /// Scaffold into an existing directory.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },

    /// Compile to dist/<name>.sb3.
    Build {
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: PathBuf,
        /// Write the debug output as well: the lowered raven-asm in dist/asm/
        /// and the uncompressed dist/project.json.
        #[arg(long)]
        debug: bool,
    },

    /// Parse, resolve, type check and expand, writing nothing.
    Check {
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: PathBuf,
    },

    /// Print the raven-asm this project lowers to.
    Expand {
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: PathBuf,
    },

    /// Rewrite sources in canonical form.
    Fmt {
        /// Report files that are not formatted, and write nothing.
        #[arg(long)]
        check: bool,
        /// Files or directories to format; the whole project is used when omitted.
        paths: Vec<PathBuf>,
    },

    /// Remove the output directory.
    Clean {
        #[arg(short, long, default_value = DEFAULT_MANIFEST)]
        manifest: PathBuf,
    },

    /// Print the language reference written for a machine reader (an LLM
    /// writing raven code): grammar, costs, memory model, the full standard
    /// library and the rules that reject code.
    Explain {
        /// Section to print: all, or one of the names `raven explain all` lists.
        #[arg(default_value = "all")]
        section: String,
    },
}

/// Run a command, reporting failure the way a compiler does.
pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New {
            name,
            here,
            force,
            with_module,
        } => {
            let project = project_name(&name);
            let root = if here {
                PathBuf::from(".")
            } else {
                PathBuf::from(&name)
            };
            let files = scaffold::default_project(&project, with_module);
            scaffold::write(&root, &files, force)?;
            let shown = if here { ".".to_string() } else { name.clone() };
            println!("    Created {shown}/ with {} file(s)", files.len());
            println!("    Next: raven build -m {shown}/{DEFAULT_MANIFEST}");
            Ok(())
        }

        Command::Init { path, force } => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string();
            let files = scaffold::default_project(&name, false);
            scaffold::write(&path, &files, force)?;
            println!("    Created {} file(s) in {}", files.len(), path.display());
            Ok(())
        }

        Command::Build { manifest, debug } => {
            let mut options = Options::new(manifest);
            options.debug = debug;
            let result = driver::build(&options)?;
            report(&result.warnings);
            let lines: usize = result
                .asm
                .iter()
                .map(|(_, source)| source.lines().count())
                .sum();
            println!(
                "    Finished {}",
                result
                    .artifact
                    .as_ref()
                    .map_or_else(|| "<none>".to_string(), |p| p.display().to_string())
            );
            println!(
                "             {} target file(s), {lines} line(s) of raven-asm",
                result.asm.len()
            );
            if let Some(dir) = &result.asm_dir {
                println!("    Kept the raven-asm in {}", dir.display());
            }
            Ok(())
        }

        Command::Check { manifest } => {
            let warnings = driver::check(&Options::new(manifest))?;
            report(&warnings);
            println!("    Checked");
            Ok(())
        }

        Command::Expand { manifest } => {
            let files = driver::expand(&Options::new(manifest))?;
            for (path, source) in files {
                println!(
                    "// ---- {} ----",
                    path.display().to_string().replace('\\', "/")
                );
                println!("{source}");
            }
            Ok(())
        }

        Command::Fmt { check, paths } => crate::fmt::run(&paths, check),

        Command::Clean { manifest } => {
            let path = driver::manifest_path(&manifest);
            let program = crate::module::load(&path)?;
            let artifact = driver::artifact_of(&program);
            let directory = artifact.parent().unwrap_or(&program.root).to_path_buf();
            if directory.exists() {
                std::fs::remove_dir_all(&directory)?;
                println!("    Removed {}", directory.display());
            } else {
                println!("    Nothing to remove ({})", directory.display());
            }
            Ok(())
        }

        Command::Explain { section } => {
            crate::explain::print(&section).map_err(raven_scratch::diag::Error::msg)
        }
    }
}

fn report(warnings: &[raven_scratch::diag::Diag]) {
    for warning in warnings {
        eprint!("{}", render_diag(warning));
    }
}

/// The project's name, given what was typed on the command line.
///
/// A name is written into `raven.toml`, used as the output file name and put in
/// the README, so it is the last path component and not the path a user typed: a
/// path would be wrong in all three places, and on Windows its backslashes are
/// not valid unescaped TOML. Both separators are split on, so the answer does not
/// depend on the platform the tool happens to be running on.
fn project_name(typed: &str) -> String {
    let last = typed
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty() && *part != "." && *part != "..")
        .unwrap_or("project");
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c == '"' || c == '\\' || c.is_control() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_project_name_is_the_last_path_component() {
        assert_eq!(project_name("hello"), "hello");
        assert_eq!(project_name("out/hello"), "hello");
        assert_eq!(project_name(r"C:\work\hello"), "hello");
        assert_eq!(project_name("../hello/"), "hello");
    }

    #[test]
    fn a_project_name_is_valid_toml() {
        // The name is interpolated into `raven.toml` unquoted, so it must not
        // end the string or carry a backslash.
        for typed in ["hello", "out/hello", r"C:\work\hello"] {
            let name = project_name(typed);
            assert!(!name.contains('"'), "{name}");
            assert!(!name.contains('\\'), "{name}");
        }
    }

    #[test]
    fn the_manifest_default_is_the_identity_constant() {
        let cli = Cli::try_parse_from(["raven", "build"]).unwrap();
        match cli.command {
            Command::Build { manifest, .. } => {
                assert_eq!(manifest, PathBuf::from(identity::MANIFEST));
            }
            other => panic!("expected build, got {other:?}"),
        }
    }

    #[test]
    fn expand_takes_a_manifest() {
        let cli = Cli::try_parse_from(["raven", "expand", "-m", "other.toml"]).unwrap();
        match cli.command {
            Command::Expand { manifest } => assert_eq!(manifest, PathBuf::from("other.toml")),
            other => panic!("expected expand, got {other:?}"),
        }
    }
}
