//! The binding between the Scratch block catalog and raven's standard library.
//!
//! Every one of the catalog's blocks is reachable from raven, and this module is
//! where that is true or false. The tests at the bottom are what keep it true: if
//! the catalog gains a block and this table does not gain a row, `cargo test`
//! fails and names the block.
//!
//! Three things live here that the catalog does not say, because Scratch does not
//! need them and raven does:
//!
//! * **the result type** of a reporter, since Scratch's values are untyped;
//! * **the shorthand**, when the language has a nicer spelling than the function;
//! * **whether the block takes a body** (`control_while` and friends are written
//!   as `control::while(c) { … }`).
//!
//! The mapping is a curated table rather than a derivation on purpose.
//! `looks_sayforsecs` does not split itself into `say_for_secs`, and a heuristic
//! that guessed would be worse than a row that had to be written.

/// The type a value block produces.
///
/// Two of these are not fixed: `data::value_of` produces whatever the named
/// variable holds, and `data::item_of_list` produces the element type of the
/// named list. The checker resolves both from the declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Num,
    Str,
    Bool,
    OfVariable,
    ListElement,
}

impl Value {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Value::Num => "num",
            Value::Str => "str",
            Value::Bool => "bool",
            Value::OfVariable => "the variable's type",
            Value::ListElement => "the list's element type",
        }
    }
}

/// How raven spells one catalog block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// A callable: `module::name(args)`. `body` marks a block that takes a
    /// `{ … }` substack, and is therefore only valid as a statement.
    Function {
        module: &'static str,
        name: &'static str,
        /// The spelling the documentation prefers, when there is one.
        shorthand: Option<&'static str>,
        /// `None` for a command block.
        result: Option<Value>,
        body: bool,
    },
    /// Covered by syntax and not callable: a keyword, or a declaration.
    Syntax(&'static str),
    /// A hat block, written as an `on <name> { … }` script.
    Hat(&'static str),
    /// A block raven deliberately does not expose, with the reason. The catalog
    /// row exists so the "every block is bound" law still holds: the block is
    /// *reached* by the language, and what the language says about it is no.
    Forbidden {
        module: &'static str,
        name: &'static str,
        why: &'static str,
    },
}

impl Binding {
    /// A short description, for diagnostics and generated documentation.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Binding::Function { module, name, .. } => format!("{module}::{name}"),
            Binding::Syntax(spelling) => (*spelling).to_string(),
            Binding::Hat(name) => format!("on {name} {{ … }}"),
            Binding::Forbidden { module, name, .. } => format!("{module}::{name}"),
        }
    }

    /// The reason a block is refused, when it is.
    #[must_use]
    pub const fn forbidden(&self) -> Option<&'static str> {
        match self {
            Binding::Forbidden { why, .. } => Some(why),
            _ => None,
        }
    }

    /// The preferred spelling: the shorthand when there is one, the call
    /// otherwise.
    #[must_use]
    pub fn spelling(&self) -> String {
        match self {
            Binding::Function {
                shorthand: Some(text),
                ..
            } => (*text).to_string(),
            other => other.describe(),
        }
    }

    /// The function name, when the binding is callable.
    #[must_use]
    pub fn function(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Binding::Function { module, name, .. } => Some((module, name)),
            Binding::Forbidden { module, name, .. } => Some((module, name)),
            _ => None,
        }
    }

    /// The type a call to this binding produces.
    #[must_use]
    pub fn result(&self) -> Option<Value> {
        match self {
            Binding::Function { result, .. } => *result,
            _ => None,
        }
    }

    /// Whether a call must be a statement with a `{ … }` body.
    #[must_use]
    pub fn takes_body(&self) -> bool {
        matches!(self, Binding::Function { body: true, .. })
    }
}

/// One row: a catalog opcode and the raven spelling it has.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub opcode: &'static str,
    pub binding: Binding,
}

const fn cmd(module: &'static str, name: &'static str) -> Binding {
    Binding::Function {
        module,
        name,
        shorthand: None,
        result: None,
        body: false,
    }
}

