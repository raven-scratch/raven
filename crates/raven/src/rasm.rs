//! Emitting raven-asm source.
//!
//! raven lowers to raven-asm *as a tree* — the same [`raven_asm::ast`] type that
//! `raven-asm`'s own parser produces — and this module turns that tree back into
//! text. Reusing raven-asm's AST rather than inventing a second one means the
//! emitter cannot drift from the parser: a field that does not exist does not
//! compile.
//!
//! The text is what `raven expand` prints and what `raven build --emit-asm`
//! writes, and the build feeds it straight back to `raven-asm`, so this printer
//! is the whole interface between the two languages. The tests at the bottom
//! round-trip printed output through `raven-asm`'s parser.

use raven_asm::ast::*;
use raven_scratch::diag::Pos;

/// A position for generated code. raven-asm recomputes real positions when it
/// parses the emitted text, so these are only placeholders.
const GENERATED: Pos = Pos::new(0, 0);

// -- builders ---------------------------------------------------------------

#[must_use]
pub fn num(text: impl Into<String>) -> Expr {
    Expr::Number(text.into(), GENERATED)
}

#[must_use]
pub fn str_(text: impl Into<String>) -> Expr {
    Expr::Str(text.into(), GENERATED)
}

#[must_use]
pub fn boolean(value: bool) -> Expr {
    Expr::Bool(value, GENERATED)
}

/// A reporter call.
#[must_use]
pub fn call(opcode: impl Into<String>, args: Vec<Expr>) -> Expr {
    Expr::Call(CallExpr {
        opcode: opcode.into(),
        args,
        pos: GENERATED,
        len: 1,
    })
}

/// A statement with no body.
#[must_use]
pub fn stmt(opcode: impl Into<String>, args: Vec<Expr>) -> Stmt {
    Stmt {
        opcode: opcode.into(),
        args,
        body: None,
        else_body: None,
        pos: GENERATED,
        len: 1,
    }
}

/// A statement with a `{ … }` body.
#[must_use]
pub fn block(opcode: impl Into<String>, args: Vec<Expr>, body: Vec<Stmt>) -> Stmt {
    Stmt {
        opcode: opcode.into(),
        args,
        body: Some(body),
        else_body: None,
        pos: GENERATED,
        len: 1,
    }
}

/// A statement with a `{ … } else { … }` body.
#[must_use]
pub fn block_else(
    opcode: impl Into<String>,
    args: Vec<Expr>,
    body: Vec<Stmt>,
    else_body: Vec<Stmt>,
) -> Stmt {
    Stmt {
        opcode: opcode.into(),
        args,
        body: Some(body),
        else_body: Some(else_body),
        pos: GENERATED,
        len: 1,
    }
}

// -- printing ---------------------------------------------------------------

/// Print a whole file.
#[must_use]
pub fn print(file: &File) -> String {
    render(file, false)
}

/// Print a whole file with a `//@ line col` comment before every item that
/// knows where it came from.
///
/// raven-asm skips comments, so this is still the same program; the markers are
/// how an error found inside generated code is reported against the raven that
/// produced it instead of against the staging `.rasm` the user never wrote.
#[must_use]
pub fn print_marked(file: &File) -> String {
    render(file, true)
}

fn render(file: &File, markers: bool) -> String {
    let mut out = String::new();
    for decl in &file.uses {
        out.push_str(&format!("use {};\n", quote(&decl.path)));
    }
    if !file.uses.is_empty() {
        out.push('\n');
    }
    if let Some(target) = &file.target {
        print_target(&mut out, target, markers);
    } else {
        print_items(&mut out, &file.items, 0, markers);
    }
    out
}

/// The marker line for an item that carries a real position.
fn marker(out: &mut String, pos: Pos, pad: &str) {
    if pos.line != 0 {
        out.push_str(&format!("{pad}//@ {} {}\n", pos.line, pos.col));
    }
}

fn print_target(out: &mut String, target: &TargetDecl, markers: bool) {
    match target.kind {
        TargetKind::Stage => out.push_str("stage {\n"),
        TargetKind::Sprite => out.push_str(&format!("sprite {} {{\n", quote(&target.name))),
    }
    print_items(out, &target.items, 1, markers);
    out.push_str("}\n");
}

