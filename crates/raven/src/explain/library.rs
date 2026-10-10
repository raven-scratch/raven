//! The pages generated from the compiler's own tables.
//!
//! Two of these are the reason the reference cannot drift: `stdlib_section`
//! walks [`crate::stdlib::BINDINGS`] and `menus` walks
//! [`crate::menu::menu_ids`], so a block that is bound by the compiler is
//! described here, and a block that is described here is bound by the compiler.

use crate::menu;
use crate::stdlib;
use raven_scratch::catalog::{self, BlockKind, Shape};

/// `stdlib` — one line per catalog block.
pub fn stdlib_section() -> String {
    let mut out = String::from(
        "\n## stdlib\n\
         # Every catalog block, in catalog order, with the raven spelling the compiler\n\
         # enforces. A name that is not here and not declared in the project does not\n\
         # exist; inventing one is the single most common way to be wrong.\n\
         #\n\
         # <opcode> :: <spelling> | args: <name:shape,…> | result: <ty|none> | \
         body: <none|next|substack|substack+else> | <kind>\n\
         #\n\
         #   spelling    the call to write. `std::module::name(args)`; a shorthand is\n\
         #               written as it appears here, without the module\n\
         #   args        the shape each argument slot accepts; see `types` for what a\n\
         #               shape takes\n\
         #   result      the type a call produces, or `none` for a command block. A\n\
         #               result of `the variable's type` / `the list's element type`\n\
         #               is resolved from the declaration the argument names\n\
         #   body        whether the call takes a `{ … }` substack, and whether it may\n\
         #               also take an `else` one\n\
         #   kind        hat, stack, cap, reporter or boolean\n\
         #   REFUSED     reachable from the catalog and deliberately not callable; the\n\
         #               reason follows on the same line\n\
         #\n\
         # A block whose raven name is unambiguous may also be called without its module\n\
         # (`move_steps(10)` for `motion::move_steps(10)`) as long as no other module\n\
         # has that name and nothing in the project does. Hats are written as\n\
         # `on <name> { … }`; `Syntax` entries are covered by `grammar`.\n",
    );
    for block in catalog::BLOCKS {
        let Some(row) = stdlib::BINDINGS.iter().find(|r| r.opcode == block.opcode) else {
            continue;
        };
        let args: Vec<String> = block
            .args
            .iter()
            .map(|a| format!("{}:{}", a.name, shape_name(a.shape)))
            .collect();
        let result = row
            .binding
            .result()
            .map_or_else(|| "none".to_string(), |v| v.name().to_string());
        let body = match block.body {
            catalog::Body::None => "none",
            catalog::Body::Next => "next",
            catalog::Body::Substack => "substack",
            catalog::Body::SubstackElse => "substack+else",
        };
        let refused = row
            .binding
            .forbidden()
            .map_or(String::new(), |why| format!(" | REFUSED: {why}"));
        out.push_str(&format!(
            "{} :: {} | args: {} | result: {result} | body: {body} | {}{refused}\n",
            block.opcode,
            row.binding.spelling(),
            args.join(", "),
            kind_name(block.kind),
        ));
    }
    out
}

/// `std` — the module directory, and the rules a call follows.
pub fn std_section() -> String {
    let mut out = String::from(
        "\n## std\n\
         # Where a callable name comes from, and the rules every call follows. The\n\
         # blocks themselves are on the `stdlib` page, one line each; this page is the\n\
         # map and the calling convention.\n\
         #\n\
         # # Rules a call follows\n\
         #\n\
         # * `module::name(args)` is the full spelling. A name that is unambiguous\n\
         #   across the whole standard library may be written without its module;\n\
         #   `menus` lists the types a menu argument takes.\n\
         # * A call that produces a value is an expression; a command is a statement\n\
         #   and ends with `;`. A call whose binding says `body: substack` takes a\n\
         #   `{ … }` block after it, and only makes sense as a statement.\n\
         # * A hat is not called: it is declared as `on <name>(args) { … }`, with the\n\
         #   `event_` prefix dropped from the opcode.\n\
         # * `std::pen` and the other extension modules are intrinsic: they need no\n\
         #   `use`, and using one warns (and is refused under `--strict`) because\n\
         #   vanilla Scratch does not have the block.\n\
         # * There is no `std::` name that is not on the `stdlib` page.\n\
         #\n\
         # # The modules\n\
         #\n\
         # module                callable  refused  what it is for\n",
    );
    for id in menu_or_modules() {
        let (callable, refused) = module_counts(id);
        out.push_str(&format!(
            "std::{:<18} {:>8}  {:>7}  {}\n",
            id,
            callable,
            refused,
            module_purpose(id)
        ));
    }

    out.push_str(
        "#\n# # Shorthands\n\
         #\n\
         # A binding may be written without its module. These are the ones the compiler\n\
         # prefers; every other name is called as `module::name`.\n#\n",
    );
    let mut shorthands: Vec<(String, String)> = stdlib::BINDINGS
        .iter()
        .filter_map(|row| match &row.binding {
            stdlib::Binding::Function {
                shorthand: Some(text),
                ..
            } => Some(((*text).to_string(), row.opcode.to_string())),
            _ => None,
        })
        .collect();
    shorthands.sort();
    for (spelling, opcode) in &shorthands {
        out.push_str(&format!("{spelling:<28} {opcode}\n"));
    }

    out.push_str(
        "#\n# # Hats\n\
         #\n\
         # Written as `on <name>(args) { … }`. The opcode keeps its `event_` prefix;\n\
         # the name here is the opcode without it.\n#\n",
    );
    let mut hats: Vec<(String, &str)> = stdlib::BINDINGS
        .iter()
        .filter_map(|row| match &row.binding {
            stdlib::Binding::Hat(name) => Some(((*name).to_string(), row.opcode)),
            _ => None,
        })
        .collect();
    hats.sort();
    for (name, opcode) in &hats {
        out.push_str(&format!("on {name:<28} {opcode}\n"));
    }

    out.push_str(
        "#\n# # Refused\n\
         #\n\
         # Reachable from the catalog and deliberately not callable. Each names the\n\
         # raven spelling to write instead.\n#\n",
    );
    for row in stdlib::BINDINGS {
        if let Some(why) = row.binding.forbidden() {
            out.push_str(&format!(
                "{:<40} {why}\n",
                format!("{} :: {}", row.opcode, row.binding.describe())
            ));
        }
    }
    out
}

