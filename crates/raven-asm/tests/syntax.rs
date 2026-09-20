//! Lexer and parser edge cases.
//!
//! These are the corners that are easy to get subtly wrong and expensive to
//! notice later: escapes, number spellings, comment forms, and every shape of
//! statement and expression the grammar allows.

use raven_asm::ast::*;
use raven_asm::lexer::{Lexer, Tok};
use raven_asm::parser;
use raven_scratch::diag::{Error, Source};

fn parse(src: &str) -> Result<File, Error> {
    let source = Source::new("test.rasm", src);
    parser::parse(&source)
}

fn parse_ok(src: &str) -> File {
    parse(src).unwrap_or_else(|e| panic!("expected this to parse:\n{src}\n{}", e.render()))
}

fn parse_err(src: &str) -> String {
    match parse(src) {
        Ok(_) => panic!("expected a parse error for:\n{src}"),
        Err(e) => e.render(),
    }
}

fn tokens(src: &str) -> Vec<Tok> {
    let source = Source::new("test.rasm", src);
    Lexer::new(&source)
        .tokenize()
        .unwrap_or_else(|e| panic!("{}", e.render()))
        .into_iter()
        .map(|t| t.tok)
        .collect()
}

/// The first script (a top-level statement) in the first target.
fn first_script(file: &File) -> &Stmt {
    file.target
        .as_ref()
        .expect("a target")
        .items
        .iter()
        .find_map(|item| match item {
            Item::Stmt(stmt) => Some(stmt),
            _ => None,
        })
        .expect("a script")
}

