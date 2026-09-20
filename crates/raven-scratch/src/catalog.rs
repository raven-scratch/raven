//! The Scratch 3 block catalog.
//!
//! Every entry records exactly what `packages/scratch-vm` expects for one
//! opcode: the input names it reads out of `args`, the field names it reads out
//! of `block.fields`, and the shape of the block itself.
//!
//! Sources for this table (all inside the `raven-scratch/scratch-editor`
//! checkout):
//!
//! * `packages/scratch-vm/src/blocks/scratch3_*.js` — the runtime reads
//!   `args.NAME` for inputs and `args.NAME` for fields, so the argument names
//!   listed here are the literal names the VM looks up.
//! * `packages/scratch-vm/src/serialization/sb2_specmap.js` — names the shadow
//!   opcode (`inputOp`) used for each input.
//! * `packages/scratch-gui/src/lib/make-toolbox-xml.js` — the shadow type and
//!   default value the editor creates for each input.
//! * `packages/scratch-vm/src/extensions/*/index.js` — extension blocks and
//!   their menus (`acceptReporters` decides whether a menu becomes a shadow
//!   menu block or a field on the block itself; see `runtime.js`
//!   `_buildExtensionBlock`).
//!
//! raven-asm exposes blocks one-for-one. A raven-asm statement *is* one Scratch block,
//! which is why the catalog doubles as the language reference.

/// Which Scratch palette the block belongs to. Used for docs and to decide
/// which extension the project needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Motion,
    Looks,
    Sound,
    Events,
    Control,
    Sensing,
    Operators,
    Variables,
    Lists,
    MyBlocks,
    Pen,
    Music,
}

impl Category {
    pub fn title(self) -> &'static str {
        match self {
            Category::Motion => "Motion",
            Category::Looks => "Looks",
            Category::Sound => "Sound",
            Category::Events => "Events",
            Category::Control => "Control",
            Category::Sensing => "Sensing",
            Category::Operators => "Operators",
            Category::Variables => "Variables",
            Category::Lists => "Lists",
            Category::MyBlocks => "My Blocks",
            Category::Pen => "Pen (extension)",
            Category::Music => "Music (extension)",
        }
    }

    /// The extension id a project must list to use this block, if any.
    pub fn extension(self) -> Option<&'static str> {
        match self {
            Category::Pen => Some("pen"),
            Category::Music => Some("music"),
            _ => None,
        }
    }
}

/// The shape of a block, which decides where it may appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Starts a script. Only valid at the top level of a target or module body.
    Hat,
    /// A command block that can be stacked.
    Stack,
    /// A command block with no bottom notch: nothing may follow it.
    Cap,
    /// A round reporter that yields a value.
    Reporter,
    /// A hexagonal reporter that yields a boolean.
    Boolean,
}

impl BlockKind {
    pub fn is_value(self) -> bool {
        matches!(self, BlockKind::Reporter | BlockKind::Boolean)
    }
}

/// Whether a block is part of vanilla Scratch 3 or only of the extended
/// runtimes raven-asm targets (TurboWarp and the `raven-scratch` fork of the VM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    Vanilla,
    Extended,
}

/// Where an argument is stored on the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    /// An entry in the block's `inputs` map, with a shadow block behind it.
    Input,
    /// An entry in the block's `fields` map (a dropdown baked into the block).
    Field,
}

/// The type of value an argument takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `math_number` shadow.
    Number,
    /// `math_positive_number` shadow.
    Positive,
    /// `math_whole_number` shadow.
    Whole,
    /// `math_integer` shadow.
    Integer,
    /// `math_angle` shadow.
    Angle,
    /// `text` shadow.
    Text,
    /// `colour_picker` shadow; written as `"#rrggbb"`.
    Color,
    /// A `data_variable` reporter (an input) or a `VARIABLE` field.
    Variable,
    /// A `data_listcontents` reporter (an input) or a `LIST` field.
    List,
    /// A broadcast message.
    Broadcast,
    /// A hexagonal boolean input; requires a boolean reporter.
    Bool,
    /// A dropdown menu, by menu id.
    Menu(&'static str),
    /// The name of one of the enclosing procedure's parameters.
    ParamName,
}

#[derive(Clone, Copy, Debug)]
pub struct ArgSpec {
    /// The Scratch input or field name, verbatim.
    pub name: &'static str,
    pub wire: Wire,
    pub shape: Shape,
}

/// How a block's `{ ... }` body attaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    /// The block takes no body.
    None,
    /// The body becomes the block's `next` (hats and `procedures_definition`).
    Next,
    /// The body becomes the `SUBSTACK` input.
    Substack,
    /// The body becomes `SUBSTACK`, and `else { ... }` becomes `SUBSTACK2`.
    SubstackElse,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockSpec {
    pub opcode: &'static str,
    pub category: Category,
    pub kind: BlockKind,
    pub stability: Stability,
    pub args: &'static [ArgSpec],
    pub body: Body,
    /// The Scratch block text, with `%1`-style placeholders in argument order.
    pub text: &'static str,
    pub summary: &'static str,
}

/// The domain of values a menu accepts at compile time.
#[derive(Clone, Copy, Debug)]
pub enum MenuDomain {
    /// A closed list of literal values.
    Fixed(&'static [&'static str]),
    /// The costumes declared by the target being compiled.
    Costumes,
    /// The backdrops declared by the stage.
    Backdrops,
    /// The sounds declared by the target being compiled.
    Sounds,
    /// Sprite names, plus the fixed extras listed here.
    Sprites(&'static [&'static str]),
    /// A menu whose value is only known at run time; any literal is accepted.
    Open,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuSpec {
    /// Internal lookup id.
    pub id: &'static str,
    /// The shadow block's opcode.
    pub opcode: &'static str,
    /// The field name inside the shadow block.
    pub field: &'static str,
    pub domain: MenuDomain,
    /// Scratch's `acceptReporters`: whether a reporter may stand in for the
    /// dropdown. When true, a reporter becomes the obscured-shadow input
    /// `[3, reporterId, shadowId]`; when false the value is a plain field and a
    /// reporter is rejected at compile time.
    pub accept_reporters: bool,
}

