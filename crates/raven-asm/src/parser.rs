//! Recursive-descent parser for raven-asm.
//!
//! Every production maps directly onto a Scratch structure; the parser never
//! rewrites or expands source. See `docs/guide/syntax.md`.

use crate::ast::*;
use crate::lexer::{Lexer, Tok, Token};
use raven_scratch::diag::{Diag, Result, Source};

pub fn parse(src: &Source) -> Result<File> {
    let tokens = Lexer::new(src).tokenize()?;
    let mut p = Parser {
        src,
        tokens,
        idx: 0,
    };
    p.file()
}

struct Parser<'a> {
    src: &'a Source,
    tokens: Vec<Token>,
    idx: usize,
}

impl<'a> Parser<'a> {
    // ---------------------------------------------------------------- helpers

    fn peek(&self) -> &Tok {
        &self.tokens[self.idx].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        let i = (self.idx + n).min(self.tokens.len() - 1);
        &self.tokens[i].tok
    }

    fn current(&self) -> &Token {
        &self.tokens[self.idx]
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.idx].clone();
        if self.idx + 1 < self.tokens.len() {
            self.idx += 1;
        }
        t
    }

    fn err_here(&self, msg: impl Into<String>) -> Diag {
        let t = self.current();
        self.src.error(t.pos, msg).span(t.len)
    }

    fn err_at(&self, tok: &Token, msg: impl Into<String>) -> Diag {
        self.src.error(tok.pos, msg).span(tok.len)
    }

    fn expect_punct(&mut self, c: char) -> Result<Token> {
        match self.peek() {
            Tok::Punct(p) if *p == c => Ok(self.advance()),
            other => Err(self
                .err_here(format!("expected `{c}`, found {}", other.describe()))
                .into()),
        }
    }

    fn eat_punct(&mut self, c: char) -> bool {
        if matches!(self.peek(), Tok::Punct(p) if *p == c) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, name: &str) -> bool {
        if matches!(self.peek(), Tok::Ident(i) if i == name) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn is_punct(&self, c: char) -> bool {
        matches!(self.peek(), Tok::Punct(p) if *p == c)
    }

    fn is_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Tok::Ident(i) if i == name)
    }

    fn expect_ident(&mut self, what: &str) -> Result<(String, Token)> {
        match self.peek().clone() {
            Tok::Ident(i) => {
                let t = self.advance();
                Ok((i, t))
            }
            other => Err(self
                .err_here(format!("expected {what}, found {}", other.describe()))
                .into()),
        }
    }

    fn expect_string(&mut self, what: &str) -> Result<(String, Token)> {
        match self.peek().clone() {
            Tok::Str(s) => {
                let t = self.advance();
                Ok((s, t))
            }
            other => Err(self
                .err_here(format!(
                    "expected {what} as a string literal, found {}",
                    other.describe()
                ))
                .into()),
        }
    }

    // ------------------------------------------------------------------ file

    fn file(&mut self) -> Result<File> {
        let mut uses = Vec::new();
        let mut target = None;
        let mut items = Vec::new();

        loop {
            if matches!(self.peek(), Tok::Eof) {
                break;
            }
            if self.is_ident("use") {
                uses.push(self.use_decl()?);
                continue;
            }
            if target.is_some() {
                let t = self.current();
                if matches!(self.peek(), Tok::Ident(i) if i == "stage" || i == "sprite") {
                    return Err(self
                        .err_at(t, "this file already declares a target")
                        .note("a file declares exactly one target")
                        .note("the stage goes in `targets.stage`; each sprite gets its own file, listed under `targets.sprites`")
                        .into());
                }
                return Err(self
                    .err_at(
                        t,
                        "a target file may not contain top-level items after the target declaration",
                    )
                    .note("declare variables, costumes, sounds, procedures and scripts inside the `{ ... }` body")
                    .into());
            }
            if self.is_ident("stage") || self.is_ident("sprite") {
                target = Some(self.target_decl()?);
                continue;
            }
            items.push(self.item()?);
        }

        if target.is_none() && items.is_empty() && uses.is_empty() {
            return Err(Diag::error(format!("`{}` is empty", self.src.display_path())).into());
        }

        Ok(File {
            uses,
            target,
            items,
        })
    }

    fn use_decl(&mut self) -> Result<UseDecl> {
        let kw = self.advance(); // `use`
        let (path, str_tok) = self.expect_string("a module path")?;
        if path.trim().is_empty() {
            return Err(self
                .err_at(&str_tok, "module path must not be empty")
                .into());
        }
        self.expect_punct(';')?;
        Ok(UseDecl { path, pos: kw.pos })
    }

    fn target_decl(&mut self) -> Result<TargetDecl> {
        let kw = self.advance();
        let kw_name = match &kw.tok {
            Tok::Ident(i) => i.clone(),
            _ => unreachable!("checked by caller"),
        };

        let (kind, name, name_pos) = if kw_name == "stage" {
            (TargetKind::Stage, "Stage".to_string(), kw.pos)
        } else {
            let (name, t) = self.expect_string("a sprite name")?;
            if name.is_empty() {
                return Err(self.err_at(&t, "sprite name must not be empty").into());
            }
            (TargetKind::Sprite, name, t.pos)
        };

        self.expect_punct('{')?;
        let mut items = Vec::new();
        while !self.is_punct('}') {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self
                    .err_at(&kw, format!("unclosed `{kw_name}` target body"))
                    .note("add a closing `}`")
                    .into());
            }
            items.push(self.item()?);
        }
        self.expect_punct('}')?;

        Ok(TargetDecl {
            kind,
            name,
            pos: name_pos,
            items,
        })
    }

    // ------------------------------------------------------------------ items

    /// `at X Y`, when a declaration wants to say where its monitor goes.
    ///
    /// `at X Y`, when a declaration wants to say where its monitor goes. It is
    /// read after the declaration's semicolon, so a declaration keeps its plain
    /// shape: `visible var score = 0; at 5 30`.
    fn monitor_at(&mut self) -> Result<Option<(f64, f64)>> {
        if !self.is_ident("at") {
            return Ok(None);
        }
        self.advance();
        let x = self.number("an x position")?;
        let y = self.number("a y position")?;
        Ok(Some((x, y)))
    }

    fn item(&mut self) -> Result<Item> {
        // `visible var x = 0;` / `visible list l = [];` — the monitor for the
        // declaration starts visible, so the value is on the stage at once.
        if self.is_ident("visible") {
            let kw = self.current().clone();
            self.advance();
            return match self.peek() {
                Tok::Ident(next) if next == "var" => {
                    let mut decl = self.var_decl(false)?;
                    decl.visible = true;
                    Ok(Item::Var(decl))
                }
                Tok::Ident(next) if next == "list" => {
                    let mut decl = self.list_decl(false)?;
                    decl.visible = true;
                    Ok(Item::List(decl))
                }
                other => Err(self
                    .err_at(
                        &kw,
                        format!(
                            "expected `var` or `list` after `visible`, found {}",
                            other.describe()
                        ),
                    )
                    .note("write `visible var name = 0;` or `visible list name = [];`")
                    .into()),
            };
        }
        if self.is_ident("global") {
            let kw = self.current().clone();
            return match self.peek_at(1) {
                Tok::Ident(next) if next == "var" => {
                    self.advance();
                    self.var_decl(true).map(Item::Var)
                }
                Tok::Ident(next) if next == "list" => {
                    self.advance();
                    self.list_decl(true).map(Item::List)
                }
                // `global visible var x = 0;` — both modifiers.
                Tok::Ident(next) if next == "visible" => {
                    self.advance();
                    self.advance();
                    match self.peek() {
                        Tok::Ident(what) if what == "var" => {
                            let mut decl = self.var_decl(true)?;
                            decl.visible = true;
                            Ok(Item::Var(decl))
                        }
                        Tok::Ident(what) if what == "list" => {
                            let mut decl = self.list_decl(true)?;
                            decl.visible = true;
                            Ok(Item::List(decl))
                        }
                        other => Err(self
                            .err_at(
                                &kw,
                                format!("expected `var` or `list` after `global visible`, found {}", other.describe()),
                            )
                            .into()),
                    }
                }
                other => Err(self
                    .err_at(
                        &kw,
                        format!("expected `var` or `list` after `global`, found {}", other.describe()),
                    )
                    .note("`global` marks a declaration that belongs to the stage, so it is visible to every sprite")
                    .note("write `global var name = 0;` or `global list name = [];`")
                    .into()),
            };
        }
        if self.is_ident("var") {
            return self.var_decl(false).map(Item::Var);
        }
        if self.is_ident("list") {
            return self.list_decl(false).map(Item::List);
        }
        if self.is_ident("broadcast") {
            return self.broadcast_decl().map(Item::Broadcast);
        }
        if self.is_ident("costume") {
            return self.costume_decl().map(Item::Costume);
        }
        if self.is_ident("sound") {
            return self.sound_decl().map(Item::Sound);
        }
        if self.is_ident("proc") {
            return self.proc_decl().map(Item::Proc);
        }
        if self.is_ident("stage") || self.is_ident("sprite") {
            let t = self.current();
            return Err(self
                .err_at(
                    t,
                    "a project may only declare one stage and each sprite lives in its own file",
                )
                .note("list every sprite file under `[targets] sprites` in raven-asm.toml")
                .into());
        }
        self.stmt().map(Item::Stmt)
    }

    fn var_decl(&mut self, global: bool) -> Result<VarDecl> {
        let kw = self.advance();
        let (name, name_tok) = self.expect_ident("a variable name")?;
        self.expect_punct('=')?;
        let init = self.literal()?;
        self.expect_punct(';')?;
        Ok(VarDecl {
            visible: false,
            at: self.monitor_at()?,
            global,
            name,
            init,
            pos: name_tok.pos.or_pos(kw.pos),
        })
    }

    fn list_decl(&mut self, global: bool) -> Result<ListDecl> {
        let kw = self.advance();
        let (name, name_tok) = self.expect_ident("a list name")?;
        self.expect_punct('=')?;
        self.expect_punct('[')?;
        let mut init = Vec::new();
        while !self.is_punct(']') {
            init.push(self.literal()?);
            if !self.eat_punct(',') {
                break;
            }
        }
        self.expect_punct(']')?;
        self.expect_punct(';')?;
        Ok(ListDecl {
            visible: false,
            at: self.monitor_at()?,
            global,
            name,
            init,
            pos: name_tok.pos.or_pos(kw.pos),
        })
    }

    fn broadcast_decl(&mut self) -> Result<BroadcastDecl> {
        let kw = self.advance();
        let (name, _) = self.expect_string("a broadcast message name")?;
        self.expect_punct(';')?;
        Ok(BroadcastDecl { name, pos: kw.pos })
    }

    fn costume_decl(&mut self) -> Result<CostumeDecl> {
        let kw = self.advance();
        let (name, _) = self.expect_string("a costume name")?;
        self.expect_punct('=')?;
        let (path, path_tok) = self.expect_string("a costume file path")?;
        let mut center = None;
        if self.is_ident("center") {
            self.advance();
            let x = self.number("the costume's rotation centre X")?;
            let y = self.number("the costume's rotation centre Y")?;
            center = Some((x, y));
        }
        self.expect_punct(';')?;
        Ok(CostumeDecl {
            name,
            path,
            center,
            pos: kw.pos,
            path_pos: path_tok.pos,
        })
    }

    fn sound_decl(&mut self) -> Result<SoundDecl> {
        let kw = self.advance();
        let (name, _) = self.expect_string("a sound name")?;
        self.expect_punct('=')?;
        let (path, path_tok) = self.expect_string("a sound file path")?;
        self.expect_punct(';')?;
        Ok(SoundDecl {
            name,
            path,
            pos: kw.pos,
            path_pos: path_tok.pos,
        })
    }

    fn proc_decl(&mut self) -> Result<ProcDecl> {
        let kw = self.advance();
        let (name, name_tok) = self.expect_ident("a procedure name")?;
        self.expect_punct('(')?;
        let mut params = Vec::new();
        while !self.is_punct(')') {
            let (pname, ptok) = self.expect_ident("a parameter name")?;

            // Every parameter carries its type. An untyped parameter would mean
            // "string" by omission, which is exactly the sort of thing a reader
            // cannot see and a dev cannot check.
            if !self.is_punct(':') {
                let found = self.peek().describe();
                return Err(self
                    .err_at(
                        self.current(),
                        format!("parameter `{pname}` needs a type, found {found}"),
                    )
                    .note("write `: str`, `: num` or `: bool` after every parameter name")
                    .note("`str` and `num` are read with `argument_reporter_string_number`; `bool` is read with `argument_reporter_boolean`")
                    .into());
            }
            self.advance();
            let (ty, ty_tok) = self.expect_ident("a parameter type (`str`, `num` or `bool`)")?;
            let kind = match ty.as_str() {
                "str" => ParamKind::String,
                "num" => ParamKind::Number,
                "bool" => ParamKind::Boolean,
                other => {
                    return Err(self
                        .err_at(&ty_tok, format!("unknown parameter type `{other}`"))
                        .note("the three types are `: str`, `: num` and `: bool`")
                        .note("they are Scratch's own input shapes: string, number and boolean")
                        .into())
                }
            };

            if params.iter().any(|p: &Param| p.name == pname) {
                return Err(self
                    .err_at(&ptok, format!("duplicate parameter `{pname}`"))
                    .into());
            }
            params.push(Param { name: pname, kind });
            if !self.eat_punct(',') {
                break;
            }
        }
        self.expect_punct(')')?;

        let warp = self.eat_ident("warp");
        let body = self.block()?;

        Ok(ProcDecl {
            name,
            params,
            warp,
            body,
            pos: name_tok.pos.or_pos(kw.pos),
        })
    }

    // ------------------------------------------------------------- statements

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_punct('{')?;
        let mut stmts = Vec::new();
        while !self.is_punct('}') {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self.err_here("unclosed block; expected `}`").into());
            }
            stmts.push(self.stmt()?);
        }
        self.expect_punct('}')?;
        Ok(stmts)
    }

    fn stmt(&mut self) -> Result<Stmt> {
        let (opcode, name_tok) = self.expect_ident("a Scratch block opcode")?;
        let mut args = Vec::new();
        if self.eat_punct('(') {
            while !self.is_punct(')') {
                args.push(self.expr()?);
                if !self.eat_punct(',') {
                    break;
                }
            }
            self.expect_punct(')')?;
        }

        if self.eat_punct(';') {
            return Ok(Stmt {
                opcode,
                args,
                body: None,
                else_body: None,
                pos: name_tok.pos,
                len: name_tok.len,
            });
        }

        if self.is_punct('{') {
            let body = self.block()?;
            let mut else_body = None;
            // `else { ... }` is only meaningful for `control_if_else`, but the
            // compiler reports that; the parser accepts it for any block.
            if self.is_ident("else") {
                self.advance();
                else_body = Some(self.block()?);
            }
            if self.is_punct(';') {
                self.advance();
            }
            return Ok(Stmt {
                opcode,
                args,
                body: Some(body),
                else_body,
                pos: name_tok.pos,
                len: name_tok.len,
            });
        }

        Err(self
            .err_here(format!(
                "expected `;` or `{{` after `{opcode}`, found {}",
                self.peek().describe()
            ))
            .note("every raven-asm statement ends with `;`, or opens a substack with `{ ... }`")
            .into())
    }

    // ------------------------------------------------------------ expressions

    fn expr(&mut self) -> Result<Expr> {
        let tok = self.current().clone();
        match &tok.tok {
            Tok::Number(raw) => {
                self.advance();
                Ok(Expr::Number(raw.clone(), tok.pos))
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Expr::Str(s.clone(), tok.pos))
            }
            Tok::Ident(name) => match name.as_str() {
                "true" | "false" => {
                    self.advance();
                    Ok(Expr::Bool(name == "true", tok.pos))
                }
                _ => {
                    if matches!(self.peek_at(1), Tok::Punct('(')) {
                        let (opcode, name_tok) = self.expect_ident("a block opcode")?;
                        self.expect_punct('(')?;
                        let mut args = Vec::new();
                        while !self.is_punct(')') {
                            args.push(self.expr()?);
                            if !self.eat_punct(',') {
                                break;
                            }
                        }
                        self.expect_punct(')')?;
                        Ok(Expr::Call(CallExpr {
                            opcode,
                            args,
                            pos: name_tok.pos,
                            len: name_tok.len,
                        }))
                    } else {
                        Err(self
                                .err_at(
                                    &tok,
                                    format!("`{name}` is not a value"),
                                )
                                .note(format!(
                                    "to read a variable use `data_variable(\"{name}\")`; to call a block use `{name}(...)`"
                                ))
                                .into())
                    }
                }
            },
            other => Err(self
                .err_at(
                    &tok,
                    format!(
                        "expected an argument expression, found {}",
                        other.describe()
                    ),
                )
                .into()),
        }
    }

    fn literal(&mut self) -> Result<Literal> {
        let tok = self.current().clone();
        match &tok.tok {
            Tok::Number(raw) => {
                self.advance();
                Ok(Literal::Number(raw.clone()))
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Literal::Str(s.clone()))
            }
            Tok::Ident(name) if name == "true" || name == "false" => {
                self.advance();
                Ok(Literal::Bool(name == "true"))
            }
            other => Err(self
                .err_at(
                    &tok,
                    format!("expected a literal value, found {}", other.describe()),
                )
                .note("variable and list initial values must be literals: numbers, strings or `true`/`false`")
                .into()),
        }
    }

    fn number(&mut self, what: &str) -> Result<f64> {
        let tok = self.current().clone();
        match &tok.tok {
            Tok::Number(raw) => {
                self.advance();
                raw.parse::<f64>().map_err(|_| {
                    self.err_at(&tok, format!("`{raw}` is not a valid number"))
                        .into()
                })
            }
            other => Err(self
                .err_at(
                    &tok,
                    format!("expected {what} as a number, found {}", other.describe()),
                )
                .into()),
        }
    }
}