const fn cmd_s(module: &'static str, name: &'static str, shorthand: &'static str) -> Binding {
    Binding::Function {
        module,
        name,
        shorthand: Some(shorthand),
        result: None,
        body: false,
    }
}

const fn cmd_body(module: &'static str, name: &'static str) -> Binding {
    Binding::Function {
        module,
        name,
        shorthand: None,
        result: None,
        body: true,
    }
}

const fn val(module: &'static str, name: &'static str, result: Value) -> Binding {
    Binding::Function {
        module,
        name,
        shorthand: None,
        result: Some(result),
        body: false,
    }
}

const fn val_s(
    module: &'static str,
    name: &'static str,
    result: Value,
    shorthand: &'static str,
) -> Binding {
    Binding::Function {
        module,
        name,
        shorthand: Some(shorthand),
        result: Some(result),
        body: false,
    }
}

const fn core(spelling: &'static str) -> Binding {
    Binding::Syntax(spelling)
}

const fn hat(name: &'static str) -> Binding {
    Binding::Hat(name)
}

const fn row(opcode: &'static str, binding: Binding) -> Row {
    Row { opcode, binding }
}

/// A catalog block raven refuses, and why.
const fn no(module: &'static str, name: &'static str, why: &'static str) -> Binding {
    Binding::Forbidden { module, name, why }
}

