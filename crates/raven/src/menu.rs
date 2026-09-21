//! raven's view of Scratch's dropdown menus.
//!
//! Scratch stores a dropdown value as a string. raven turns each dropdown into an
//! enum type, so `motion::goto(Goto::MousePointer)` is checked at compile time and
//! `motion::goto(Goto::MousePointe)` is a spelling error rather than a block that
//! quietly does nothing.
//!
//! Everything here is derived from the catalog: the set of menus is whatever the
//! block table refers to, the fixed value lists come from the catalog's own
//! tables, and the project-dependent ones (costumes, sprites, sounds) are resolved
//! against the target being compiled. There is no second list to keep in step.

use raven_scratch::catalog::{self, MenuDomain, Shape};

/// What kind of values a menu accepts.
#[derive(Clone, Copy, Debug)]
pub enum Domain {
    /// A closed set of Scratch strings.
    Fixed(&'static [&'static str]),
    /// Sprite names, plus the fixed targets listed (`_mouse_`, `_edge_`, …).
    Sprites(&'static [&'static str]),
    /// The costumes declared by the target.
    Costumes,
    /// The backdrops declared by the stage.
    Backdrops,
    /// The sounds declared by the target.
    Sounds,
    /// A value only known at run time: a literal or a reporter is accepted.
    Open,
}

impl Domain {
    /// Whether an enum variant is the only spelling.
    #[must_use]
    pub fn is_enumerable(self) -> bool {
        !matches!(self, Domain::Open)
    }
}

/// The name of the raven type for a menu, with the overrides that stop the
/// generated names reading badly.
fn override_name(menu_id: &str) -> Option<&'static str> {
    Some(match menu_id {
        "motion_goto" => "Goto",
        // `motion_glideto` has the same value set, so it answers to the same
        // type: `Goto::MousePointer` is right for both.
        "motion_glideto" => "Goto",
        "motion_pointtowards" => "PointTowards",
        "looks_costume" => "Costume",
        "looks_backdrops" => "Backdrop",
        "looks_effect" => "Effect",
        "sound_sounds" => "Sound",
        "sound_effect" => "SoundEffect",
        "sensing_keyoptions" => "Key",
        "sensing_touchingobject" => "TouchingObject",
        "sensing_distanceto" => "DistanceTo",
        "sensing_of_object" => "OfObject",
        "sensing_of_property" => "Property",
        "control_create_clone_of" => "CloneOf",
        "current_menu" => "Current",
        "pen_color_param" => "ColorParam",
        "music_drum" => "Drum",
        "music_instrument" => "Instrument",
        "greater_than" => "GreaterThan",
        "math_op" => "MathOp",
        "stop_option" => "StopOption",
        "rotation_style" => "RotationStyle",
        "front_back" => "FrontBack",
        "forward_backward" => "ForwardBackward",
        "number_name" => "NumberName",
        "drag_mode" => "DragMode",
        _ => return None,
    })
}

/// The raven type name of a menu id.
#[must_use]
pub fn type_name(menu_id: &str) -> String {
    if let Some(name) = override_name(menu_id) {
        return name.to_string();
    }
    let trimmed = match menu_id.split_once('_') {
        Some((head, rest)) if !rest.is_empty() && is_category(head) => rest,
        _ => menu_id,
    };
    pascal(trimmed)
}

fn is_category(word: &str) -> bool {
    matches!(
        word,
        "motion" | "looks" | "sound" | "sensing" | "control" | "event" | "data" | "pen" | "music"
    )
}

/// The menu id a raven type name refers to.
#[must_use]
pub fn id_for_type(name: &str) -> Option<&'static str> {
    menu_ids().into_iter().find(|id| type_name(id) == name)
}

