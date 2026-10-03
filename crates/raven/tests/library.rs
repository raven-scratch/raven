//! Checks on what ships in `lib/`.
//!
//! A library is data as much as it is code, and `lib/case`'s answer is a table:
//! the costumes it wears, in the order a code counts in. `cs_code` reads the
//! *number* a switch lands on, so the order is the code and a wrong order is a
//! wrong case, silently.

use std::path::{Path, PathBuf};

use raven::ast::Item;
use raven_scratch::diag::Source;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// `cs_none` first, then `cs_A` … `cs_Z`, then `cs_a` … `cs_z`: a code is the
/// distance from `cs_none`, so `A` is 1 and `a` is 27.
#[test]
fn the_case_library_wears_both_cases_of_every_letter() {
    let path = repository().join("lib/case/engine.rav");
    let text = std::fs::read_to_string(&path).expect("lib/case/engine.rav");
    let file =
        raven::parser::parse(&Source::new("lib/case/engine.rav", text)).expect("the module parses");

    let found: Vec<String> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Costume(costume) => Some(costume.name.clone()),
            _ => None,
        })
        .collect();
    let expected: Vec<String> = std::iter::once("cs_none".to_string())
        .chain(
            ('A'..='Z')
                .chain('a'..='z')
                .map(|letter| format!("cs_{letter}")),
        )
        .collect();
    assert_eq!(found, expected);
}

/// The demo copies the library rather than importing it, so the copy *is* the
/// install, and the two have to say the same thing.
#[test]
fn the_case_demo_installs_the_library_it_ships_with() {
    let library =
        std::fs::read_to_string(repository().join("lib/case/engine.rav")).expect("the library");
    let installed =
        std::fs::read_to_string(repository().join("examples/raven/case/src/case/engine.rav"))
            .expect("the demo's copy");
    assert_eq!(
        library, installed,
        "engine.rav differs between lib/case and examples/raven/case"
    );
}