/// Every catalog block, and how raven reaches it.
pub static BINDINGS: &[Row] = &[
    // ------------------------------------------------------------- Motion --
    row("motion_movesteps", cmd("motion", "move_steps")),
    row("motion_turnright", cmd("motion", "turn_right")),
    row("motion_turnleft", cmd("motion", "turn_left")),
    row("motion_goto", cmd("motion", "goto")),
    row("motion_gotoxy", cmd("motion", "go_to_xy")),
    row("motion_glideto", cmd("motion", "glide_to")),
    row("motion_glidesecstoxy", cmd("motion", "glide_secs_to_xy")),
    row(
        "motion_pointindirection",
        cmd("motion", "point_in_direction"),
    ),
    row("motion_pointtowards", cmd("motion", "point_towards")),
    row("motion_changexby", cmd("motion", "change_x_by")),
    row("motion_setx", cmd("motion", "set_x")),
    row("motion_changeyby", cmd("motion", "change_y_by")),
    row("motion_sety", cmd("motion", "set_y")),
    row("motion_ifonedgebounce", cmd("motion", "if_on_edge_bounce")),
    row(
        "motion_setrotationstyle",
        cmd("motion", "set_rotation_style"),
    ),
    row("motion_xposition", val("motion", "x_position", Value::Num)),
    row("motion_yposition", val("motion", "y_position", Value::Num)),
    row("motion_direction", val("motion", "direction", Value::Num)),
    // -------------------------------------------------------------- Looks --
    row("looks_say", cmd("looks", "say")),
    row("looks_sayforsecs", cmd("looks", "say_for_secs")),
    row("looks_think", cmd("looks", "think")),
    row("looks_thinkforsecs", cmd("looks", "think_for_secs")),
    row("looks_show", cmd("looks", "show")),
    row("looks_hide", cmd("looks", "hide")),
    row("looks_switchcostumeto", cmd("looks", "switch_costume_to")),
    row("looks_nextcostume", cmd("looks", "next_costume")),
    row("looks_switchbackdropto", cmd("looks", "switch_backdrop_to")),
    row(
        "looks_switchbackdroptoandwait",
        cmd("looks", "switch_backdrop_to_and_wait"),
    ),
    row("looks_nextbackdrop", cmd("looks", "next_backdrop")),
    row("looks_changeeffectby", cmd("looks", "change_effect_by")),
    row("looks_seteffectto", cmd("looks", "set_effect_to")),
    row(
        "looks_cleargraphiceffects",
        cmd("looks", "clear_graphic_effects"),
    ),
    row("looks_changesizeby", cmd("looks", "change_size_by")),
    row("looks_setsizeto", cmd("looks", "set_size_to")),
    row("looks_gotofrontback", cmd("looks", "go_to_front_back")),
    row(
        "looks_goforwardbackwardlayers",
        cmd("looks", "go_forward_backward_layers"),
    ),
    row("looks_size", val("looks", "size", Value::Num)),
    row(
        "looks_costumenumbername",
        val("looks", "costume_number_name", Value::Str),
    ),
    row(
        "looks_backdropnumbername",
        val("looks", "backdrop_number_name", Value::Str),
    ),
    // -------------------------------------------------------------- Sound --
    row("sound_play", cmd("sound", "play")),
    row("sound_playuntildone", cmd("sound", "play_until_done")),
    row("sound_stopallsounds", cmd("sound", "stop_all_sounds")),
    row("sound_changeeffectby", cmd("sound", "change_effect_by")),
    row("sound_seteffectto", cmd("sound", "set_effect_to")),
    row("sound_cleareffects", cmd("sound", "clear_effects")),
    row("sound_changevolumeby", cmd("sound", "change_volume_by")),
    row("sound_setvolumeto", cmd("sound", "set_volume_to")),
    row("sound_volume", val("sound", "volume", Value::Num)),
    // ------------------------------------------------------------- Events --
    row("event_whenflagclicked", hat("flag_clicked")),
    row("event_whenkeypressed", hat("key_pressed")),
    row("event_whenthisspriteclicked", hat("clicked")),
    row("event_whenstageclicked", hat("stage_clicked")),
    row("event_whenbackdropswitchesto", hat("backdrop_switches_to")),
    row("event_whengreaterthan", hat("greater_than")),
    row("event_whenbroadcastreceived", hat("broadcast_received")),
    row("event_broadcast", cmd("events", "broadcast")),
    row(
        "event_broadcastandwait",
        cmd("events", "broadcast_and_wait"),
    ),
    // ------------------------------------------------------------ Control --
    row("control_wait", cmd("control", "wait")),
    row("control_repeat", core("repeat n { … }")),
    row("control_forever", core("forever { … }")),
    row("control_if", core("if c { … }")),
    row("control_if_else", core("if c { … } else { … }")),
    row("control_wait_until", cmd("control", "wait_until")),
    row("control_repeat_until", core("repeat_until c { … }")),
    row("control_stop", cmd("control", "stop")),
    row("control_start_as_clone", hat("clone_start")),
    row("control_create_clone_of", cmd("control", "create_clone_of")),
    row(
        "control_delete_this_clone",
        cmd("control", "delete_this_clone"),
    ),
    row("control_while", cmd_body("control", "while")),
    row("control_for_each", cmd_body("control", "for_each")),
    row("control_all_at_once", cmd_body("control", "all_at_once")),
    row("control_get_counter", val("control", "counter", Value::Num)),
    row("control_incr_counter", cmd("control", "incr_counter")),
    row("control_clear_counter", cmd("control", "clear_counter")),
    // ------------------------------------------------------------ Sensing --
    row(
        "sensing_touchingobject",
        val("sensing", "touching_object", Value::Bool),
    ),
    row(
        "sensing_touchingcolor",
        val("sensing", "touching_color", Value::Bool),
    ),
    row(
        "sensing_coloristouchingcolor",
        val("sensing", "color_is_touching_color", Value::Bool),
    ),
    row(
        "sensing_distanceto",
        val("sensing", "distance_to", Value::Num),
    ),
    row("sensing_askandwait", cmd("sensing", "ask_and_wait")),
    row("sensing_answer", val("sensing", "answer", Value::Str)),
    row(
        "sensing_keypressed",
        val("sensing", "key_pressed", Value::Bool),
    ),
    row(
        "sensing_mousedown",
        val("sensing", "mouse_down", Value::Bool),
    ),
    row("sensing_mousex", val("sensing", "mouse_x", Value::Num)),
    row("sensing_mousey", val("sensing", "mouse_y", Value::Num)),
    row("sensing_setdragmode", cmd("sensing", "set_drag_mode")),
    row("sensing_loudness", val("sensing", "loudness", Value::Num)),
    row("sensing_timer", val("sensing", "timer", Value::Num)),
    row("sensing_resettimer", cmd("sensing", "reset_timer")),
    row("sensing_of", val("sensing", "of", Value::Str)),
    row("sensing_current", val("sensing", "current", Value::Num)),
    row(
        "sensing_dayssince2000",
        val("sensing", "days_since_2000", Value::Num),
    ),
    row("sensing_username", val("sensing", "username", Value::Str)),
    row("sensing_online", val("sensing", "online", Value::Bool)),
    // ---------------------------------------------------------- Operators --
    row("operator_add", core("a + b")),
    row("operator_subtract", core("a - b")),
    row("operator_multiply", core("a * b")),
    row("operator_divide", core("a / b")),
    row("operator_random", val("operators", "random", Value::Num)),
    row("operator_lt", core("a < b")),
    row("operator_equals", core("a == b")),
    row("operator_gt", core("a > b")),
    row("operator_and", core("a && b")),
    row("operator_or", core("a || b")),
    row("operator_not", core("!a")),
    row("operator_join", val("operators", "join", Value::Str)),
    row(
        "operator_letter_of",
        val("operators", "letter_of", Value::Str),
    ),
    row("operator_length", val("operators", "length", Value::Num)),
    row(
        "operator_contains",
        val("operators", "contains", Value::Bool),
    ),
    row("operator_mod", core("a % b")),
    row("operator_round", val("operators", "round", Value::Num)),
    row("operator_mathop", val("operators", "mathop", Value::Num)),
    // --------------------------------------------------------------- Data --
    //
    // Nothing here names a Scratch variable, because raven does not declare one.
    // Program state lives in the virtual memory system, and the only way to
    // touch it is to name a `var` or a `let` in raven and let the compiler pick
    // the cell. The five blocks below are therefore refused rather than bound:
    // they are the low-level name-and-value interface, and raven's whole claim
    // is that a program cannot reach it.
    row(
        "data_variable",
        no(
            "data",
            "value_of",
            "a program never names a Scratch variable; `var n: num = 0;` is a VMS cell, and `n` reads it",
        ),
    ),
    row(
        "data_setvariableto",
        no("data", "set_variable_to", "variables are VMS cells; write `n = value;`"),
    ),
    row(
        "data_changevariableby",
        no("data", "change_variable_by", "variables are VMS cells; write `n += value;`"),
    ),
    row(
        "data_showvariable",
        no(
            "data",
            "show_variable",
            "a VMS cell has no monitor; use a `list` if you want one on screen",
        ),
    ),
    row(
        "data_hidevariable",
        no(
            "data",
            "hide_variable",
            "a VMS cell has no monitor; use a `list` if you want one on screen",
        ),
    ),
    row(
        "data_listcontents",
        val("data", "contents_of_list", Value::Str),
    ),
    row("data_addtolist", cmd("data", "add_to_list")),
    row("data_deleteoflist", cmd("data", "delete_of_list")),
    row("data_deletealloflist", cmd("data", "delete_all_of_list")),
    row("data_insertatlist", cmd("data", "insert_at_list")),
    row(
        "data_replaceitemoflist",
        cmd_s("data", "replace_item_of_list", "l[i] = e;"),
    ),
    row(
        "data_itemoflist",
        val_s("data", "item_of_list", Value::ListElement, "l[i]"),
    ),
    row(
        "data_itemnumoflist",
        val("data", "item_num_of_list", Value::Num),
    ),
    row(
        "data_lengthoflist",
        val("data", "length_of_list", Value::Num),
    ),
    row(
        "data_listcontainsitem",
        val("data", "list_contains_item", Value::Bool),
    ),
    row("data_showlist", cmd("data", "show_list")),
    row("data_hidelist", cmd("data", "hide_list")),
    // --------------------------------------------------------- My Blocks --
    row(
        "argument_reporter_string_number",
        core("a `str` or `num` procedure parameter"),
    ),
    row(
        "argument_reporter_boolean",
        core("a `bool` procedure parameter"),
    ),
    // ---------------------------------------------------------------- Pen --
    row("pen_clear", cmd("pen", "clear")),
    row("pen_stamp", cmd("pen", "stamp")),
    row("pen_penDown", cmd("pen", "pen_down")),
    row("pen_penUp", cmd("pen", "pen_up")),
    row(
        "pen_setPenColorToColor",
        cmd("pen", "set_pen_color_to_color"),
    ),
    row(
        "pen_changePenColorParamBy",
        cmd("pen", "change_pen_color_param_by"),
    ),
    row(
        "pen_setPenColorParamTo",
        cmd("pen", "set_pen_color_param_to"),
    ),
    row("pen_changePenSizeBy", cmd("pen", "change_pen_size_by")),
    row("pen_setPenSizeTo", cmd("pen", "set_pen_size_to")),
    row(
        "pen_setPenShadeToNumber",
        cmd("pen", "set_pen_shade_to_number"),
    ),
    row("pen_changePenShadeBy", cmd("pen", "change_pen_shade_by")),
    row("pen_setPenHueToNumber", cmd("pen", "set_pen_hue_to_number")),
    row("pen_changePenHueBy", cmd("pen", "change_pen_hue_by")),
    // -------------------------------------------------------------- Music --
    row(
        "music_playDrumForBeats",
        cmd("music", "play_drum_for_beats"),
    ),
    row("music_restForBeats", cmd("music", "rest_for_beats")),
    row(
        "music_playNoteForBeats",
        cmd("music", "play_note_for_beats"),
    ),
    row("music_setInstrument", cmd("music", "set_instrument")),
    row("music_setTempo", cmd("music", "set_tempo")),
    row("music_changeTempo", cmd("music", "change_tempo")),
    row("music_getTempo", val("music", "tempo", Value::Num)),
];

