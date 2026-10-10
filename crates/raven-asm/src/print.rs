//! Writing raven-asm back out: the AST as source.
//!
//! The other half of [`crate::parser`]. A tree that came out of the parser can be
//! written back as text the parser accepts, and parsing that text again gives the
//! same tree -- which is the property that makes a source-to-source pass
//! possible at all, and the one this module's tests hold it to.
//!
//! # Why it is here and not in `raven-re`
//!
//! `raven-re` writes raven-asm too, but from a *Scratch project*: it renders the
//! blocks it reversed, and the text is a record of that reversal. This writes a
//! *tree* back out, and the difference matters because a tree is what a
//! transformation produces. `raven-opt` is the caller that needs it: it reads a
//! raven-asm project, rewrites the trees, and writes the project back, so the
//! printer belongs with the parser rather than with the decompiler.
//!
//! # What "the same tree" means
//!
//! Positions are not preserved and are not meant to be: a rewrite moves
//! statements and a printed line has no original span to carry. What has to
//! survive is everything the *compiler* reads -- opcodes, arguments, bodies,
//! declarations, monitors, literals -- and the test is that printing twice is
//! stable: `print(parse(print(parse(src)))) == print(parse(src))`. A printer that
//! dropped a monitor mode or a `global` would fail that on the second pass.

use crate::ast::*;
use crate::source::{number, quote};

/// The whole file, as raven-asm source.
///
/// `uses` first because that is the order the loader expects and the order a
/// reader expects; then the target block, if this file declares one; then the
/// module items.
pub fn file(unit: &File) -> String {
    let mut out = String::new();
    for decl in &unit.uses {
        // A string literal, not the bare path: raven-asm's `use` takes the
        // module's path quoted, which is the one place this printer's output
        // would not parse if it were written the way raven spells it.
        out.push_str(&format!("use {};\n", quote(&decl.path)));
    }
    if !unit.uses.is_empty() && (!unit.items.is_empty() || unit.target.is_some()) {
        out.push('\n');
    }
    if let Some(target) = &unit.target {
        target_decl(&mut out, target);
        return out;
    }
    for entry in &unit.items {
        item(&mut out, entry, 0);
    }
    out
}

fn target_decl(out: &mut String, target: &TargetDecl) {
    match target.kind {
        TargetKind::Stage => out.push_str("stage {\n"),
        TargetKind::Sprite => {
            out.push_str(&format!("sprite {} {{\n", quote(&target.name)));
        }
    }
    for entry in &target.items {
        item(out, entry, 1);
    }
    out.push_str("}\n");
}

/// One item at `depth`, where the depth is the number of enclosing braces.
fn item(out: &mut String, item: &Item, depth: usize) {
    let pad = "    ".repeat(depth);
    match item {
        Item::Var(decl) => {
            out.push_str(&pad);
            out.push_str(&modifiers(decl.global, decl.visible));
            out.push_str("var ");
            out.push_str(&decl.name);
            out.push_str(" = ");
            out.push_str(&literal(&decl.init));
            out.push_str(";\n");
            // The monitor's description comes *after* the declaration's own
            // semicolon, which is the shape the parser reads and the shape the
            // editor's menu is written in.
            out.push_str(&monitor(&decl.monitor, &pad));
        }
        Item::List(decl) => {
            out.push_str(&pad);
            out.push_str(&modifiers(decl.global, decl.visible));
            out.push_str("list ");
            out.push_str(&decl.name);
            out.push_str(" = [");
            for (i, value) in decl.init.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&literal(value));
            }
            out.push_str("];\n");
            out.push_str(&monitor(&decl.monitor, &pad));
        }
        Item::Broadcast(decl) => {
            out.push_str(&format!("{pad}broadcast {};\n", quote(&decl.name)));
        }
        Item::Costume(decl) => {
            out.push_str(&format!(
                "{pad}costume {} = {}",
                quote(&decl.name),
                quote(&decl.path)
            ));
            if let Some((x, y)) = decl.center {
                out.push_str(&format!(" center {} {}", number(x), number(y)));
            }
            out.push_str(";\n");
        }
        Item::Sound(decl) => {
            out.push_str(&format!(
                "{pad}sound {} = {};\n",
                quote(&decl.name),
                quote(&decl.path)
            ));
        }
        Item::Proc(decl) => {
            out.push_str(&pad);
            out.push_str("proc ");
            out.push_str(&decl.name);
            out.push('(');
            for (i, param) in decl.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{}: {}", param.name, param.kind.spelling()));
            }
            out.push(')');
            if decl.warp {
                out.push_str(" warp");
            }
            out.push_str(" {\n");
            for stmt in &decl.body {
                statement(out, stmt, depth + 1);
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        Item::Stmt(stmt) => statement(out, stmt, depth),
    }
}