fn print_items(out: &mut String, items: &[Item], depth: usize, markers: bool) {
    let pad = indent(depth);
    for item in items {
        if markers {
            let pos = match item {
                Item::Var(var) => var.pos,
                Item::List(list) => list.pos,
                Item::Broadcast(decl) => decl.pos,
                Item::Costume(decl) => decl.path_pos,
                Item::Sound(decl) => decl.path_pos,
                Item::Proc(decl) => decl.pos,
                Item::Stmt(stmt) => stmt.pos,
            };
            marker(out, pos, &pad);
        }
        match item {
            Item::Var(var) => {
                out.push_str(&format!(
                    "{pad}{}{}var {} = {};\n",
                    if var.global { "global " } else { "" },
                    if var.visible { "visible " } else { "" },
                    identifier(&var.name),
                    literal(&var.init)
                ));
            }
            Item::List(list) => {
                let items: Vec<String> = list.init.iter().map(literal).collect();
                out.push_str(&format!(
                    "{pad}{}{}list {} = [{}];\n",
                    if list.global { "global " } else { "" },
                    if list.visible { "visible " } else { "" },
                    identifier(&list.name),
                    items.join(", ")
                ));
            }
            Item::Broadcast(broadcast) => {
                out.push_str(&format!("{pad}broadcast {};\n", quote(&broadcast.name)));
            }
            Item::Costume(costume) => {
                out.push_str(&format!(
                    "{pad}costume {} = {}",
                    quote(&costume.name),
                    quote(&costume.path)
                ));
                if let Some((x, y)) = costume.center {
                    out.push_str(&format!(" center {} {}", number(x), number(y)));
                }
                out.push_str(";\n");
            }
            Item::Sound(sound) => {
                out.push_str(&format!(
                    "{pad}sound {} = {};\n",
                    quote(&sound.name),
                    quote(&sound.path)
                ));
            }
            Item::Proc(proc) => {
                let params: Vec<String> = proc
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", identifier(&p.name), p.kind.spelling()))
                    .collect();
                out.push_str(&format!(
                    "{pad}proc {}({}){} {{\n",
                    identifier(&proc.name),
                    params.join(", "),
                    if proc.warp { " warp" } else { "" }
                ));
                print_stmts(out, &proc.body, depth + 1);
                out.push_str(&format!("{pad}}}\n"));
            }
            Item::Stmt(stmt) => print_stmt(out, stmt, depth),
        }
    }
}

fn print_stmts(out: &mut String, stmts: &[Stmt], depth: usize) {
    for stmt in stmts {
        print_stmt(out, stmt, depth);
    }
}

fn print_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    let pad = indent(depth);
    out.push_str(&pad);
    out.push_str(&stmt.opcode);
    if !stmt.args.is_empty() {
        let args: Vec<String> = stmt.args.iter().map(render_expr).collect();
        out.push_str(&format!("({})", args.join(", ")));
    }
    match (&stmt.body, &stmt.else_body) {
        (Some(body), Some(else_body)) => {
            out.push_str(" {\n");
            print_stmts(out, body, depth + 1);
            out.push_str(&format!("{pad}}} else {{\n"));
            print_stmts(out, else_body, depth + 1);
            out.push_str(&format!("{pad}}}\n"));
        }
        (Some(body), None) => {
            out.push_str(" {\n");
            print_stmts(out, body, depth + 1);
            out.push_str(&format!("{pad}}}\n"));
        }
        // A bodyless statement ends with a semicolon; a body never does.
        (None, _) => out.push_str(";\n"),
    }
}

fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Number(text, _) => text.clone(),
        Expr::Str(text, _) => quote(text),
        Expr::Bool(value, _) => value.to_string(),
        Expr::Call(call) => {
            let args: Vec<String> = call.args.iter().map(render_expr).collect();
            format!("{}({})", call.opcode, args.join(", "))
        }
    }
}