/// The statements inside that script's body.
fn first_stmts(file: &File) -> &[Stmt] {
    first_script(file).body.as_deref().unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// Lexing
// ---------------------------------------------------------------------------

#[test]
fn skips_line_and_block_comments() {
    let toks = tokens("a // comment\n/* block\ncomment */ b");
    assert_eq!(
        toks,
        vec![Tok::Ident("a".into()), Tok::Ident("b".into()), Tok::Eof]
    );
}

#[test]
fn block_comments_do_not_nest() {
    // `/* /* */` ends at the first `*/`, exactly like C. The trailing `*/` is
    // then a stray `*`, which is not a token.
    let err = {
        let source = Source::new("test.rasm", "/* /* */ */");
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("unexpected character"), "{err}");
}

#[test]
fn an_unterminated_block_comment_is_an_error() {
    let source = Source::new("test.rasm", "a /* never closed");
    let err = Lexer::new(&source).tokenize().unwrap_err().render();
    assert!(err.contains("unterminated block comment"), "{err}");
}

#[test]
fn decodes_every_string_escape() {
    let toks = tokens(r#""tab:\t nl:\n cr:\r quote:\" slash:\\ nul:\0 star:\u{2605}""#);
    match &toks[0] {
        Tok::Str(s) => {
            assert_eq!(
                s,
                "tab:\t nl:\n cr:\r quote:\" slash:\\ nul:\0 star:\u{2605}"
            );
        }
        other => panic!("expected a string, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_and_malformed_escapes() {
    let err = {
        let source = Source::new("test.rasm", r#""bad \q escape""#);
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("unknown escape"), "{err}");

    let err = {
        let source = Source::new("test.rasm", r#""bad \u2605 escape""#);
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("expected `{` after `\\u`"), "{err}");

    let err = {
        let source = Source::new("test.rasm", r#""bad \u{110000} escape""#);
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("invalid unicode escape"), "{err}");
}

#[test]
fn a_string_may_not_span_a_line() {
    let err = {
        let source = Source::new("test.rasm", "\"one\ntwo\"");
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("unterminated string literal"), "{err}");
}

#[test]
fn numbers_keep_their_spelling() {
    for (src, expected) in [
        ("10", "10"),
        ("1.5", "1.5"),
        (".5", ".5"),
        ("1e3", "1e3"),
        ("1E-3", "1E-3"),
        ("-0.25", "-0.25"),
        ("-3", "-3"),
        ("1.50", "1.50"),
    ] {
        match &tokens(src)[0] {
            Tok::Number(raw) => assert_eq!(raw, expected, "lexing {src}"),
            other => panic!("expected a number for {src}, got {other:?}"),
        }
    }
}

#[test]
fn a_minus_must_introduce_a_number() {
    let err = {
        let source = Source::new("test.rasm", "motion_movesteps(-a);");
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(
        err.contains("`-` may only introduce a negative number literal"),
        "{err}"
    );
    assert!(err.contains("operator_subtract"), "{err}");
}

#[test]
fn skips_a_byte_order_mark() {
    let toks = tokens("\u{feff}abc");
    assert_eq!(toks[0], Tok::Ident("abc".into()));
}

#[test]
fn rejects_characters_the_grammar_has_no_use_for() {
    let err = {
        let source = Source::new("test.rasm", "a $ b");
        Lexer::new(&source).tokenize().unwrap_err().render()
    };
    assert!(err.contains("unexpected character `$`"), "{err}");
}

// ---------------------------------------------------------------------------
// Statements and expressions
// ---------------------------------------------------------------------------

#[test]
fn accepts_every_statement_shape() {
    let file = parse_ok(
        r#"sprite "S" {
    event_whenflagclicked {
        looks_hide;
        looks_show();
        motion_movesteps(10, 20);
        control_repeat(4) { }
        control_if_else(operator_lt(1, 2)) { } else { };
        control_if_else(operator_lt(1, 2)) { } else { }
    }
}"#,
    );

    let stmts = first_stmts(&file);
    assert_eq!(stmts.len(), 6);
    assert_eq!(stmts[0].opcode, "looks_hide");
    assert!(stmts[0].body.is_none());
    assert_eq!(stmts[2].args.len(), 2);
    assert!(stmts[3].body.is_some());
    assert!(stmts[4].else_body.is_some());
    // The optional trailing semicolon after `else { ... }` is accepted either way.
    assert!(stmts[5].else_body.is_some());
}

#[test]
fn reporters_nest_to_any_depth() {
    let file = parse_ok(
        r#"sprite "S" {
    event_whenflagclicked {
        data_setvariableto("x", operator_add(operator_subtract(operator_multiply(1, 2), 3), 4));
    }
}"#,
    );
    let stmt = &first_stmts(&file)[0];
    let Some(Expr::Call(outer)) = stmt.args.get(1) else {
        panic!("expected a nested call");
    };
    assert_eq!(outer.opcode, "operator_add");
    let Some(Expr::Call(inner)) = outer.args.first() else {
        panic!("expected a nested call");
    };
    assert_eq!(inner.opcode, "operator_subtract");
    let Some(Expr::Call(innermost)) = inner.args.first() else {
        panic!("expected a nested call");
    };
    assert_eq!(innermost.opcode, "operator_multiply");
}

#[test]
fn a_bare_identifier_is_not_a_value() {
    let err = parse_err(r#"sprite "S" { event_whenflagclicked { looks_say(score); } }"#);
    assert!(err.contains("`score` is not a value"), "{err}");
    assert!(err.contains("data_variable"), "{err}");
}

#[test]
fn a_statement_needs_a_terminator() {
    let err = parse_err(r#"sprite "S" { event_whenflagclicked { looks_hide } }"#);
    assert!(err.contains("expected `;` or `{`"), "{err}");
}

#[test]
fn else_must_follow_a_block() {
    let err = parse_err(r#"sprite "S" { control_if(foo()) else { } }"#);
    assert!(err.contains("expected `;` or `{`"), "{err}");
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

#[test]
fn declarations_parse_with_every_parameter_kind_and_warp() {
    let file = parse_ok(
        r#"sprite "S" {
    costume "c" = "a.svg" center 32 24;
    costume "d" = "b.svg";
    sound "s" = "c.wav";
    var n = -1.50;
    var text = "hi";
    var flag = true;
    list empty = [];
    list items = [1, "two", false,];
    broadcast "msg";

    proc none() { }
    proc one(a: str) { }
    proc typed(a: str, b: num, c: bool) warp { }

    event_whenflagclicked { }
}"#,
    );

    let items = &file.target.as_ref().unwrap().items;
    let costumes: Vec<&CostumeDecl> = items
        .iter()
        .filter_map(|i| match i {
            Item::Costume(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(costumes.len(), 2);
    assert_eq!(costumes[0].center, Some((32.0, 24.0)));
    assert_eq!(costumes[1].center, None);

    let lists: Vec<&ListDecl> = items
        .iter()
        .filter_map(|i| match i {
            Item::List(l) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(lists[0].init.len(), 0);
    // A trailing comma is allowed.
    assert_eq!(lists[1].init.len(), 3);

    let procs: Vec<&ProcDecl> = items
        .iter()
        .filter_map(|i| match i {
            Item::Proc(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(procs.len(), 3);
    assert!(!procs[0].warp && procs[0].params.is_empty());
    assert_eq!(procs[1].params[0].kind, ParamKind::String);
    assert_eq!(procs[2].params[0].kind, ParamKind::String);
    assert_eq!(procs[2].params[1].kind, ParamKind::Number);
    assert_eq!(procs[2].params[2].kind, ParamKind::Boolean);
    assert!(procs[2].warp);
}

#[test]
fn proc_rejects_duplicate_parameters() {
    let err = parse_err(r#"sprite "S" { proc p(a: str, a: str) { } }"#);
    assert!(err.contains("duplicate parameter `a`"), "{err}");
}

#[test]
fn proc_rejects_a_parameter_without_a_type() {
    let err = parse_err(r#"sprite "S" { proc p(a) { } }"#);
    assert!(err.contains("parameter `a` needs a type"), "{err}");
    assert!(err.contains("`: str`, `: num` or `: bool`"), "{err}");
}

#[test]
fn proc_rejects_unknown_parameter_types() {
    let err = parse_err(r#"sprite "S" { proc p(a: string) { } }"#);
    assert!(err.contains("unknown parameter type `string`"), "{err}");
    assert!(err.contains(": num"), "{err}");
}

#[test]
fn declarations_need_literal_values() {
    let err = parse_err(r#"sprite "S" { var x = operator_add(1, 2); }"#);
    assert!(err.contains("expected a literal value"), "{err}");
}

#[test]
fn global_marks_a_declaration_as_project_wide() {
    let file = parse_ok(
        r#"sprite "S" {
    var local = 0;
    list local_list = [];
    global var shared = 1;
    global list shared_list = [2];
}"#,
    );
    let items = &file.target.as_ref().unwrap().items;

    let vars: Vec<&VarDecl> = items
        .iter()
        .filter_map(|i| match i {
            Item::Var(v) => Some(v),
            _ => None,
        })
        .collect();
    assert_eq!(vars.len(), 2);
    assert!(!vars[0].global, "a bare `var` still belongs to its target");
    assert!(vars[1].global);

    let lists: Vec<&ListDecl> = items
        .iter()
        .filter_map(|i| match i {
            Item::List(l) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(lists.len(), 2);
    assert!(!lists[0].global);
    assert!(lists[1].global);
}

#[test]
fn global_must_introduce_a_variable_or_a_list() {
    let err = parse_err(r#"sprite "S" { global proc p() { } }"#);
    assert!(
        err.contains("expected `var` or `list` after `global`"),
        "{err}"
    );
    assert!(err.contains("global var name = 0;"), "{err}");

    let err = parse_err(r#"sprite "S" { global 5; }"#);
    assert!(
        err.contains("expected `var` or `list` after `global`"),
        "{err}"
    );
}

#[test]
fn a_module_file_parses_before_the_compiler_judges_it() {
    // The parser does not enforce what a module may contain; the compiler does.
    // It must still accept the file so the compiler can report it properly.
    let file = parse_ok(
        r#"use "motion.rasm";

global var shared = 0;
broadcast "ping";

proc helper() { }
"#,
    );
    assert!(file.target.is_none());
    assert_eq!(file.items.len(), 3);
}

// ---------------------------------------------------------------------------
// Files and targets
// ---------------------------------------------------------------------------

#[test]
fn a_module_file_has_no_target() {
    let file = parse_ok(
        r#"use "motion.rasm";

proc helper() { }
"#,
    );
    assert!(file.target.is_none());
    assert_eq!(file.uses.len(), 1);
    assert_eq!(file.items.len(), 1);
}

#[test]
fn a_target_file_may_not_repeat_its_target() {
    let err = parse_err(r#"sprite "A" { } sprite "B" { }"#);
    assert!(err.contains("already declares a target"), "{err}");
    assert!(err.contains("exactly one target"), "{err}");
}

#[test]
fn items_after_a_target_are_rejected() {
    let err = parse_err(r#"sprite "A" { } var x = 1;"#);
    assert!(
        err.contains("may not contain top-level items after the target declaration"),
        "{err}"
    );
}

#[test]
fn an_unclosed_body_is_reported_at_the_opening_token() {
    let err = parse_err(
        r#"sprite "S" {
    event_whenflagclicked {
        looks_hide;
"#,
    );
    assert!(err.contains("unclosed block"), "{err}");
}

#[test]
fn an_empty_file_is_rejected() {
    let err = parse_err("// nothing but a comment\n");
    assert!(err.contains("is empty"), "{err}");
}

#[test]
fn use_requires_a_non_empty_string_path() {
    let err = parse_err(
        r#"use "  ";
sprite "S" { }"#,
    );
    assert!(err.contains("module path must not be empty"), "{err}");

    let err = parse_err("use noun;");
    assert!(
        err.contains("expected a module path as a string literal"),
        "{err}"
    );
}

#[test]
fn the_stage_takes_no_name() {
    let file = parse_ok("stage { costume \"b\" = \"a.svg\"; }");
    let target = file.target.as_ref().unwrap();
    assert_eq!(target.kind, TargetKind::Stage);
    assert_eq!(target.name, "Stage");

    let file = parse_ok(r#"sprite "Player" { costume "c" = "a.svg"; }"#);
    let target = file.target.as_ref().unwrap();
    assert_eq!(target.kind, TargetKind::Sprite);
    assert_eq!(target.name, "Player");
}

#[test]
fn errors_carry_a_line_and_column() {
    let err =
        parse_err("sprite \"S\" {\n    event_whenflagclicked {\n        looks_say(1 2);\n    }\n}");
    assert!(err.contains("test.rasm:3:"), "{err}");
}