/// Menus that are baked into a block as a dropdown field rather than a shadow
/// block, plus the closed ones whose values raven-asm can check. Returns `None` for
/// menus whose value set depends on the rest of the project.
pub fn fixed_menu_values(id: &str) -> Option<&'static [&'static str]> {
    Some(match id {
        "looks_effect" => EFFECTS,
        "sound_effect" => SOUND_EFFECTS,
        "rotation_style" => ROTATION_STYLES,
        "front_back" => FRONT_BACK,
        "forward_backward" => FORWARD_BACKWARD,
        "number_name" => NUMBER_NAME,
        "stop_option" => STOP_OPTIONS,
        "drag_mode" => DRAG_MODES,
        "math_op" => MATH_OPS,
        "greater_than" => GREATER_THAN,
        "current_menu" => CURRENT_MENU,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------

/// Every menu here is a value input with a shadow menu block, so Scratch lets a
/// reporter stand in for the dropdown: `make-toolbox-xml.js` declares the
/// built-in ones as `<value><shadow type="...">`, and the `pen` and `music`
/// extensions declare `acceptReporters: true`, which
/// `runtime._buildExtensionBlock` turns into the same shadow menu block. Menus
/// that are baked into the block as a field instead are not listed here; they
/// are resolved by `fixed_menu_values` and never accept a reporter.
pub static MENUS: &[MenuSpec] = &[
    MenuSpec {
        id: "motion_goto",
        opcode: "motion_goto_menu",
        field: "TO",
        domain: MenuDomain::Sprites(&["_random_", "_mouse_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "motion_glideto",
        opcode: "motion_glideto_menu",
        field: "TO",
        domain: MenuDomain::Sprites(&["_random_", "_mouse_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "motion_pointtowards",
        opcode: "motion_pointtowards_menu",
        field: "TOWARDS",
        domain: MenuDomain::Sprites(&["_mouse_", "_random_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "looks_costume",
        opcode: "looks_costume",
        field: "COSTUME",
        domain: MenuDomain::Costumes,
        accept_reporters: true,
    },
    MenuSpec {
        id: "looks_backdrops",
        opcode: "looks_backdrops",
        field: "BACKDROP",
        domain: MenuDomain::Backdrops,
        accept_reporters: true,
    },
    MenuSpec {
        id: "sound_sounds",
        opcode: "sound_sounds_menu",
        field: "SOUND_MENU",
        domain: MenuDomain::Sounds,
        accept_reporters: true,
    },
    MenuSpec {
        id: "sensing_touchingobject",
        opcode: "sensing_touchingobjectmenu",
        field: "TOUCHINGOBJECTMENU",
        domain: MenuDomain::Sprites(&["_mouse_", "_edge_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "sensing_distanceto",
        opcode: "sensing_distancetomenu",
        field: "DISTANCETOMENU",
        domain: MenuDomain::Sprites(&["_mouse_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "sensing_keyoptions",
        opcode: "sensing_keyoptions",
        field: "KEY_OPTION",
        domain: MenuDomain::Fixed(KEYS),
        accept_reporters: true,
    },
    MenuSpec {
        id: "sensing_of_object",
        opcode: "sensing_of_object_menu",
        field: "OBJECT",
        domain: MenuDomain::Sprites(&["_stage_"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "control_create_clone_of",
        opcode: "control_create_clone_of_menu",
        field: "CLONE_OPTION",
        domain: MenuDomain::Sprites(&["_myself_"]),
        accept_reporters: true,
    },
    // Extension menus. `pen` and `music` declare `acceptReporters: true`.
    MenuSpec {
        id: "pen_color_param",
        opcode: "pen_menu_colorParam",
        field: "colorParam",
        domain: MenuDomain::Fixed(&["color", "saturation", "brightness", "transparency"]),
        accept_reporters: true,
    },
    MenuSpec {
        id: "music_drum",
        opcode: "music_menu_DRUM",
        field: "DRUM",
        domain: MenuDomain::Open,
        accept_reporters: true,
    },
    MenuSpec {
        id: "music_instrument",
        opcode: "music_menu_INSTRUMENT",
        field: "INSTRUMENT",
        domain: MenuDomain::Open,
        accept_reporters: true,
    },
];

pub fn menu(id: &str) -> Option<&'static MenuSpec> {
    MENUS.iter().find(|m| m.id == id)
}

/// Keyboard values accepted by Scratch's key dropdowns.
pub static KEYS: &[&str] = &[
    "any",
    "space",
    "up arrow",
    "down arrow",
    "right arrow",
    "left arrow",
    "enter",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
];

pub static EFFECTS: &[&str] = &[
    "COLOR",
    "FISHEYE",
    "WHIRL",
    "PIXELATE",
    "MOSAIC",
    "BRIGHTNESS",
    "GHOST",
];

pub static SOUND_EFFECTS: &[&str] = &["PITCH", "PAN"];

pub static ROTATION_STYLES: &[&str] = &["all around", "left-right", "don't rotate"];

pub static FRONT_BACK: &[&str] = &["front", "back"];

pub static FORWARD_BACKWARD: &[&str] = &["forward", "backward"];

pub static NUMBER_NAME: &[&str] = &["number", "name"];

pub static STOP_OPTIONS: &[&str] = &[
    "all",
    "this script",
    "other scripts in sprite",
    "other scripts in stage",
];

pub static DRAG_MODES: &[&str] = &["draggable", "not draggable"];

pub static MATH_OPS: &[&str] = &[
    "abs", "floor", "ceiling", "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "ln", "log",
    "e ^", "10 ^",
];

pub static GREATER_THAN: &[&str] = &["loudness", "timer"];

pub static CURRENT_MENU: &[&str] = &[
    "YEAR",
    "MONTH",
    "DATE",
    "DAYOFWEEK",
    "HOUR",
    "MINUTE",
    "SECOND",
];

// ---------------------------------------------------------------------------
// Argument shortcuts
// ---------------------------------------------------------------------------

const fn input(name: &'static str, shape: Shape) -> ArgSpec {
    ArgSpec {
        name,
        wire: Wire::Input,
        shape,
    }
}

const fn field(name: &'static str, shape: Shape) -> ArgSpec {
    ArgSpec {
        name,
        wire: Wire::Field,
        shape,
    }
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

use BlockKind::{Boolean, Cap, Hat, Reporter, Stack};
use Body::{Next as BodyNext, None as BodyNone, Substack, SubstackElse};
use Shape::{
    Angle, Bool, Broadcast, Color, Integer, List, Menu, Number, ParamName, Positive, Text,
    Variable, Whole,
};
use Stability::{Extended, Vanilla};

pub static BLOCKS: &[BlockSpec] = &[
    // ------------------------------------------------------------- Motion --
    BlockSpec {
        opcode: "motion_movesteps",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("STEPS", Number)],
        body: BodyNone,
        text: "move %1 steps",
        summary: "Move the sprite forward by a number of steps.",
    },
    BlockSpec {
        opcode: "motion_turnright",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DEGREES", Number)],
        body: BodyNone,
        text: "turn right %1 degrees",
        summary: "Rotate the sprite clockwise.",
    },
    BlockSpec {
        opcode: "motion_turnleft",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DEGREES", Number)],
        body: BodyNone,
        text: "turn left %1 degrees",
        summary: "Rotate the sprite anticlockwise.",
    },
    BlockSpec {
        opcode: "motion_goto",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("TO", Menu("motion_goto"))],
        body: BodyNone,
        text: "go to %1",
        summary: "Move the sprite to another sprite, the mouse pointer or a random spot.",
    },
    BlockSpec {
        opcode: "motion_gotoxy",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("X", Number), input("Y", Number)],
        body: BodyNone,
        text: "go to x: %1 y: %2",
        summary: "Move the sprite to a stage coordinate.",
    },
    BlockSpec {
        opcode: "motion_glideto",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SECS", Number), input("TO", Menu("motion_glideto"))],
        body: BodyNone,
        text: "glide %1 secs to %2",
        summary: "Glide to another sprite, the mouse pointer or a random spot.",
    },
    BlockSpec {
        opcode: "motion_glidesecstoxy",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[
            input("SECS", Number),
            input("X", Number),
            input("Y", Number),
        ],
        body: BodyNone,
        text: "glide %1 secs to x: %2 y: %3",
        summary: "Glide to a stage coordinate over a duration.",
    },
    BlockSpec {
        opcode: "motion_pointindirection",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DIRECTION", Angle)],
        body: BodyNone,
        text: "point in direction %1",
        summary: "Point the sprite in a direction (90 is right).",
    },
    BlockSpec {
        opcode: "motion_pointtowards",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("TOWARDS", Menu("motion_pointtowards"))],
        body: BodyNone,
        text: "point towards %1",
        summary: "Point the sprite towards another sprite or the mouse pointer.",
    },
    BlockSpec {
        opcode: "motion_changexby",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DX", Number)],
        body: BodyNone,
        text: "change x by %1",
        summary: "Move the sprite horizontally.",
    },
    BlockSpec {
        opcode: "motion_setx",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("X", Number)],
        body: BodyNone,
        text: "set x to %1",
        summary: "Set the sprite's horizontal position.",
    },
    BlockSpec {
        opcode: "motion_changeyby",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DY", Number)],
        body: BodyNone,
        text: "change y by %1",
        summary: "Move the sprite vertically.",
    },
    BlockSpec {
        opcode: "motion_sety",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[input("Y", Number)],
        body: BodyNone,
        text: "set y to %1",
        summary: "Set the sprite's vertical position.",
    },
    BlockSpec {
        opcode: "motion_ifonedgebounce",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "if on edge, bounce",
        summary: "Turn the sprite away from the stage edge.",
    },
    BlockSpec {
        opcode: "motion_setrotationstyle",
        category: Category::Motion,
        kind: Stack,
        stability: Vanilla,
        args: &[field("STYLE", Menu("rotation_style"))],
        body: BodyNone,
        text: "set rotation style %1",
        summary: "Choose how the sprite's costume rotates.",
    },
    BlockSpec {
        opcode: "motion_xposition",
        category: Category::Motion,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "x position",
        summary: "The sprite's x coordinate.",
    },
    BlockSpec {
        opcode: "motion_yposition",
        category: Category::Motion,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "y position",
        summary: "The sprite's y coordinate.",
    },
    BlockSpec {
        opcode: "motion_direction",
        category: Category::Motion,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "direction",
        summary: "The direction the sprite is pointing.",
    },
    // -------------------------------------------------------------- Looks --
    BlockSpec {
        opcode: "looks_say",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("MESSAGE", Text)],
        body: BodyNone,
        text: "say %1",
        summary: "Show a speech bubble.",
    },
    BlockSpec {
        opcode: "looks_sayforsecs",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("MESSAGE", Text), input("SECS", Number)],
        body: BodyNone,
        text: "say %1 for %2 seconds",
        summary: "Show a speech bubble for a duration.",
    },
    BlockSpec {
        opcode: "looks_think",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("MESSAGE", Text)],
        body: BodyNone,
        text: "think %1",
        summary: "Show a thought bubble.",
    },
    BlockSpec {
        opcode: "looks_thinkforsecs",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("MESSAGE", Text), input("SECS", Number)],
        body: BodyNone,
        text: "think %1 for %2 seconds",
        summary: "Show a thought bubble for a duration.",
    },
    BlockSpec {
        opcode: "looks_show",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "show",
        summary: "Show the sprite.",
    },
    BlockSpec {
        opcode: "looks_hide",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "hide",
        summary: "Hide the sprite.",
    },
    BlockSpec {
        opcode: "looks_switchcostumeto",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("COSTUME", Menu("looks_costume"))],
        body: BodyNone,
        text: "switch costume to %1",
        summary: "Switch the sprite's costume.",
    },
    BlockSpec {
        opcode: "looks_nextcostume",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "next costume",
        summary: "Switch to the next costume in the list.",
    },
    BlockSpec {
        opcode: "looks_switchbackdropto",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("BACKDROP", Menu("looks_backdrops"))],
        body: BodyNone,
        text: "switch backdrop to %1",
        summary: "Switch the stage backdrop.",
    },
    BlockSpec {
        opcode: "looks_switchbackdroptoandwait",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("BACKDROP", Menu("looks_backdrops"))],
        body: BodyNone,
        text: "switch backdrop to %1 and wait",
        summary: "Switch the backdrop and wait for the scripts it starts.",
    },
    BlockSpec {
        opcode: "looks_nextbackdrop",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "next backdrop",
        summary: "Switch to the next backdrop in the list.",
    },
    BlockSpec {
        opcode: "looks_changeeffectby",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[
            field("EFFECT", Menu("looks_effect")),
            input("CHANGE", Number),
        ],
        body: BodyNone,
        text: "change %1 effect by %2",
        summary: "Change a graphic effect.",
    },
    BlockSpec {
        opcode: "looks_seteffectto",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[
            field("EFFECT", Menu("looks_effect")),
            input("VALUE", Number),
        ],
        body: BodyNone,
        text: "set %1 effect to %2",
        summary: "Set a graphic effect.",
    },
    BlockSpec {
        opcode: "looks_cleargraphiceffects",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "clear graphic effects",
        summary: "Reset every graphic effect.",
    },
    BlockSpec {
        opcode: "looks_changesizeby",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CHANGE", Number)],
        body: BodyNone,
        text: "change size by %1",
        summary: "Grow or shrink the sprite.",
    },
    BlockSpec {
        opcode: "looks_setsizeto",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SIZE", Number)],
        body: BodyNone,
        text: "set size to %1%",
        summary: "Set the sprite's size as a percentage.",
    },
    BlockSpec {
        opcode: "looks_gotofrontback",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[field("FRONT_BACK", Menu("front_back"))],
        body: BodyNone,
        text: "go to %1 layer",
        summary: "Move the sprite to the front or the back layer.",
    },
    BlockSpec {
        opcode: "looks_goforwardbackwardlayers",
        category: Category::Looks,
        kind: Stack,
        stability: Vanilla,
        args: &[
            field("FORWARD_BACKWARD", Menu("forward_backward")),
            input("NUM", Integer),
        ],
        body: BodyNone,
        text: "go %1 %2 layers",
        summary: "Move the sprite a number of layers forwards or backwards.",
    },
    BlockSpec {
        opcode: "looks_size",
        category: Category::Looks,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "size",
        summary: "The sprite's size as a percentage.",
    },
    BlockSpec {
        opcode: "looks_costumenumbername",
        category: Category::Looks,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("NUMBER_NAME", Menu("number_name"))],
        body: BodyNone,
        text: "costume %1",
        summary: "The sprite's current costume number or name.",
    },
    BlockSpec {
        opcode: "looks_backdropnumbername",
        category: Category::Looks,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("NUMBER_NAME", Menu("number_name"))],
        body: BodyNone,
        text: "backdrop %1",
        summary: "The stage's current backdrop number or name.",
    },
    // -------------------------------------------------------------- Sound --
    BlockSpec {
        opcode: "sound_play",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SOUND_MENU", Menu("sound_sounds"))],
        body: BodyNone,
        text: "start sound %1",
        summary: "Start a sound without waiting.",
    },
    BlockSpec {
        opcode: "sound_playuntildone",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SOUND_MENU", Menu("sound_sounds"))],
        body: BodyNone,
        text: "play sound %1 until done",
        summary: "Play a sound and wait for it to finish.",
    },
    BlockSpec {
        opcode: "sound_stopallsounds",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "stop all sounds",
        summary: "Stop every playing sound.",
    },
    BlockSpec {
        opcode: "sound_changeeffectby",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[
            field("EFFECT", Menu("sound_effect")),
            input("VALUE", Number),
        ],
        body: BodyNone,
        text: "change %1 effect by %2",
        summary: "Change a sound effect.",
    },
    BlockSpec {
        opcode: "sound_seteffectto",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[
            field("EFFECT", Menu("sound_effect")),
            input("VALUE", Number),
        ],
        body: BodyNone,
        text: "set %1 effect to %2",
        summary: "Set a sound effect.",
    },
    BlockSpec {
        opcode: "sound_cleareffects",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "clear sound effects",
        summary: "Reset every sound effect.",
    },
    BlockSpec {
        opcode: "sound_changevolumeby",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[input("VOLUME", Number)],
        body: BodyNone,
        text: "change volume by %1",
        summary: "Turn the volume up or down.",
    },
    BlockSpec {
        opcode: "sound_setvolumeto",
        category: Category::Sound,
        kind: Stack,
        stability: Vanilla,
        args: &[input("VOLUME", Number)],
        body: BodyNone,
        text: "set volume to %1%",
        summary: "Set the volume as a percentage.",
    },
    BlockSpec {
        opcode: "sound_volume",
        category: Category::Sound,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "volume",
        summary: "The current volume as a percentage.",
    },
    // ------------------------------------------------------------- Events --
    BlockSpec {
        opcode: "event_whenflagclicked",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[],
        body: BodyNext,
        text: "when green flag clicked",
        summary: "Runs when the green flag is clicked.",
    },
    BlockSpec {
        opcode: "event_whenkeypressed",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[field("KEY_OPTION", Menu("sensing_keyoptions"))],
        body: BodyNext,
        text: "when %1 key pressed",
        summary: "Runs when a key is pressed.",
    },
    BlockSpec {
        opcode: "event_whenthisspriteclicked",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[],
        body: BodyNext,
        text: "when this sprite clicked",
        summary: "Runs when the sprite is clicked.",
    },
    BlockSpec {
        opcode: "event_whenstageclicked",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[],
        body: BodyNext,
        text: "when stage clicked",
        summary: "Runs when the stage is clicked.",
    },
    BlockSpec {
        opcode: "event_whenbackdropswitchesto",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[field("BACKDROP", Menu("looks_backdrops"))],
        body: BodyNext,
        text: "when backdrop switches to %1",
        summary: "Runs when the backdrop changes.",
    },
    BlockSpec {
        opcode: "event_whengreaterthan",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[
            field("WHENGREATERTHANMENU", Menu("greater_than")),
            input("VALUE", Number),
        ],
        body: BodyNext,
        text: "when %1 > %2",
        summary: "Runs when the timer or loudness passes a value.",
    },
    BlockSpec {
        opcode: "event_whenbroadcastreceived",
        category: Category::Events,
        kind: Hat,
        stability: Vanilla,
        args: &[field("BROADCAST_OPTION", Broadcast)],
        body: BodyNext,
        text: "when I receive %1",
        summary: "Runs when a broadcast message is sent.",
    },
    BlockSpec {
        opcode: "event_broadcast",
        category: Category::Events,
        kind: Stack,
        stability: Vanilla,
        args: &[input("BROADCAST_INPUT", Broadcast)],
        body: BodyNone,
        text: "broadcast %1",
        summary: "Send a broadcast message.",
    },
    BlockSpec {
        opcode: "event_broadcastandwait",
        category: Category::Events,
        kind: Stack,
        stability: Vanilla,
        args: &[input("BROADCAST_INPUT", Broadcast)],
        body: BodyNone,
        text: "broadcast %1 and wait",
        summary: "Send a broadcast message and wait for its scripts.",
    },
    // ------------------------------------------------------------ Control --
    BlockSpec {
        opcode: "control_wait",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DURATION", Positive)],
        body: BodyNone,
        text: "wait %1 seconds",
        summary: "Pause the script.",
    },
    BlockSpec {
        opcode: "control_repeat",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("TIMES", Whole)],
        body: Substack,
        text: "repeat %1",
        summary: "Run the body a fixed number of times.",
    },
    BlockSpec {
        opcode: "control_forever",
        category: Category::Control,
        kind: Cap,
        stability: Vanilla,
        args: &[],
        body: Substack,
        text: "forever",
        summary: "Run the body until the script is stopped. Nothing may follow it.",
    },
    BlockSpec {
        opcode: "control_if",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CONDITION", Bool)],
        body: Substack,
        text: "if %1 then",
        summary: "Run the body when the condition is true.",
    },
    BlockSpec {
        opcode: "control_if_else",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CONDITION", Bool)],
        body: SubstackElse,
        text: "if %1 then ... else",
        summary: "Run one of two bodies depending on the condition.",
    },
    BlockSpec {
        opcode: "control_wait_until",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CONDITION", Bool)],
        body: BodyNone,
        text: "wait until %1",
        summary: "Pause until the condition becomes true.",
    },
    BlockSpec {
        opcode: "control_repeat_until",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CONDITION", Bool)],
        body: Substack,
        text: "repeat until %1",
        summary: "Run the body until the condition becomes true.",
    },
    BlockSpec {
        opcode: "control_stop",
        category: Category::Control,
        kind: Cap,
        stability: Vanilla,
        args: &[field("STOP_OPTION", Menu("stop_option"))],
        body: BodyNext,
        text: "stop %1",
        summary: "Stop all scripts, this script, or the sprite's other scripts.",
    },
    BlockSpec {
        opcode: "control_start_as_clone",
        category: Category::Control,
        kind: Hat,
        stability: Vanilla,
        args: &[],
        body: BodyNext,
        text: "when I start as a clone",
        summary: "Runs in a freshly created clone.",
    },
    BlockSpec {
        opcode: "control_create_clone_of",
        category: Category::Control,
        kind: Stack,
        stability: Vanilla,
        args: &[input("CLONE_OPTION", Menu("control_create_clone_of"))],
        body: BodyNone,
        text: "create clone of %1",
        summary: "Clone a sprite.",
    },
    BlockSpec {
        opcode: "control_delete_this_clone",
        category: Category::Control,
        kind: Cap,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "delete this clone",
        summary: "Delete the running clone.",
    },
    BlockSpec {
        opcode: "control_while",
        category: Category::Control,
        kind: Stack,
        stability: Extended,
        args: &[input("CONDITION", Bool)],
        body: Substack,
        text: "while %1",
        summary: "Run the body while the condition is true (extended runtimes).",
    },
    BlockSpec {
        opcode: "control_for_each",
        category: Category::Control,
        kind: Stack,
        stability: Extended,
        args: &[field("VARIABLE", Variable), input("VALUE", Number)],
        body: Substack,
        text: "for each %1 in %2",
        summary: "Run the body once per item of a list (extended runtimes).",
    },
    BlockSpec {
        opcode: "control_all_at_once",
        category: Category::Control,
        kind: Stack,
        stability: Extended,
        args: &[],
        body: Substack,
        text: "all at once",
        summary: "Run the body without yielding (extended runtimes).",
    },
    BlockSpec {
        opcode: "control_get_counter",
        category: Category::Control,
        kind: Reporter,
        stability: Extended,
        args: &[],
        body: BodyNone,
        text: "counter",
        summary: "The value of the counter (extended runtimes).",
    },
    BlockSpec {
        opcode: "control_incr_counter",
        category: Category::Control,
        kind: Stack,
        stability: Extended,
        args: &[],
        body: BodyNone,
        text: "increment counter",
        summary: "Add one to the counter (extended runtimes).",
    },
    BlockSpec {
        opcode: "control_clear_counter",
        category: Category::Control,
        kind: Stack,
        stability: Extended,
        args: &[],
        body: BodyNone,
        text: "clear counter",
        summary: "Reset the counter (extended runtimes).",
    },
    // ------------------------------------------------------------ Sensing --
    BlockSpec {
        opcode: "sensing_touchingobject",
        category: Category::Sensing,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("TOUCHINGOBJECTMENU", Menu("sensing_touchingobject"))],
        body: BodyNone,
        text: "touching %1?",
        summary: "True when the sprite touches another sprite, the mouse pointer or the edge.",
    },
    BlockSpec {
        opcode: "sensing_touchingcolor",
        category: Category::Sensing,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("COLOR", Color)],
        body: BodyNone,
        text: "touching color %1?",
        summary: "True when the sprite touches a colour.",
    },
    BlockSpec {
        opcode: "sensing_coloristouchingcolor",
        category: Category::Sensing,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("COLOR", Color), input("COLOR2", Color)],
        body: BodyNone,
        text: "color %1 is touching %2?",
        summary: "True when one colour of the sprite touches another colour.",
    },
    BlockSpec {
        opcode: "sensing_distanceto",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("DISTANCETOMENU", Menu("sensing_distanceto"))],
        body: BodyNone,
        text: "distance to %1",
        summary: "Distance to another sprite or the mouse pointer.",
    },
    BlockSpec {
        opcode: "sensing_askandwait",
        category: Category::Sensing,
        kind: Stack,
        stability: Vanilla,
        args: &[input("QUESTION", Text)],
        body: BodyNone,
        text: "ask %1 and wait",
        summary: "Ask a question and store the answer.",
    },
    BlockSpec {
        opcode: "sensing_answer",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "answer",
        summary: "The most recent answer.",
    },
    BlockSpec {
        opcode: "sensing_keypressed",
        category: Category::Sensing,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("KEY_OPTION", Menu("sensing_keyoptions"))],
        body: BodyNone,
        text: "key %1 pressed?",
        summary: "True while a key is held down.",
    },
    BlockSpec {
        opcode: "sensing_mousedown",
        category: Category::Sensing,
        kind: Boolean,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "mouse down?",
        summary: "True while the mouse button is held.",
    },
    BlockSpec {
        opcode: "sensing_mousex",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "mouse x",
        summary: "The mouse pointer's x coordinate.",
    },
    BlockSpec {
        opcode: "sensing_mousey",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "mouse y",
        summary: "The mouse pointer's y coordinate.",
    },
    BlockSpec {
        opcode: "sensing_setdragmode",
        category: Category::Sensing,
        kind: Stack,
        stability: Vanilla,
        args: &[field("DRAG_MODE", Menu("drag_mode"))],
        body: BodyNone,
        text: "set drag mode %1",
        summary: "Allow or forbid dragging the sprite in the player.",
    },
    BlockSpec {
        opcode: "sensing_loudness",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "loudness",
        summary: "How loud the microphone input is.",
    },
    BlockSpec {
        opcode: "sensing_timer",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "timer",
        summary: "Seconds since the timer was reset.",
    },
    BlockSpec {
        opcode: "sensing_resettimer",
        category: Category::Sensing,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "reset timer",
        summary: "Reset the timer to zero.",
    },
    BlockSpec {
        opcode: "sensing_of",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[
            field("PROPERTY", Menu("sensing_of_property")),
            input("OBJECT", Menu("sensing_of_object")),
        ],
        body: BodyNone,
        text: "%1 of %2",
        summary: "Read a property of the stage or of another sprite.",
    },
    BlockSpec {
        opcode: "sensing_current",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("CURRENTMENU", Menu("current_menu"))],
        body: BodyNone,
        text: "current %1",
        summary: "The current date or time component.",
    },
    BlockSpec {
        opcode: "sensing_dayssince2000",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "days since 2000",
        summary: "Days elapsed since 1 January 2000.",
    },
    BlockSpec {
        opcode: "sensing_username",
        category: Category::Sensing,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "username",
        summary: "The signed-in user's name.",
    },
    BlockSpec {
        opcode: "sensing_online",
        category: Category::Sensing,
        kind: Reporter,
        stability: Extended,
        args: &[],
        body: BodyNone,
        text: "online?",
        summary: "Whether the player is online (extended runtimes).",
    },
    // ---------------------------------------------------------- Operators --
    BlockSpec {
        opcode: "operator_add",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM1", Number), input("NUM2", Number)],
        body: BodyNone,
        text: "%1 + %2",
        summary: "Add two numbers.",
    },
    BlockSpec {
        opcode: "operator_subtract",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM1", Number), input("NUM2", Number)],
        body: BodyNone,
        text: "%1 - %2",
        summary: "Subtract the second number from the first.",
    },
    BlockSpec {
        opcode: "operator_multiply",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM1", Number), input("NUM2", Number)],
        body: BodyNone,
        text: "%1 * %2",
        summary: "Multiply two numbers.",
    },
    BlockSpec {
        opcode: "operator_divide",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM1", Number), input("NUM2", Number)],
        body: BodyNone,
        text: "%1 / %2",
        summary: "Divide the first number by the second.",
    },
    BlockSpec {
        opcode: "operator_random",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("FROM", Number), input("TO", Number)],
        body: BodyNone,
        text: "pick random %1 to %2",
        summary: "A random number in an inclusive range.",
    },
    BlockSpec {
        opcode: "operator_lt",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND1", Text), input("OPERAND2", Text)],
        body: BodyNone,
        text: "%1 < %2",
        summary: "True when the first value is smaller.",
    },
    BlockSpec {
        opcode: "operator_equals",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND1", Text), input("OPERAND2", Text)],
        body: BodyNone,
        text: "%1 = %2",
        summary: "True when the values are equal.",
    },
    BlockSpec {
        opcode: "operator_gt",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND1", Text), input("OPERAND2", Text)],
        body: BodyNone,
        text: "%1 > %2",
        summary: "True when the first value is larger.",
    },
    BlockSpec {
        opcode: "operator_and",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND1", Bool), input("OPERAND2", Bool)],
        body: BodyNone,
        text: "%1 and %2",
        summary: "True when both conditions are true.",
    },
    BlockSpec {
        opcode: "operator_or",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND1", Bool), input("OPERAND2", Bool)],
        body: BodyNone,
        text: "%1 or %2",
        summary: "True when either condition is true.",
    },
    BlockSpec {
        opcode: "operator_not",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("OPERAND", Bool)],
        body: BodyNone,
        text: "not %1",
        summary: "Invert a condition.",
    },
    BlockSpec {
        opcode: "operator_join",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("STRING1", Text), input("STRING2", Text)],
        body: BodyNone,
        text: "join %1 %2",
        summary: "Join two values into one string.",
    },
    BlockSpec {
        opcode: "operator_letter_of",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("LETTER", Whole), input("STRING", Text)],
        body: BodyNone,
        text: "letter %1 of %2",
        summary: "One character of a string.",
    },
    BlockSpec {
        opcode: "operator_length",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("STRING", Text)],
        body: BodyNone,
        text: "length of %1",
        summary: "The number of characters in a string.",
    },
    BlockSpec {
        opcode: "operator_contains",
        category: Category::Operators,
        kind: Boolean,
        stability: Vanilla,
        args: &[input("STRING1", Text), input("STRING2", Text)],
        body: BodyNone,
        text: "%1 contains %2?",
        summary: "True when the first string contains the second.",
    },
    BlockSpec {
        opcode: "operator_mod",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM1", Number), input("NUM2", Number)],
        body: BodyNone,
        text: "%1 mod %2",
        summary: "The remainder of a division.",
    },
    BlockSpec {
        opcode: "operator_round",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("NUM", Number)],
        body: BodyNone,
        text: "round %1",
        summary: "Round a number to the nearest integer.",
    },
    BlockSpec {
        opcode: "operator_mathop",
        category: Category::Operators,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("OPERATOR", Menu("math_op")), input("NUM", Number)],
        body: BodyNone,
        text: "%1 of %2",
        summary: "A mathematical function.",
    },
    // ---------------------------------------------------------- Variables --
    BlockSpec {
        opcode: "data_variable",
        category: Category::Variables,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("VARIABLE", Variable)],
        body: BodyNone,
        text: "%1",
        summary: "Read a variable's value.",
    },
    BlockSpec {
        opcode: "data_setvariableto",
        category: Category::Variables,
        kind: Stack,
        stability: Vanilla,
        args: &[field("VARIABLE", Variable), input("VALUE", Text)],
        body: BodyNone,
        text: "set %1 to %2",
        summary: "Assign a value to a variable.",
    },
    BlockSpec {
        opcode: "data_changevariableby",
        category: Category::Variables,
        kind: Stack,
        stability: Vanilla,
        args: &[field("VARIABLE", Variable), input("VALUE", Number)],
        body: BodyNone,
        text: "change %1 by %2",
        summary: "Add to a variable's numeric value.",
    },
    BlockSpec {
        opcode: "data_showvariable",
        category: Category::Variables,
        kind: Stack,
        stability: Vanilla,
        args: &[field("VARIABLE", Variable)],
        body: BodyNone,
        text: "show variable %1",
        summary: "Show a variable monitor on the stage.",
    },
    BlockSpec {
        opcode: "data_hidevariable",
        category: Category::Variables,
        kind: Stack,
        stability: Vanilla,
        args: &[field("VARIABLE", Variable)],
        body: BodyNone,
        text: "hide variable %1",
        summary: "Hide a variable monitor.",
    },
    // -------------------------------------------------------------- Lists --
    BlockSpec {
        opcode: "data_listcontents",
        category: Category::Lists,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("LIST", List)],
        body: BodyNone,
        text: "%1",
        summary: "The entire contents of a list.",
    },
    BlockSpec {
        opcode: "data_addtolist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[input("ITEM", Text), field("LIST", List)],
        body: BodyNone,
        text: "add %1 to %2",
        summary: "Append an item to a list.",
    },
    BlockSpec {
        opcode: "data_deleteoflist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[input("INDEX", Integer), field("LIST", List)],
        body: BodyNone,
        text: "delete %1 of %2",
        summary: "Remove one item from a list.",
    },
    BlockSpec {
        opcode: "data_deletealloflist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[field("LIST", List)],
        body: BodyNone,
        text: "delete all of %1",
        summary: "Empty a list.",
    },
    BlockSpec {
        opcode: "data_insertatlist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[
            input("ITEM", Text),
            input("INDEX", Integer),
            field("LIST", List),
        ],
        body: BodyNone,
        text: "insert %1 at %2 of %3",
        summary: "Insert an item at a position in a list.",
    },
    BlockSpec {
        opcode: "data_replaceitemoflist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[
            input("INDEX", Integer),
            field("LIST", List),
            input("ITEM", Text),
        ],
        body: BodyNone,
        text: "replace item %1 of %2 with %3",
        summary: "Replace one item of a list.",
    },
    BlockSpec {
        opcode: "data_itemoflist",
        category: Category::Lists,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("INDEX", Integer), field("LIST", List)],
        body: BodyNone,
        text: "item %1 of %2",
        summary: "Read one item of a list.",
    },
    BlockSpec {
        opcode: "data_itemnumoflist",
        category: Category::Lists,
        kind: Reporter,
        stability: Vanilla,
        args: &[input("ITEM", Text), field("LIST", List)],
        body: BodyNone,
        text: "item # of %1 in %2",
        summary: "The position of the first matching item.",
    },
    BlockSpec {
        opcode: "data_lengthoflist",
        category: Category::Lists,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("LIST", List)],
        body: BodyNone,
        text: "length of %1",
        summary: "How many items a list holds.",
    },
    BlockSpec {
        opcode: "data_listcontainsitem",
        category: Category::Lists,
        kind: Boolean,
        stability: Vanilla,
        args: &[field("LIST", List), input("ITEM", Text)],
        body: BodyNone,
        text: "%1 contains %2?",
        summary: "True when a list contains a value.",
    },
    BlockSpec {
        opcode: "data_showlist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[field("LIST", List)],
        body: BodyNone,
        text: "show list %1",
        summary: "Show a list monitor on the stage.",
    },
    BlockSpec {
        opcode: "data_hidelist",
        category: Category::Lists,
        kind: Stack,
        stability: Vanilla,
        args: &[field("LIST", List)],
        body: BodyNone,
        text: "hide list %1",
        summary: "Hide a list monitor.",
    },
    // ----------------------------------------------------------- My Blocks --
    BlockSpec {
        opcode: "argument_reporter_string_number",
        category: Category::MyBlocks,
        kind: Reporter,
        stability: Vanilla,
        args: &[field("VALUE", ParamName)],
        body: BodyNone,
        text: "%1",
        summary: "Read a string or number parameter of the enclosing custom block.",
    },
    BlockSpec {
        opcode: "argument_reporter_boolean",
        category: Category::MyBlocks,
        kind: Boolean,
        stability: Vanilla,
        args: &[field("VALUE", ParamName)],
        body: BodyNone,
        text: "%1",
        summary: "Read a boolean parameter of the enclosing custom block.",
    },
    // ---------------------------------------------------------------- Pen --
    BlockSpec {
        opcode: "pen_clear",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "erase all",
        summary: "Erase every pen trail and stamp.",
    },
    BlockSpec {
        opcode: "pen_stamp",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "stamp",
        summary: "Stamp the sprite's costume onto the pen layer.",
    },
    BlockSpec {
        opcode: "pen_penDown",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "pen down",
        summary: "Start drawing when the sprite moves.",
    },
    BlockSpec {
        opcode: "pen_penUp",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "pen up",
        summary: "Stop drawing when the sprite moves.",
    },
    BlockSpec {
        opcode: "pen_setPenColorToColor",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("COLOR", Color)],
        body: BodyNone,
        text: "set pen color to %1",
        summary: "Set the pen colour from an RGB value.",
    },
    BlockSpec {
        opcode: "pen_changePenColorParamBy",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[
            input("COLOR_PARAM", Menu("pen_color_param")),
            input("VALUE", Number),
        ],
        body: BodyNone,
        text: "change pen %1 by %2",
        summary: "Change one pen colour parameter.",
    },
    BlockSpec {
        opcode: "pen_setPenColorParamTo",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[
            input("COLOR_PARAM", Menu("pen_color_param")),
            input("VALUE", Number),
        ],
        body: BodyNone,
        text: "set pen %1 to %2",
        summary: "Set one pen colour parameter.",
    },
    BlockSpec {
        opcode: "pen_changePenSizeBy",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SIZE", Number)],
        body: BodyNone,
        text: "change pen size by %1",
        summary: "Change the pen diameter.",
    },
    BlockSpec {
        opcode: "pen_setPenSizeTo",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SIZE", Number)],
        body: BodyNone,
        text: "set pen size to %1",
        summary: "Set the pen diameter.",
    },
    BlockSpec {
        opcode: "pen_setPenShadeToNumber",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SHADE", Number)],
        body: BodyNone,
        text: "set pen shade to %1",
        summary: "Legacy pen shade block.",
    },
    BlockSpec {
        opcode: "pen_changePenShadeBy",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("SHADE", Number)],
        body: BodyNone,
        text: "change pen shade by %1",
        summary: "Legacy pen shade block.",
    },
    BlockSpec {
        opcode: "pen_setPenHueToNumber",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("HUE", Number)],
        body: BodyNone,
        text: "set pen color to %1",
        summary: "Legacy pen hue block.",
    },
    BlockSpec {
        opcode: "pen_changePenHueBy",
        category: Category::Pen,
        kind: Stack,
        stability: Vanilla,
        args: &[input("HUE", Number)],
        body: BodyNone,
        text: "change pen color by %1",
        summary: "Legacy pen hue block.",
    },
    // -------------------------------------------------------------- Music --
    BlockSpec {
        opcode: "music_playDrumForBeats",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("DRUM", Menu("music_drum")), input("BEATS", Number)],
        body: BodyNone,
        text: "play drum %1 for %2 beats",
        summary: "Play a drum sample.",
    },
    BlockSpec {
        opcode: "music_restForBeats",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("BEATS", Number)],
        body: BodyNone,
        text: "rest for %1 beats",
        summary: "Pause for a number of beats.",
    },
    BlockSpec {
        opcode: "music_playNoteForBeats",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("NOTE", Number), input("BEATS", Number)],
        body: BodyNone,
        text: "play note %1 for %2 beats",
        summary: "Play a note on the current instrument.",
    },
    BlockSpec {
        opcode: "music_setInstrument",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("INSTRUMENT", Menu("music_instrument"))],
        body: BodyNone,
        text: "set instrument to %1",
        summary: "Choose the instrument used by note blocks.",
    },
    BlockSpec {
        opcode: "music_setTempo",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("TEMPO", Number)],
        body: BodyNone,
        text: "set tempo to %1",
        summary: "Set the tempo in beats per minute.",
    },
    BlockSpec {
        opcode: "music_changeTempo",
        category: Category::Music,
        kind: Stack,
        stability: Vanilla,
        args: &[input("TEMPO", Number)],
        body: BodyNone,
        text: "change tempo by %1",
        summary: "Change the tempo in beats per minute.",
    },
    BlockSpec {
        opcode: "music_getTempo",
        category: Category::Music,
        kind: Reporter,
        stability: Vanilla,
        args: &[],
        body: BodyNone,
        text: "tempo",
        summary: "The current tempo in beats per minute.",
    },
];