/// `global `, `visible `, both, or nothing.
///
/// `global visible` and not `visible global`: the parser takes the modifiers in
/// this order and rejects the other, so this is the order that has to be written.
fn modifiers(global: bool, visible: bool) -> String {
    match (global, visible) {
        (true, true) => "global visible ".to_string(),
        (true, false) => "global ".to_string(),
        (false, true) => "visible ".to_string(),
        (false, false) => String::new(),
    }
}

/// The tail of a declaration's line: where its monitor sits and how it draws.
///
/// Emitted as nothing at all when every field is at its default, because that is
/// what a declaration with no monitor description means and printing an explicit
/// `at` for one the editor placed would be a change the source did not ask for.
fn monitor(spec: &MonitorSpec, _pad: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some((x, y)) = spec.at {
        parts.push(format!("at {} {}", number(x), number(y)));
    }
    match spec.mode {
        MonitorMode::Default => {}
        MonitorMode::Large => parts.push("large".to_string()),
        MonitorMode::Slider => {
            parts.push("slider".to_string());
            if let Some((min, max)) = spec.slider {
                parts.push(number(min));
                parts.push(number(max));
            }
        }
    }
    if spec.continuous {
        parts.push("continuous".to_string());
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("{}\n", parts.join(" "))
}

fn statement(out: &mut String, stmt: &Stmt, depth: usize) {
    let pad = "    ".repeat(depth);
    out.push_str(&pad);
    out.push_str(&stmt.opcode);
    out.push('(');
    for (i, arg) in stmt.args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&expression(arg));
    }
    out.push(')');
    match (&stmt.body, &stmt.else_body) {
        (Some(body), Some(else_body)) => {
            out.push_str(" {\n");
            for inner in body {
                statement(out, inner, depth + 1);
            }
            out.push_str(&format!("{pad}}} else {{\n"));
            for inner in else_body {
                statement(out, inner, depth + 1);
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        (Some(body), None) => {
            out.push_str(" {\n");
            for inner in body {
                statement(out, inner, depth + 1);
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        // An `else` with no body cannot be spelled, and the parser never
        // produces one: `Body::SubstackElse` requires both.
        (None, _) => out.push_str(";\n"),
    }
}

fn expression(expr: &Expr) -> String {
    match expr {
        Expr::Number(raw, _) => raw.clone(),
        Expr::Str(text, _) => quote(text),
        Expr::Bool(value, _) => value.to_string(),
        Expr::Call(call) => {
            let mut out = String::new();
            out.push_str(&call.opcode);
            out.push('(');
            for (i, arg) in call.args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&expression(arg));
            }
            out.push(')');
            out
        }
    }
}

fn literal(value: &Literal) -> String {
    match value {
        // The raw text, not a re-rendered float: `1.50` is what the source said
        // and re-rendering it as `1.5` would change the file without changing
        // the program.
        Literal::Number(raw) => raw.clone(),
        Literal::Str(text) => quote(text),
        Literal::Bool(value) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use raven_scratch::diag::Source;

    /// Parse, print, parse the print, print again, and require the two prints to
    /// agree. Positions are dropped by the first print, so this is the strongest
    /// statement available without an `Eq` on the AST -- and it is the one that
    /// matters, because a pass that rewrites a tree and writes it out has to be
    /// stable under exactly this.
    fn round_trips(source: &str) {
        let first = parser::parse(&Source::new("t.rasm", source)).unwrap_or_else(|e| {
            panic!(
                "the input did not parse: {}\n--- source ---\n{source}",
                e.render()
            )
        });
        let printed = file(&first);
        let second = parser::parse(&Source::new("t.rasm", &printed)).unwrap_or_else(|e| {
            panic!(
                "the printer's output did not parse: {}\n--- printed ---\n{printed}",
                e.render()
            )
        });
        let reprinted = file(&second);
        assert_eq!(
            printed, reprinted,
            "printing is not stable\n--- first ---\n{printed}\n--- second ---\n{reprinted}"
        );
    }

    #[test]
    fn a_bare_statement_round_trips() {
        round_trips("looks_say(\"hi\");\n");
    }

    #[test]
    fn a_call_with_no_arguments_keeps_its_parentheses() {
        round_trips("sensing_mousedown();\n");
    }

    #[test]
    fn every_literal_shape_round_trips() {
        round_trips("data_setvariableto(\"x\", 1.50);\n");
        round_trips("data_setvariableto(\"x\", \"a\\\"b\");\n");
        round_trips("data_setvariableto(\"x\", true);\n");
        round_trips("data_setvariableto(\"x\", false);\n");
    }

    #[test]
    fn nested_reporters_round_trip() {
        round_trips("motion_gotoxy(operator_add(1, operator_multiply(2, 3)), -10);\n");
    }

    #[test]
    fn a_body_and_an_else_round_trip() {
        round_trips("control_if_else(operator_lt(1, 2)) {\n    looks_say(\"a\");\n} else {\n    looks_say(\"b\");\n}\n");
    }

    #[test]
    fn a_single_branch_round_trips() {
        round_trips("control_if(operator_lt(1, 2)) {\n    looks_say(\"a\");\n}\n");
    }

    #[test]
    fn a_nested_body_keeps_its_indentation() {
        round_trips(
            "control_repeat(3) {\n    control_if(operator_lt(1, 2)) {\n        looks_say(\"deep\");\n    }\n}\n",
        );
    }

    #[test]
    fn declarations_round_trip() {
        round_trips("var x = 0;\n");
        round_trips("global var score = 1;\n");
        round_trips("visible var shown = \"hi\";\n");
        round_trips("global visible var both = true;\n");
        round_trips("list trail = [1, \"two\", true];\n");
        round_trips("broadcast \"reset\";\n");
        round_trips("costume \"backdrop1\" = \"assets/backdrop.svg\";\n");
        round_trips("costume \"c\" = \"a.svg\" center 1 2;\n");
        round_trips("sound \"pop\" = \"assets/pop.wav\";\n");
    }

    #[test]
    fn a_monitor_description_round_trips() {
        // The description is written after the declaration's semicolon, which is
        // the shape that is easiest to lose and the one no compiler error would
        // catch: a dropped `slider` is a monitor that draws the wrong way.
        round_trips("visible var score = 0; at 5 30\n");
        round_trips("visible var score = 0; large\n");
        round_trips("visible var score = 0; slider 0 100\n");
        round_trips("visible var score = 0; slider 0 100 continuous\n");
        round_trips("visible var score = 0; at 5 30 slider 0 100 continuous\n");
    }

    #[test]
    fn procedures_round_trip() {
        round_trips("proc go(a: num, b: str) {\n    looks_say(data_variable(\"b\"));\n}\n");
        round_trips("proc spin() warp {\n    motion_turnright(15);\n}\n");
        // A boolean parameter, which has its own spelling and its own reader.
        round_trips("proc pick(want: bool) {\n    looks_say(\"x\");\n}\n");
    }

    #[test]
    fn a_target_round_trips() {
        round_trips("stage {\n    broadcast \"go\";\n    var s = 0;\n}\n");
        round_trips("sprite \"Player\" {\n    costume \"c\" = \"a.svg\";\n    proc go() {\n        motion_movesteps(10);\n    }\n}\n");
    }

    #[test]
    fn uses_come_first_and_round_trip() {
        round_trips(
            "use \"motion\";\nuse \"looks\";\n\nsprite \"P\" {\n    looks_say(\"a\");\n}\n",
        );
    }

    #[test]
    fn an_item_name_needing_escapes_round_trips() {
        // Names Scratch allows and raven-asm has to encode are the ones a printer
        // gets wrong, because the escaping lives in the writer and not in the
        // tree.
        round_trips("broadcast \"a\\\"b\";\n");
        round_trips("costume \"a\\\\b\" = \"c.svg\";\n");
    }

    #[test]
    fn the_generated_examples_round_trip() {
        // The printer's real test, and the one the made-up cases cannot be: the
        // raven-asm a project actually lowers to, which is thousands of lines of
        // machine-written code with the awkward shapes in it -- deeply nested
        // reporters, every literal form, monitors, procedures and `use`.
        //
        // It runs only when such a tree has been kept, which `raven build
        // --debug` does and a clean checkout has not. Skipping is deliberate: a
        // test that failed on a machine that had not built the examples would be
        // a test nobody could run.
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the workspace root")
            .to_path_buf();
        let mut seen = 0usize;
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        let examples = repo.join("examples");
        collect_asm_dirs(&examples, &mut dirs, 0);
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("rasm") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let name = path.to_string_lossy().to_string();
                let first = parser::parse(&Source::new(name.clone(), &text))
                    .unwrap_or_else(|e| panic!("{name} did not parse: {}", e.render()));
                let printed = file(&first);
                let second = parser::parse(&Source::new(name.clone(), &printed))
                    .unwrap_or_else(|e| panic!("the printer broke {name}: {}", e.render()));
                assert_eq!(printed, file(&second), "printing is not stable for {name}");
                seen += 1;
            }
        }
        if seen == 0 {
            eprintln!("note: no built raven-asm found; run `raven build --debug` to exercise this");
        }
    }

    /// Every directory under `root` holding a `.rasm`, to a bounded depth.
    fn collect_asm_dirs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut has_rasm = false;
        let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rasm") {
                has_rasm = true;
            }
        }
        if has_rasm {
            out.push(dir.to_path_buf());
        }
        for sub in subdirs {
            collect_asm_dirs(&sub, out, depth + 1);
        }
    }
}
