//! raven's view of Scratch's dropdown menus.
//!
//! Scratch stores a dropdown value as a string. raven turns each dropdown into a
//! type, so `motion::goto(Goto::MousePointer)` is checked at compile time and
//! `motion::goto(Goto::MousePointe)` is a spelling error rather than a block that
//! quietly does nothing.
//!
//! A menu is written in one of two ways, and the rule is the same one the rest of
//! the language uses: **a value raven knows is a name, and a value only the
//! project knows is the project's own text.**
//!
//! * A fixed dropdown holds strings Scratch itself defines — `mouse-pointer`,
//!   `up arrow`, `draggable` — so each is an enum variant, `Goto::MousePointer`.
//! * A menu that can name a costume, a backdrop, a sound or a sprite holds a name
//!   the *author* wrote. Inventing a variant for it means spelling the author's
//!   name a second way, and the second spelling is the confusing one: a sound
//!   declared as `theme` has no `Sound::Theme`, and any scheme that builds one is
//!   lossy — `my_sound` and `mySound` have to fold together. So the name is
//!   written as its literal and checked against what the target declares:
//!   `sound::play("theme")`.
//!
//! Everything here is derived from the catalog: the set of menus is whatever the
//! block table refers to, the fixed value lists come from the catalog's own
//! tables, and the project-dependent ones are resolved against the target being
//! compiled. There is no second list to keep in step.

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

    /// Whether a value here is a name the project declares rather than one raven
    /// knows.
    ///
    /// These are the menus written as the literal of the name, because the name
    /// belongs to the author: a costume, a backdrop, a sound, or a sprite.
    #[must_use]
    pub fn declares_names(self) -> bool {
        matches!(
            self,
            Domain::Costumes | Domain::Backdrops | Domain::Sounds | Domain::Sprites(_)
        )
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
}