fn literal(literal: &Literal) -> String {
    match literal {
        Literal::Number(text) => text.clone(),
        Literal::Str(text) => quote(text),
        Literal::Bool(value) => value.to_string(),
    }
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

/// An identifier that is legal in raven-asm, whatever the caller handed us.
fn identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            if index == 0 && c.is_ascii_digit() {
                out.push('_');
            }
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// A raven-asm string literal. The escapes are raven-asm's, because raven-asm
/// owns the syntax that has to read them back.
pub use raven_asm::source::quote;

/// A number as raven-asm writes it. Integral values lose the `.0`, because that
/// is what the source meant.
fn number(value: f64) -> String {
    raven_asm::source::number(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_scratch::diag::Source;

    /// Print a file, then hand it back to raven-asm's parser.
    fn round_trip(file: &File) -> raven_asm::ast::File {
        let text = print(file);
        let source = Source::new("generated.rasm", text.clone());
        match raven_asm::parser::parse(&source) {
            Ok(parsed) => parsed,
            Err(error) => panic!(
                "raven-asm rejected the emitted source:\n{text}\n{}",
                error.render()
            ),
        }
    }

    #[test]
    fn a_sprite_round_trips() {
        let file = File {
            uses: Vec::new(),
            target: Some(TargetDecl {
                kind: TargetKind::Sprite,
                name: "Player".into(),
                pos: GENERATED,
                items: vec![
                    Item::Var(VarDecl {
                        monitor: MonitorSpec::default(),
                        visible: false,
                        global: false,
                        name: "score".into(),
                        init: Literal::Number("0".into()),
                        pos: GENERATED,
                    }),
                    Item::Costume(CostumeDecl {
                        name: "idle".into(),
                        path: "assets/idle.svg".into(),
                        center: Some((32.0, 32.0)),
                        pos: GENERATED,
                        path_pos: GENERATED,
                    }),
                    Item::Proc(ProcDecl {
                        name: "hop".into(),
                        params: vec![Param {
                            name: "h".into(),
                            kind: ParamKind::Number,
                        }],
                        warp: true,
                        body: vec![stmt("motion_movesteps", vec![call("arg", Vec::new())])],
                        pos: GENERATED,
                    }),
                    Item::Stmt(block(
                        "event_whenflagclicked",
                        Vec::new(),
                        vec![block_else(
                            "control_if_else",
                            vec![call("operator_gt", vec![num("1"), num("2")])],
                            vec![stmt("looks_say", vec![str_("hi")])],
                            vec![stmt("data_changevariableby", vec![str_("score"), num("1")])],
                        )],
                    )),
                ],
            }),
            items: Vec::new(),
        };
        let text = print(&file);
        assert!(text.contains("sprite \"Player\" {"), "{text}");
        assert!(text.contains("proc hop(h: num) warp {"), "{text}");
        assert!(
            text.contains("costume \"idle\" = \"assets/idle.svg\" center 32 32;"),
            "{text}"
        );
        let parsed = round_trip(&file);
        assert!(parsed.target.is_some());
    }

    #[test]
    fn a_stage_with_globals_round_trips() {
        let file = File {
            uses: Vec::new(),
            target: Some(TargetDecl {
                kind: TargetKind::Stage,
                name: "Stage".into(),
                pos: GENERATED,
                items: vec![
                    Item::Var(VarDecl {
                        monitor: MonitorSpec::default(),
                        visible: false,
                        global: true,
                        name: "best".into(),
                        init: Literal::Number("0".into()),
                        pos: GENERATED,
                    }),
                    Item::List(ListDecl {
                        monitor: MonitorSpec::default(),
                        visible: false,
                        global: true,
                        name: "trail".into(),
                        init: vec![Literal::Number("1".into()), Literal::Str("two".into())],
                        pos: GENERATED,
                    }),
                    Item::Broadcast(BroadcastDecl {
                        name: "reset".into(),
                        pos: GENERATED,
                    }),
                ],
            }),
            items: Vec::new(),
        };
        let text = print(&file);
        assert!(text.contains("global var best = 0;"), "{text}");
        assert!(text.contains("global list trail = [1, \"two\"];"), "{text}");
        assert!(text.contains("broadcast \"reset\";"), "{text}");
        round_trip(&file);
    }

    #[test]
    fn strings_are_escaped_the_way_raven_asm_reads_them() {
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
        let file = File {
            uses: Vec::new(),
            target: Some(TargetDecl {
                kind: TargetKind::Sprite,
                name: "A\"B".into(),
                pos: GENERATED,
                items: vec![Item::Stmt(stmt(
                    "looks_say",
                    vec![str_("line\nbreak\tand \\ slash")],
                ))],
            }),
            items: Vec::new(),
        };
        round_trip(&file);
    }

    #[test]
    fn procedure_calls_print_as_bare_names() {
        let call = call("my_proc", vec![num("1")]);
        assert_eq!(render_expr(&call), "my_proc(1)");
    }

    #[test]
    fn identifiers_that_are_not_legal_are_made_legal() {
        assert_eq!(identifier("ok_1"), "ok_1");
        assert_eq!(identifier("1bad"), "_1bad");
        assert_eq!(identifier("a::b"), "a__b");
        assert_eq!(identifier(""), "_");
    }
}
