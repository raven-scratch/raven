//! The `raven-asm` command line interface.

use crate::compile;
use crate::docs_gen;
use crate::manifest::MANIFEST_NAME;
use crate::scaffold;
use clap::{Parser, Subcommand};
use raven_scratch::diag::{render_diag, Error, Level, Result};
use raven_scratch::zipw::ZipWriter;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = crate::identity::CRATE,
    version,
    about = "Compile raven-asm source into Scratch 3 .sb3 projects",
    long_about = "raven-asm is a small language that maps one statement to one Scratch block. \
                  This tool creates projects, checks them and builds .sb3 files.",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a new project directory with the default example
    New {
        /// Directory to create
        name: String,
        /// Write into the current directory instead of creating a new one
        #[arg(long)]
        here: bool,
        /// Overwrite files if the directory is not empty
        #[arg(long)]
        force: bool,
        /// Also create `src/lib/shapes.rasm`, a module shared with `use`
        #[arg(long)]
        with_module: bool,
    },
    /// Set up a raven-asm project in an existing directory
    Init {
        /// Directory to initialise
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite an existing raven-asm.toml
        #[arg(long)]
        force: bool,
    },
    /// Compile the project into a .sb3 file
    Build {
        /// Path to the manifest
        #[arg(short, long, default_value = MANIFEST_NAME)]
        manifest_path: PathBuf,
        /// Also write dist/project.json for inspection
        #[arg(long)]
        debug: bool,
        /// Reject blocks vanilla Scratch cannot run (TurboWarp-only blocks)
        #[arg(long)]
        strict: bool,
    },
    /// Parse and validate the project without writing anything
    Check {
        /// Path to the manifest
        #[arg(short, long, default_value = MANIFEST_NAME)]
        manifest_path: PathBuf,
        /// Reject blocks vanilla Scratch cannot run (TurboWarp-only blocks)
        #[arg(long)]
        strict: bool,
    },
    /// Remove build output
    Clean {
        /// Path to the manifest
        #[arg(short, long, default_value = MANIFEST_NAME)]
        manifest_path: PathBuf,
    },
    /// Print the block catalog
    Catalog {
        /// Emit the Markdown block reference used by the documentation
        #[arg(long)]
        markdown: bool,
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Only show blocks from categories containing this text
        #[arg(short, long)]
        category: Option<String>,
    },
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New {
            name,
            here,
            force,
            with_module,
        } => new_project(&name, here, force, with_module),
        Command::Init { path, force } => init_project(&path, force),
        Command::Build {
            manifest_path,
            debug,
            strict,
        } => build(&manifest_path, debug, strict),
        Command::Check {
            manifest_path,
            strict,
        } => check(&manifest_path, strict),
        Command::Clean { manifest_path } => clean(&manifest_path),
        Command::Catalog {
            markdown,
            json,
            category,
        } => catalog(markdown, json, category.as_deref()),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The project's name, given what was typed on the command line.
///
/// A name is written into `raven-asm.toml` and used as the output file name, so
/// it is the last path component and not the path a user typed. Both separators
/// are split on, so the answer does not depend on the platform the tool happens
/// to be running on, and a character that would not survive being interpolated
/// into TOML is replaced.
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

fn new_project(name: &str, here: bool, force: bool, with_module: bool) -> Result<()> {
    let dir_name = project_name(name);

    let root = if here {
        std::env::current_dir().map_err(Error::from)?
    } else {
        PathBuf::from(name)
    };

    if !here && root.exists() {
        let empty = std::fs::read_dir(&root)
            .map(|mut r| r.next().is_none())
            .unwrap_or(false);
        if !empty && !force {
            return Err(Error::msg(format!(
                "`{}` already exists and is not empty",
                root.display()
            ))
            .note("pass `--force` to write into it anyway, or pick another name"));
        }
    }

    let project = if with_module {
        scaffold::default_project_with_module(&dir_name)
    } else {
        scaffold::default_project(&dir_name)
    };
    project.write(&root)?;

    println!(
        "     Created {} project `{}`",
        if with_module { "module-enabled" } else { "new" },
        dir_name
    );
    for path in project.paths() {
        println!("       {}", root.join(path).display());
    }
    println!();
    if here {
        println!("   Next: `raven-asm build`");
    } else {
        println!("   Next: cd {} && raven-asm build", root.display());
    }
    Ok(())
}

fn init_project(path: &Path, force: bool) -> Result<()> {
    let dir_name = std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let manifest = path.join(MANIFEST_NAME);
    if manifest.exists() && !force {
        return Err(
            Error::msg(format!("`{}` already exists", manifest.display()))
                .note("pass `--force` to overwrite it"),
        );
    }

    std::fs::create_dir_all(path)?;
    let project = scaffold::default_project(&dir_name);
    project.write(path)?;
    println!(
        "   Initialised raven-asm project `{dir_name}` in {}",
        path.display()
    );
    println!("   Next: `raven-asm check`");
    Ok(())
}

