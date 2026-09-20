//! Which blocks may be evaluated twice.
//!
//! `let` is a substitution and a macro parameter is a substitution, so a value
//! that is mentioned twice appears twice. For most blocks that is free and
//! harmless; for a block that reads the world, the two copies can disagree.
//!
//! The catalog records what a block *is* — its opcode and kind — and this module
//! records the one thing the catalog does not: whether a reporter's answer can
//! change between two evaluations. Everything else follows from the kind, so the
//! table below is the complete list of decisions.

use raven_scratch::catalog::{self, BlockKind};

/// How many times a block's result may be re-evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Purity {
    /// Same inputs, same result, at any time. May be duplicated freely.
    Pure,
    /// Reads the world. Two evaluations may differ, so it may be evaluated once
    /// per source-level mention.
    Sampled,
    /// A command: it changes something, and the shape of the language keeps it
    /// out of expression position entirely.
    Effectful,
}

impl Purity {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Purity::Pure => "pure",
            Purity::Sampled => "sampled",
            Purity::Effectful => "effectful",
        }
    }

    /// Whether a source-level alias of this expression may be used more than once.
    #[must_use]
    pub const fn may_be_duplicated(self) -> bool {
        matches!(self, Purity::Pure)
    }
}

/// The reporters and booleans whose answer depends on when they are asked.
///
/// Everything in the core palette that is not here is a pure function of its
/// inputs: the `operator_*` family, `argument_reporter_*`, and every block that
/// only reads something the project cannot change under it. The list is checked
/// against the catalog by the tests below, so a block added to the catalog has to
/// be classified deliberately rather than by omission.
pub static SAMPLED: &[&str] = &[
    // Motion: the sprite's own state.
    "motion_xposition",
    "motion_yposition",
    "motion_direction",
    // Looks: the costume and size the sprite happens to be wearing.
    "looks_size",
    "looks_costumenumbername",
    "looks_backdropnumbername",
    // Sound.
    "sound_volume",
    // Sensing: the entire category reads the world.
    "sensing_touchingobject",
    "sensing_touchingcolor",
    "sensing_coloristouchingcolor",
    "sensing_distanceto",
    "sensing_answer",
    "sensing_keypressed",
    "sensing_mousedown",
    "sensing_mousex",
    "sensing_mousey",
    "sensing_loudness",
    "sensing_timer",
    "sensing_of",
    "sensing_current",
    "sensing_dayssince2000",
    "sensing_username",
    "sensing_online",
    // Operators: `random` is the one reporter in the category that is not a
    // function of its inputs.
    "operator_random",
    // Data: reading a variable or a list is reading state, not computing.
    "data_variable",
    "data_listcontents",
    "data_itemoflist",
    "data_itemnumoflist",
    "data_lengthoflist",
    "data_listcontainsitem",
    // Control: the extended counter.
    "control_get_counter",
    // Music.
    "music_getTempo",
];

/// The purity of a catalog block.
///
/// An opcode that is not in the catalog is [`Purity::Effectful`]: unknown code is
/// never duplicated.
#[must_use]
pub fn of(opcode: &str) -> Purity {
    let Some(spec) = catalog::block(opcode) else {
        return Purity::Effectful;
    };
    match spec.kind {
        BlockKind::Hat | BlockKind::Stack | BlockKind::Cap => Purity::Effectful,
        BlockKind::Reporter | BlockKind::Boolean => {
            if SAMPLED.contains(&opcode) {
                Purity::Sampled
            } else {
                Purity::Pure
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sampled_opcode_is_a_real_reporter() {
        for opcode in SAMPLED {
            let spec = catalog::block(opcode)
                .unwrap_or_else(|| panic!("`{opcode}` is listed as sampled but is not a block"));
            assert!(
                spec.kind.is_value(),
                "`{opcode}` is listed as sampled but is a {:?}",
                spec.kind
            );
            assert_eq!(of(opcode), Purity::Sampled, "`{opcode}`");
        }
    }

    #[test]
    fn command_blocks_are_effectful_and_operators_are_pure() {
        assert_eq!(of("motion_movesteps"), Purity::Effectful);
        assert_eq!(of("control_if"), Purity::Effectful);
        assert_eq!(of("event_whenflagclicked"), Purity::Effectful);
        assert_eq!(of("operator_add"), Purity::Pure);
        assert_eq!(of("operator_join"), Purity::Pure);
        assert_eq!(of("argument_reporter_string_number"), Purity::Pure);
    }

    #[test]
    fn unknown_opcodes_are_never_duplicated() {
        assert_eq!(of("no_such_block"), Purity::Effectful);
        assert!(!Purity::Sampled.may_be_duplicated());
        assert!(Purity::Pure.may_be_duplicated());
    }

    #[test]
    fn the_sampled_list_has_no_duplicates() {
        let mut seen = SAMPLED.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            SAMPLED.len(),
            "the sampled list names a block twice"
        );
    }
}