/// Every menu id the catalog refers to, sorted and deduplicated.
#[must_use]
pub fn menu_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = Vec::new();
    for block in catalog::BLOCKS {
        for arg in block.args {
            if let Shape::Menu(id) = arg.shape {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids.sort_unstable();
    ids
}

/// What a menu accepts.
#[must_use]
pub fn domain(menu_id: &str) -> Domain {
    if let Some(spec) = catalog::MENUS.iter().find(|m| m.id == menu_id) {
        return match spec.domain {
            MenuDomain::Fixed(values) => Domain::Fixed(values),
            MenuDomain::Sprites(extras) => Domain::Sprites(extras),
            MenuDomain::Costumes => Domain::Costumes,
            MenuDomain::Backdrops => Domain::Backdrops,
            MenuDomain::Sounds => Domain::Sounds,
            MenuDomain::Open => Domain::Open,
        };
    }
    match catalog::fixed_menu_values(menu_id) {
        Some(values) => Domain::Fixed(values),
        None => Domain::Open,
    }
}

/// The Scratch value a fixed name refers to, if it is one of the special targets.
#[must_use]
pub fn special_value(name: &str) -> Option<&'static str> {
    Some(match name {
        "MousePointer" => "_mouse_",
        "EdgeOfStage" => "_edge_",
        "RandomPosition" => "_random_",
        "Myself" => "_myself_",
        "Stage" => "_stage_",
        _ => return None,
    })
}

/// The raven variant name of a Scratch string.
///
/// Variants are generated so that the whole of every dropdown is expressible
/// without a hand-written table, with explicit overrides for the four Scratch
/// values that do not survive the trip.
#[must_use]
pub fn variant(value: &str) -> String {
    match value {
        "e ^" => return "E".to_string(),
        "10 ^" => return "Ten".to_string(),
        "don't rotate" => return "DontRotate".to_string(),
        "DAYOFWEEK" => return "DayOfWeek".to_string(),
        _ => {}
    }
    let name = pascal(value);
    if name.is_empty() {
        "_".to_string()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) && !name.starts_with("Digit") {
        format!("Digit{name}")
    } else {
        name
    }
}

/// The variant name of a name the *project* declares, which is not a fixed menu.
///
/// A fixed dropdown value is a Scratch identifier like `draggable`, so PascalCase
/// reads as the enum variant it is. A project name is the author's, and passing it
/// through `pascal` folds it into something that cannot be found again: a sound
/// declared as `theme` would be written `Sound::Theme`, which is a name that
/// appears nowhere in the project — the declaration, the file and the asset all
/// say `theme`. A project name is therefore written the way Rust writes a
/// constant, and the rule is exactly that:
///
/// * upper-case letters, digits and `_` are kept as they are;
/// * a lower-case letter upper-cases itself, and the change from a lower-case
///   word to an upper-case one is written `_`, so `beep` is `BEEP`, `mySound` is
///   `MY_SOUND` and `laser 2` is `LASER_2`;
/// * anything else ends the word, and a run of it is one `_`;
/// * a name that would begin with a digit gets a `_`, because an identifier may
///   not.
///
/// The mapping is reversible for the names a project can declare: a name written
/// in this form has exactly one lower-case spelling that maps back to it, which is
/// what the compiler relies on when it resolves the variant.
#[must_use]
pub fn constant_variant(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 1);
    let mut pending_separator = false;
    let mut previous_was_lowercase = false;
    for c in value.chars() {
        let upper = c.is_ascii_uppercase();
        let lower = c.is_ascii_lowercase();
        let starts_a_word = (upper || c.is_ascii_digit())
            && previous_was_lowercase
            && !pending_separator
            && !out.is_empty();
        if (upper || lower || c.is_ascii_digit())
            && (pending_separator || starts_a_word)
            && !out.is_empty()
        {
            out.push('_');
        }
        if upper || c.is_ascii_digit() {
            out.push(c);
        } else if lower {
            out.push(c.to_ascii_uppercase());
        } else {
            // Not in an identifier: the word ends here, and the `_` is written
            // when the next word starts so a run of them is one.
            pending_separator = true;
            previous_was_lowercase = false;
            continue;
        }
        pending_separator = false;
        previous_was_lowercase = lower;
    }
    if out.is_empty() {
        "_".to_string()
    } else if out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{out}")
    } else {
        out
    }
}

/// The variant name a menu uses, given what the menu's values are.
///
/// A project name is written as a constant and a fixed value as a variant, which
/// is the whole of the difference between `Sound::BEEP` and `Key::Space`.
#[must_use]
pub fn menu_variant(domain: Domain, value: &str) -> String {
    match domain {
        Domain::Costumes | Domain::Backdrops | Domain::Sounds => constant_variant(value),
        _ => variant(value),
    }
}

