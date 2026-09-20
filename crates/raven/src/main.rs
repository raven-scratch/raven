//! The `raven` binary: parse arguments, run a command, report failures the way a
//! compiler does.

use clap::Parser;
use raven::cli::{self, Cli};

fn main() {
    let cli = Cli::parse();
    if let Err(error) = cli::run(cli) {
        eprint!("{}", error.render());
        std::process::exit(1);
    }
}