fn build(manifest_path: &Path, debug: bool, strict: bool) -> Result<()> {
    let output = compile::build_with_options(manifest_path, compile::BuildOptions { strict })?;
    report_warnings(&output.warnings);

    std::fs::create_dir_all(&output.output_dir).map_err(|e| {
        Error::msg(format!(
            "cannot create `{}`: {e}",
            output.output_dir.display()
        ))
    })?;

    let json = serde_json::to_vec(&output.project)
        .map_err(|e| Error::msg(format!("cannot serialize project.json: {e}")))?;

    let mut zip = ZipWriter::new();
    zip.add("project.json", json.clone());
    for asset in &output.assets {
        zip.add(asset.filename(), asset.data.clone());
    }
    let archive = zip.finish();

    let sb3_path = output.output_dir.join(&output.output_file);
    std::fs::write(&sb3_path, &archive)
        .map_err(|e| Error::msg(format!("cannot write `{}`: {e}", sb3_path.display())))?;

    if debug {
        let debug_path = output.output_dir.join("project.json");
        let pretty = serde_json::to_vec_pretty(&output.project)
            .map_err(|e| Error::msg(format!("cannot serialize project.json: {e}")))?;
        std::fs::write(&debug_path, pretty)
            .map_err(|e| Error::msg(format!("cannot write `{}`: {e}", debug_path.display())))?;
        println!("      Debug {}", debug_path.display());
    }

    let blocks: usize = output.project.targets.iter().map(|t| t.blocks.len()).sum();
    println!(
        "    Finished {} ({} bytes)",
        sb3_path.display(),
        archive.len()
    );
    println!(
        "             {} target(s), {} block(s), {} asset(s), {} monitor(s){}",
        output.project.targets.len(),
        blocks,
        output.assets.len(),
        output.project.monitors.len(),
        if output.project.extensions.is_empty() {
            String::new()
        } else {
            format!(", extensions: {}", output.project.extensions.join(", "))
        }
    );
    Ok(())
}

fn check(manifest_path: &Path, strict: bool) -> Result<()> {
    let output = compile::build_with_options(manifest_path, compile::BuildOptions { strict })?;
    report_warnings(&output.warnings);
    let blocks: usize = output.project.targets.iter().map(|t| t.blocks.len()).sum();
    println!(
        "    Checked {} target(s), {} block(s), {} asset(s)",
        output.project.targets.len(),
        blocks,
        output.assets.len()
    );
    Ok(())
}

fn clean(manifest_path: &Path) -> Result<()> {
    // Clean only needs the manifest: it must work even when the sources no
    // longer compile, so it deliberately does not run the compiler.
    let root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = crate::manifest::Manifest::load(manifest_path)?;
    let output_dir = manifest.output_dir(&root);
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir)
            .map_err(|e| Error::msg(format!("cannot remove `{}`: {e}", output_dir.display())))?;
        println!("   Removed {}", output_dir.display());
    } else {
        println!(
            "   Nothing to remove ({} does not exist)",
            output_dir.display()
        );
    }
    Ok(())
}

fn catalog(markdown: bool, json: bool, category: Option<&str>) -> Result<()> {
    if json {
        let value = docs_gen::json();
        println!(
            "{}",
            serde_json::to_string_pretty(&value)
                .map_err(|e| Error::msg(format!("cannot serialize catalog: {e}")))?
        );
    } else if markdown {
        print!("{}", docs_gen::markdown());
    } else {
        print!("{}", docs_gen::text(category));
    }
    Ok(())
}

fn report_warnings(warnings: &[raven_scratch::diag::Diag]) {
    for warning in warnings.iter().filter(|d| d.level == Level::Warning) {
        eprint!("{}", render_diag(warning));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_name_is_the_last_path_component() {
        assert_eq!(project_name("hello"), "hello");
        assert_eq!(project_name("out/hello"), "hello");
        assert_eq!(project_name(r"C:\work\hello"), "hello");
        assert_eq!(project_name("../hello/"), "hello");
    }

    #[test]
    fn a_project_name_is_valid_toml() {
        // The name is interpolated into `raven-asm.toml`, so it must not end the
        // string or carry a backslash.
        for typed in ["hello", "out/hello", r"C:\work\hello"] {
            let name = project_name(typed);
            assert!(!name.contains('"'), "{name}");
            assert!(!name.contains('\\'), "{name}");
        }
    }

    #[test]
    fn clean_removes_the_output_without_compiling() {
        let dir = std::env::temp_dir().join(format!(
            "raven-asm-clean-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dist")).expect("create dist");
        std::fs::write(dir.join("dist/c.sb3"), b"stale").expect("write stale archive");
        // The stage source does not exist, so a compiling `clean` would fail.
        std::fs::write(
            dir.join(MANIFEST_NAME),
            "[project]\nname = \"c\"\noutput = \"dist\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .expect("write manifest");

        let manifest_path = dir.join(MANIFEST_NAME);
        run(Cli {
            command: Command::Clean { manifest_path },
        })
        .expect("clean must not compile the project");

        assert!(
            !dir.join("dist").exists(),
            "clean should remove the output directory"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