/// `menus` — every dropdown as a type.
pub fn menus() -> String {
    let mut out = String::from(
        "\n## menus\n\
         # A dropdown is an enum type. `<MenuType>::<Variant>` is the only spelling for a\n\
         # closed menu; a typo is a compile error. `acceptReporters` menus also take an\n\
         # expression of the same type, so a computed string may fill them.\n\
         # A field menu — the key dropdown on a key hat, the costume dropdown on a\n\
         # switch — is the other way round: it takes the variant and nothing else.\n\
         # The key menu is the one wider than Scratch's editor: the runtime matches a key\n\
         # hat on its own field whatever that field holds, so it carries the thirty-two\n\
         # printable ASCII keys the dropdown has no item for, Key::Exclamation..Key::Tilde,\n\
         # and then the extended runtimes' own backspace, delete, escape, shift and control,\n\
         # which vanilla drops before any block sees them.\n\
         #\n\
         # <menu id> -> <RavenType> :: <variants, or how the values are found>\n",
    );
    for id in menu::menu_ids() {
        let name = menu::type_name(id);
        let description = match menu::domain(id) {
            menu::Domain::Fixed(values) => {
                let variants: Vec<String> = values.iter().map(|v| menu::variant(v)).collect();
                format!("variants: {}", variants.join(", "))
            }
            menu::Domain::Sprites(extras) => {
                let extras: Vec<String> = extras
                    .iter()
                    .map(|v| menu::variant(v).to_string())
                    .collect();
                format!(
                    "a sprite name in the project, or one of: {}",
                    extras.join(", ")
                )
            }
            menu::Domain::Costumes => "the target's costume names".to_string(),
            menu::Domain::Backdrops => "the stage's backdrop names".to_string(),
            menu::Domain::Sounds => "the target's sound names".to_string(),
            menu::Domain::Open => "open: any literal; this menu is not enumerable".to_string(),
        };
        out.push_str(&format!("{id} -> {name} :: {description}\n"));
    }
    out
}

/// `prelude` — the prelude source, verbatim.
pub fn prelude() -> String {
    format!(
        "\n## prelude\n\
         # Imported into every file without being written. Every item is an ordinary raven\n\
         # macro or declaration: it can be read, shadowed or replaced, and a program's own\n\
         # definition wins. Its lowerings are on the `costs` page and its macro rules on\n\
         # the `macros` page.\n\
         # Source: crates/raven/src/prelude.rav\n\n{}",
        include_str!("../prelude.rav")
    )
}

/// Every module a callable name can come from, in the order `stdlib::modules`
/// gives them. A helper because `std_section` walks it twice.
fn menu_or_modules() -> Vec<&'static str> {
    stdlib::modules()
}

/// How many bindings in a module are callable, and how many are refused.
fn module_counts(module: &str) -> (usize, usize) {
    let mut callable = 0;
    let mut refused = 0;
    for row in stdlib::BINDINGS {
        match &row.binding {
            stdlib::Binding::Function { module: m, .. } if *m == module => callable += 1,
            stdlib::Binding::Forbidden { module: m, .. } if *m == module => refused += 1,
            _ => {}
        }
    }
    (callable, refused)
}

/// What a module is for. Written here rather than derived, because the catalog
/// says which sprite a block belongs to and not what the sprite is about.
fn module_purpose(module: &str) -> &'static str {
    match module {
        "control" => "control flow, waits and clones",
        "data" => "Scratch variables and lists — refused, see below",
        "events" => "broadcasts and hats",
        "looks" => "costumes, size, layers, say and think",
        "motion" => "position, direction and gliding",
        "operators" => "arithmetic, text and the maths functions",
        "sensing" => "the world: keys, mouse, timer, answer, of",
        "sound" => "playing sounds and setting the volume",
        "pen" => "the pen extension, intrinsic",
        _ => "extension module",
    }
}

fn shape_name(shape: Shape) -> String {
    match shape {
        Shape::Number | Shape::Positive | Shape::Whole | Shape::Integer | Shape::Angle => {
            "num".to_string()
        }
        Shape::Text => "str|num".to_string(),
        Shape::Bool => "bool".to_string(),
        Shape::Color => "colour".to_string(),
        Shape::Variable => "variable-name".to_string(),
        Shape::List => "list-name".to_string(),
        Shape::Broadcast => "broadcast-name".to_string(),
        Shape::Menu(id) => menu::type_name(id),
        Shape::ParamName => "parameter-name".to_string(),
    }
}

fn kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Hat => "hat",
        BlockKind::Stack => "stack",
        BlockKind::Cap => "cap",
        BlockKind::Reporter => "reporter",
        BlockKind::Boolean => "boolean",
    }
}