/// The raven spelling of a catalog block.
#[must_use]
pub fn binding(opcode: &str) -> Option<&'static Binding> {
    BINDINGS
        .iter()
        .find(|r| r.opcode == opcode)
        .map(|r| &r.binding)
}

/// The catalog block a `module::name` call refers to.
#[must_use]
pub fn opcode_for(module: &str, name: &str) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|r| matches!(r.binding.function(), Some((m, n)) if m == module && n == name))
        .map(|r| r.opcode)
}

/// Every intrinsic module raven exposes, sorted.
#[must_use]
pub fn modules() -> Vec<&'static str> {
    let mut modules: Vec<&'static str> = BINDINGS
        .iter()
        .filter_map(|r| r.binding.function().map(|(module, _)| module))
        .collect();
    modules.sort_unstable();
    modules.dedup();
    modules
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_scratch::catalog::{self, BlockKind};

    /// The law, checked: no catalog block is unreachable from raven.
    #[test]
    fn every_catalog_block_is_bound() {
        let missing: Vec<&str> = catalog::BLOCKS
            .iter()
            .map(|b| b.opcode)
            .filter(|opcode| binding(opcode).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "these catalog blocks have no raven spelling: {missing:?}\n\
             add a row to `crate::stdlib::BINDINGS`"
        );
    }

    #[test]
    fn every_binding_names_a_real_block_exactly_once() {
        let mut seen: Vec<&str> = Vec::new();
        for row in BINDINGS {
            assert!(
                catalog::block(row.opcode).is_some(),
                "`{}` is bound in raven but is not in the catalog",
                row.opcode
            );
            assert!(
                !seen.contains(&row.opcode),
                "`{}` is bound twice",
                row.opcode
            );
            seen.push(row.opcode);
        }
        assert_eq!(
            BINDINGS.len(),
            catalog::BLOCKS.len(),
            "the binding table and the catalog have different sizes"
        );
    }

    #[test]
    fn function_names_are_unique_within_a_module() {
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for row in BINDINGS {
            if let Some((module, name)) = row.binding.function() {
                assert!(
                    !seen.contains(&(module, name)),
                    "`{module}::{name}` is bound to more than one block"
                );
                seen.push((module, name));
            }
        }
    }

    #[test]
    fn hats_and_scripts_agree() {
        for row in BINDINGS {
            let is_hat = catalog::block(row.opcode).expect("checked above").kind == BlockKind::Hat;
            assert_eq!(
                is_hat,
                matches!(row.binding, Binding::Hat(_)),
                "`{}` is {} a hat block",
                row.opcode,
                if is_hat { "not bound as" } else { "bound as" }
            );
        }
    }

    #[test]
    fn body_blocks_take_a_body_and_produce_nothing() {
        for row in BINDINGS {
            if let Binding::Function { body, result, .. } = row.binding {
                if body {
                    let spec = catalog::block(row.opcode).expect("checked above");
                    assert_eq!(
                        format!("{:?}", spec.body),
                        "Substack",
                        "`{}` takes a body in raven but not in the catalog",
                        row.opcode
                    );
                    assert!(result.is_none(), "`{}` is a body and a value", row.opcode);
                }
            }
        }
    }

    #[test]
    fn values_and_commands_are_told_apart_by_the_catalog() {
        for row in BINDINGS {
            let spec = catalog::block(row.opcode).expect("checked above");
            if let Binding::Function { result, .. } = row.binding {
                assert_eq!(
                    spec.kind.is_value(),
                    result.is_some(),
                    "`{}` is a {:?} but its binding says result = {result:?}",
                    row.opcode,
                    spec.kind
                );
            }
        }
    }

    #[test]
    fn opcodes_resolve_back_from_their_function_names() {
        for row in BINDINGS {
            if let Some((module, name)) = row.binding.function() {
                assert_eq!(
                    opcode_for(module, name),
                    Some(row.opcode),
                    "`{module}::{name}` does not resolve back"
                );
            }
        }
    }

    #[test]
    fn rows_carry_a_spelling() {
        for row in BINDINGS {
            assert!(
                !row.binding.spelling().trim().is_empty(),
                "`{}` has no spelling",
                row.opcode
            );
        }
    }

    #[test]
    fn the_module_list_is_the_catalog_categories() {
        assert_eq!(
            modules(),
            [
                "control",
                "data",
                "events",
                "looks",
                "motion",
                "music",
                "operators",
                "pen",
                "sensing",
                "sound",
            ]
        );
    }

    #[test]
    fn a_binding_describes_itself() {
        assert_eq!(
            binding("motion_movesteps").unwrap().describe(),
            "motion::move_steps"
        );
        assert_eq!(binding("control_if").unwrap().describe(), "if c { … }");
        assert_eq!(
            binding("data_replaceitemoflist").unwrap().spelling(),
            "l[i] = e;"
        );
        assert_eq!(
            binding("event_whenflagclicked").unwrap().describe(),
            "on flag_clicked { … }"
        );
    }

    /// The five blocks that name a Scratch variable are reached by raven and
    /// refused by it: the language's claim is that a program cannot use the raw
    /// name-and-value interface at all.
    #[test]
    fn the_raw_variable_blocks_are_refused_with_a_reason() {
        for opcode in [
            "data_variable",
            "data_setvariableto",
            "data_changevariableby",
            "data_showvariable",
            "data_hidevariable",
        ] {
            let binding = binding(opcode).unwrap_or_else(|| panic!("`{opcode}` has no row"));
            let why = binding.forbidden().unwrap_or_else(|| {
                panic!("`{opcode}` is bound as `{}`", binding.describe());
            });
            assert!(why.len() > 10, "`{opcode}` has no useful reason: {why}");
            let described = binding.describe();
            let (module, name) = described.split_once("::").expect("a module and a name");
            assert_eq!(
                opcode_for(module, name),
                Some(opcode),
                "does not resolve back"
            );
        }
        assert_eq!(
            binding("data_addtolist").unwrap().forbidden(),
            None,
            "a list is a Scratch list, and its blocks are how a program uses one"
        );
    }
}