/// Blocks that are valid script roots but are not written by hand.
pub static PROCEDURES_DEFINITION: &str = "procedures_definition";

/// Look up a block by opcode.
pub fn block(opcode: &str) -> Option<&'static BlockSpec> {
    BLOCKS.iter().find(|b| b.opcode == opcode)
}

/// The nearest opcode to `name`, for "did you mean" hints.
pub fn suggest(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for b in BLOCKS {
        let d = levenshtein(name, b.opcode);
        let limit = (name.len() / 3).max(2);
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, b.opcode));
        }
    }
    best.map(|(_, o)| o)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_are_unique() {
        for (i, a) in BLOCKS.iter().enumerate() {
            for b in &BLOCKS[i + 1..] {
                assert_ne!(a.opcode, b.opcode, "duplicate opcode in catalog");
            }
        }
    }

    #[test]
    fn every_menu_reference_resolves() {
        for b in BLOCKS {
            for a in b.args {
                if let Shape::Menu(id) = a.shape {
                    // Virtual menus are resolved by the compiler rather than MENUS.
                    if matches!(
                        id,
                        "looks_effect"
                            | "sound_effect"
                            | "rotation_style"
                            | "front_back"
                            | "forward_backward"
                            | "number_name"
                            | "stop_option"
                            | "drag_mode"
                            | "math_op"
                            | "greater_than"
                            | "current_menu"
                            | "sensing_of_property"
                    ) {
                        continue;
                    }
                    assert!(menu(id).is_some(), "no catalog menu named `{id}`");
                }
            }
        }
    }

    #[test]
    fn an_unknown_menu_id_is_none_not_a_panic() {
        assert!(menu("no_such_menu").is_none());
    }

    #[test]
    fn extension_menus_accept_reporters() {
        // `pen` and `music` declare `acceptReporters: true`, which is what lets
        // a reporter fill these inputs.
        for id in ["pen_color_param", "music_drum", "music_instrument"] {
            assert!(
                menu(id).expect("extension menu").accept_reporters,
                "`{id}` must accept reporters"
            );
        }
    }

    #[test]
    fn field_and_input_names_are_unique_per_block() {
        for b in BLOCKS {
            for (i, a) in b.args.iter().enumerate() {
                for other in &b.args[i + 1..] {
                    assert_ne!(a.name, other.name, "duplicate argument in {}", b.opcode);
                }
            }
        }
    }

    #[test]
    fn bodies_are_only_on_command_blocks() {
        for b in BLOCKS {
            if b.body != Body::None {
                assert!(
                    matches!(b.kind, Stack | Cap | Hat),
                    "{} takes a body but is not a command block",
                    b.opcode
                );
            }
        }
    }

    #[test]
    fn suggestions_work() {
        assert_eq!(suggest("motion_movestep"), Some("motion_movesteps"));
        assert_eq!(suggest("looks_sya"), Some("looks_say"));
        assert_eq!(suggest("completely_unrelated_thing"), None);
    }
}