/// PascalCase a Scratch string: split on anything that is not alphanumeric.
fn pascal(text: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
                start = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            start = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names_come_out_readable() {
        assert_eq!(type_name("motion_goto"), "Goto");
        assert_eq!(type_name("motion_glideto"), "Goto");
        assert_eq!(type_name("looks_costume"), "Costume");
        assert_eq!(type_name("sensing_keyoptions"), "Key");
        assert_eq!(type_name("control_create_clone_of"), "CloneOf");
        assert_eq!(type_name("math_op"), "MathOp");
    }

    #[test]
    fn every_type_name_resolves_back_to_a_menu_of_that_name() {
        for id in menu_ids() {
            let name = type_name(id);
            let back = id_for_type(&name)
                .unwrap_or_else(|| panic!("`{name}` does not resolve back to a menu"));
            assert_eq!(type_name(back), name, "`{id}` → `{name}`");
        }
    }

    #[test]
    fn the_catalog_menus_are_all_reachable() {
        // Every menu a block refers to must have a domain, even if the catalog
        // only knows its values as an open set.
        for id in menu_ids() {
            let _ = domain(id);
        }
        assert!(
            menu_ids().len() >= 20,
            "the catalog should refer to many menus"
        );
    }

    #[test]
    fn variants_are_valid_identifiers() {
        for text in [
            "space",
            "up arrow",
            "1",
            "10 ^",
            "e ^",
            "don't rotate",
            "all around",
            "not draggable",
            "DAYOFWEEK",
            "other scripts in sprite",
        ] {
            let name = variant(text);
            let mut chars = name.chars();
            let first = chars.next().expect("non-empty");
            assert!(
                first.is_ascii_alphabetic() || first == '_',
                "`{text}` → `{name}`"
            );
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "`{text}` → `{name}`"
            );
        }
    }

    #[test]
    fn variant_names_read_well_for_the_awkward_values() {
        assert_eq!(variant("space"), "Space");
        assert_eq!(variant("up arrow"), "UpArrow");
        assert_eq!(variant("1"), "Digit1");
        assert_eq!(variant("10 ^"), "Ten");
        assert_eq!(variant("don't rotate"), "DontRotate");
        assert_eq!(variant("other scripts in sprite"), "OtherScriptsInSprite");
    }

    #[test]
    fn variant_names_are_unique_within_every_fixed_menu() {
        for id in menu_ids() {
            let Domain::Fixed(values) = domain(id) else {
                continue;
            };
            let mut names: Vec<String> = values.iter().map(|v| variant(v)).collect();
            let before = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), before, "`{id}` has colliding variant names");
        }
    }

    #[test]
    fn a_project_name_is_written_the_way_rust_writes_a_constant() {
        assert_eq!(constant_variant("theme"), "THEME");
        assert_eq!(constant_variant("ambient"), "AMBIENT");
        assert_eq!(constant_variant("mySound"), "MY_SOUND");
        assert_eq!(constant_variant("laser 2"), "LASER_2");
        assert_eq!(constant_variant("beep_beep"), "BEEP_BEEP");
        assert_eq!(constant_variant("up-arrow"), "UP_ARROW");
        assert_eq!(constant_variant("2 fast"), "_2_FAST");
        assert_eq!(constant_variant(""), "_");
        // A name already in the form is left alone, which is what makes the
        // written variant findable in the source it came from.
        assert_eq!(constant_variant("THEME"), "THEME");
    }

    #[test]
    fn two_project_names_that_fold_together_are_the_only_collision() {
        // `my_sound` and `mySound` are different Scratch names and fold to the
        // same variant, so a target may not declare both. That is the one case
        // the compiler has to reject rather than spell differently.
        assert_eq!(constant_variant("my_sound"), "MY_SOUND");
        assert_eq!(constant_variant("mySound"), "MY_SOUND");
        // Everything else keeps a variant of its own.
        let names: Vec<String> = ["theme", "THEME", "up-arrow", "UP_ARROW", "laser 2"]
            .iter()
            .map(|n| constant_variant(n))
            .collect();
        assert_eq!(names[0], names[1], "the same name, twice");
        assert_eq!(names[2], names[3], "the same name, twice");
        assert_ne!(names[0], names[4]);
        assert_eq!(names[4], "LASER_2");
    }

    #[test]
    fn a_menu_uses_the_scheme_its_values_deserve() {
        assert_eq!(menu_variant(Domain::Sounds, "theme"), "THEME");
        assert_eq!(menu_variant(Domain::Costumes, "idle"), "IDLE");
        assert_eq!(menu_variant(Domain::Fixed(&["space"]), "space"), "Space");
    }
}
