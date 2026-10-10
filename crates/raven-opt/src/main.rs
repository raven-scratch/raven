//! The `raven-opt` command line interface.
//!
//! One job: a raven-asm project in, an optimised raven-asm project out. There is
//! no `--optimize` here because the whole program is the optimiser; there is no
//! `--strict` because this is not a compiler and has nothing to reject; and there
//! is no `--debug` because the output *is* the source.

use clap::Parser;
use raven_asm::manifest::MANIFEST_NAME;
use raven_scratch::diag::Result;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "raven-opt",
    version,
    about = "Optimise a raven-asm project into an optimised raven-asm project",
    long_about = "Reads a raven-asm project, rewrites it to emit fewer Scratch blocks for \
                  the same program, and writes raven-asm source back out. What comes out \
                  builds with `raven-asm build` and means what it meant before.\n\n\
                  Every rewrite is an identity in Scratch's own semantics applied to operands \
                  the compiler already knows; the ones that would need an inference -- \
                  propagating a variable, inlining a procedure, substituting a reporter -- are \
                  not made. raven-asm itself never optimises: run this if you want it."
)]
struct Cli {
    /// Path to the manifest
    #[arg(short, long, default_value = MANIFEST_NAME)]
    manifest_path: PathBuf,

    /// Write the optimised project here, leaving the input alone
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Rewrite the project's own files instead of writing a copy
    #[arg(long, conflicts_with = "output")]
    in_place: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let report = if cli.in_place {
        raven_opt::optimize_in_place(&cli.manifest_path)?
    } else {
        // With no `--output`, write beside the manifest rather than over it: a
        // program whose default is to destroy its input is a program nobody runs
        // twice by accident.
        let output = cli.output.clone().unwrap_or_else(|| {
            let parent = cli
                .manifest_path
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            parent.join("dist").join("optimized")
        });
        raven_opt::optimize_project(&cli.manifest_path, &output)?
    };

    println!("   Optimised {report}");
    if report.changed == 0 {
        println!("             nothing to rewrite: every file was already minimal");
    }
    Ok(())
}
