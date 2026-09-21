//! The raven parser: a recursive-descent parser over one file's tokens.
//!
//! It produces [`crate::ast::File`] and nothing else. Everything that can be
//! decided from the token stream alone is decided here — including the rewrite
//! that keeps the surface grammar small:
//!
//! * `while c { … }` and `for i in a..b { … }` become calls to `while_loop` and
//!   `for_range`.
//!
//! `x += e` is not rewritten into a macro, because a Scratch variable and a VMS
//! cell change by different blocks; it becomes the core node
//! [`crate::ast::Stmt::CompoundAssign`]. Nothing else about the language is
//! visible at this layer, which is why the rest of the compiler never has to ask
//! whether a construct was written with sugar.

use raven_scratch::diag::{Result, Source};

use crate::ast::*;
use crate::diag::Span;
use crate::lexer::{self, InterpPart as LexPart, Kw, Tok, Token, P};
use crate::ty::{Scalar, Ty};

/// Parse one file.
pub fn parse(source: &Source) -> Result<File> {
    let lexed = lexer::lex(source)?;
    let mut parser = Parser {
        src: source,
        toks: lexed.tokens,
        at: 0,
    };
    parser.file()
}

struct Parser<'a> {
    src: &'a Source,
    toks: Vec<Token>,
    at: usize,
}

impl Parser<'_> {
    // -- token helpers ----------------------------------------------------

    fn peek(&self) -> &Tok {
        &self.toks[self.at.min(self.toks.len() - 1)].tok
    }

    fn peek_n(&self, n: usize) -> &Tok {
        &self.toks[(self.at + n).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.at.min(self.toks.len() - 1)].span
    }

    fn span_n(&self, n: usize) -> Span {
        self.toks[(self.at + n).min(self.toks.len() - 1)].span
    }

    fn bump(&mut self) -> Token {
        let token = self.toks[self.at.min(self.toks.len() - 1)].clone();
        if self.at < self.toks.len() - 1 {
            self.at += 1;
        }
        token
    }

    fn at_punct(&self, p: P) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn at_punct_n(&self, n: usize, p: P) -> bool {
        matches!(self.peek_n(n), Tok::Punct(q) if *q == p)
    }

    fn at_kw(&self, k: Kw) -> bool {
        matches!(self.peek(), Tok::Kw(q) if *q == k)
    }

    /// `a` immediately followed by `b` with no space between: this is what tells
    /// `x += 1` from `x + = 1`.
    fn adjacent(&self, a: Span, b: Span) -> bool {
        a.pos.line == b.pos.line && a.pos.col + a.len == b.pos.col
    }

    fn eat(&mut self, p: P) -> bool {
        if self.at_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, k: Kw) -> bool {
        if self.at_kw(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: P) -> Result<Token> {
        if self.at_punct(p) {
            Ok(self.bump())
        } else {
            Err(self.expected(&format!("`{}`", p.text())))
        }
    }

    fn expect_kw(&mut self, k: Kw) -> Result<Token> {
        if self.at_kw(k) {
            Ok(self.bump())
        } else {
            Err(self.expected(&format!("`{}`", k.text())))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<Ident> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let token = self.bump();
                Ok(Ident::new(name, token.span))
            }
            other => Err(self.expected(&format!("{what}, found {}", other.describe()))),
        }
    }

    /// A path segment after `::`. A keyword is allowed here, because a name that
    /// follows `::` can only be an item — which is how `events::broadcast` and
    /// `control::while` are spelled.
    fn expect_segment(&mut self) -> Result<Ident> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let token = self.bump();
                Ok(Ident::new(name, token.span))
            }
            Tok::Kw(kw) => {
                let token = self.bump();
                Ok(Ident::new(kw.text(), token.span))
            }
            other => Err(self.expected(&format!("a path segment, found {}", other.describe()))),
        }
    }

    fn expect_str(&mut self, what: &str) -> Result<(String, Span)> {
        match self.peek().clone() {
            Tok::Str(text) => {
                let token = self.bump();
                Ok((text, token.span))
            }
            other => Err(self.expected(&format!("{what}, found {}", other.describe()))),
        }
    }

    fn expected(&self, what: &str) -> raven_scratch::diag::Error {
        let span = self.span();
        let mut error: raven_scratch::diag::Error = self
            .src
            .error(span.pos, format!("expected {what}"))
            .span(span.len.max(1))
            .into();
        if let Some(hint) = self.hint() {
            error = error.note(hint);
        }
        error
    }

    fn hint(&self) -> Option<String> {
        match self.peek() {
            Tok::Eof => Some("the file ends here".to_string()),
            _ => None,
        }
    }

    fn err(&self, span: Span, message: impl Into<String>) -> raven_scratch::diag::Error {
        self.src
            .error(span.pos, message)
            .span(span.len.max(1))
            .into()
    }

    // -- file -------------------------------------------------------------

    fn file(&mut self) -> Result<File> {
        let start = self.span();
        let mut uses = Vec::new();
        while self.at_kw(Kw::Use) {
            uses.push(self.use_decl()?);
        }
        let mut items = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            items.push(self.item()?);
        }
        if items.is_empty() && uses.is_empty() {
            return Err(self.err(start, "this file is empty"));
        }
        Ok(File {
            path: self.src.path.clone(),
            uses,
            items,
            span: start,
        })
    }

    fn use_decl(&mut self) -> Result<UseDecl> {
        let start = self.expect_kw(Kw::Use)?.span;
        let mut segments = vec![self.expect_ident("a module path")?];
        let mut names = None;
        loop {
            if !self.at_punct(P::ColonColon) {
                break;
            }
            if matches!(self.peek_n(1), Tok::Punct(P::LBrace)) {
                self.bump();
                self.bump();
                let mut list = Vec::new();
                if !self.at_punct(P::RBrace) {
                    loop {
                        list.push(self.expect_ident("an item name")?);
                        if !self.eat(P::Comma) {
                            break;
                        }
                    }
                }
                self.expect(P::RBrace)?;
                names = Some(list);
                break;
            }
            self.bump();
            segments.push(self.expect_ident("a module path segment")?);
        }
        let semi = self.expect(P::Semi)?;
        let end = Span::new(semi.span.pos, 1);
        let span = Span::new(
            start.pos,
            if end.pos.line == start.pos.line {
                (end.pos.col - start.pos.col).max(1)
            } else {
                1
            },
        );
        let path = Path { span, segments };
        Ok(UseDecl { path, names, span })
    }

    fn item(&mut self) -> Result<Item> {
        let public = self.eat_kw(Kw::Pub);
        if self.at_kw(Kw::Stage) || self.at_kw(Kw::Sprite) {
            if public {
                return Err(self.err(self.span(), "`pub` does not apply to a target"));
            }
            return Ok(Item::Target(self.target()?));
        }
        if self.at_kw(Kw::Use) {
            return Err(self.err(
                self.span(),
                "`use` must come before every other item in a file",
            ));
        }
        let span = self.span();
        let item = match self.peek().clone() {
            Tok::Kw(Kw::Var) => Item::Var(self.var_decl(public)?),
            Tok::Kw(Kw::Const) => Item::Const(self.const_decl(public)?),
            Tok::Kw(Kw::Struct) => Item::Struct(self.struct_decl(public)?),
            Tok::Kw(Kw::Watch) => {
                if public {
                    return Err(self.err(span, "`pub` does not apply to a `watch`"));
                }
                Item::Watch(self.watch_decl()?)
            }
            Tok::Kw(Kw::Broadcast) => {
                if public {
                    return Err(self.err(span, "`pub` does not apply to a broadcast"));
                }
                Item::Broadcast(self.broadcast_decl()?)
            }
            Tok::Kw(Kw::Costume) => {
                if public {
                    return Err(self.err(span, "`pub` does not apply to a costume"));
                }
                Item::Costume(self.costume_decl()?)
            }
            Tok::Kw(Kw::Sound) => {
                if public {
                    return Err(self.err(span, "`pub` does not apply to a sound"));
                }
                Item::Sound(self.sound_decl()?)
            }
            Tok::Kw(Kw::Proc) => Item::Proc(self.proc_decl(public)?),
            Tok::Kw(Kw::Fn) => Item::Fn(self.fn_decl(public)?),
            Tok::Kw(Kw::Macro) => Item::Macro(self.macro_decl(public)?),
            Tok::Kw(Kw::On) => {
                if public {
                    return Err(self.err(span, "`pub` does not apply to a script"));
                }
                Item::Script(self.script()?)
            }
            other => {
                let mut error = self.err(
                    span,
                    format!("expected an item, found {}", other.describe()),
                );
                if matches!(other, Tok::Ident(_)) && matches!(self.peek_n(1), Tok::Punct(P::LParen))
                {
                    error = error.note(
                        "a top-level call is not a script; wrap it in `on flag_clicked { … }`",
                    );
                }
                return Err(error);
            }
        };
        Ok(item)
    }

    fn target(&mut self) -> Result<TargetDecl> {
        let start = self.span();
        let (kind, name) = if self.eat_kw(Kw::Stage) {
            (TargetKind::Stage, "Stage".to_string())
        } else {
            self.expect_kw(Kw::Sprite)?;
            let (name, _) = self.expect_str("a sprite name")?;
            (TargetKind::Sprite, name)
        };
        let items = self.braced_items()?;
        Ok(TargetDecl {
            kind,
            name,
            items,

            span: Span::new(start.pos, 6),
        })
    }

    fn braced_items(&mut self) -> Result<Vec<Item>> {
        self.expect(P::LBrace)?;
        let mut items = Vec::new();
        while !self.at_punct(P::RBrace) {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self.err(self.span(), "unclosed `{`"));
            }
            if self.at_kw(Kw::Stage) || self.at_kw(Kw::Sprite) {
                return Err(self.err(
                    self.span(),
                    "a target cannot be declared inside another target",
                ));
            }
            items.push(self.item()?);
        }
        self.expect(P::RBrace)?;
        Ok(items)
    }

    // -- declarations -----------------------------------------------------

    fn ty(&mut self) -> Result<Ty> {
        if self.at_kw(Kw::List) {
            self.bump();
            self.expect(P::Lt)?;
            let element = self.scalar("an element type")?;
            self.expect(P::Gt)?;
            return Ok(Ty::List(element));
        }
        if self.at_kw(Kw::Map) {
            self.bump();
            self.expect(P::Lt)?;
            let key = self.scalar("a key type")?;
            self.expect(P::Comma)?;
            let value = self.scalar("a value type")?;
            self.expect(P::Gt)?;
            return Ok(Ty::Map(key, value));
        }
        if let Tok::Ident(name) = self.peek().clone() {
            self.bump();
            return Ok(Ty::Struct(crate::ty::intern_struct(&name)));
        }
        Ok(match self.scalar("a type")? {
            Scalar::Num => Ty::Num,
            Scalar::Str => Ty::Str,
            Scalar::Bool => Ty::Bool,
        })
    }

    /// `watch score, best;`
    fn watch_decl(&mut self) -> Result<WatchDecl> {
        let start = self.expect_kw(Kw::Watch)?.span;
        let mut names = Vec::new();
        loop {
            names.push(self.expect_ident("a variable or list name")?);
            if !self.eat(P::Comma) {
                break;
            }
        }
        let end = self.expect(P::Semi)?;
        Ok(WatchDecl {
            names,

            span: Span::new(
                start.pos,
                end.span.pos.col.saturating_sub(start.pos.col) + 1,
            ),
        })
    }

    /// `struct Point { x: num, y: num }`
    fn struct_decl(&mut self, public: bool) -> Result<StructDecl> {
        let start = self.expect_kw(Kw::Struct)?.span;
        let name = self.expect_ident("a struct name")?;
        self.expect(P::LBrace)?;
        let mut fields: Vec<StructField> = Vec::new();
        while !self.at_punct(P::RBrace) {
            let field = self.expect_ident("a field name")?;
            self.expect(P::Colon)?;
            let ty = self.ty()?;
            if ty.is_list() {
                return Err(self
                    .err(
                        field.span,
                        format!(
                            "`{}` is a `{}`, which cannot be a field",
                            field.name,
                            ty.name()
                        ),
                    )
                    .note("a struct is a fixed run of one cell per scalar field")
                    .note("a list is a Scratch list; declare it beside the struct instead"));
            }
            if fields.iter().any(|f| f.name.name == field.name) {
                return Err(self.err(field.span, format!("`{}` is declared twice", field.name)));
            }
            fields.push(StructField { name: field, ty });
            if !self.eat(P::Comma) && !self.eat(P::Semi) {
                break;
            }
        }
        let end = self.expect(P::RBrace)?;
        if fields.is_empty() {
            return Err(self.err(start, format!("`{}` has no fields", name.name)));
        }
        Ok(StructDecl {
            public,
            name,
            fields,

            span: Span::new(
                start.pos,
                end.span.pos.col.saturating_sub(start.pos.col) + 1,
            ),
        })
    }

    fn scalar(&mut self, what: &str) -> Result<Scalar> {
        match self.peek().clone() {
            Tok::Kw(Kw::Num) => {
                self.bump();
                Ok(Scalar::Num)
            }
            Tok::Kw(Kw::Str) => {
                self.bump();
                Ok(Scalar::Str)
            }
            Tok::Kw(Kw::Bool) => {
                self.bump();
                Ok(Scalar::Bool)
            }
            other => Err(self.expected(&format!("{what}, found {}", other.describe()))),
        }
    }

    fn var_decl(&mut self, public: bool) -> Result<VarDecl> {
        let start = self.expect_kw(Kw::Var)?.span;
        // `var $i: num = 0;` inside a macro names a variable the caller owns:
        // the `$` marks it as a substitution rather than a hygienic temporary.
        let name = match self.peek().clone() {
            Tok::Param(raw) => {
                let token = self.bump();
                Ident::new(format!("${raw}"), token.span)
            }
            _ => self.expect_ident("a variable name")?,
        };
        if !self.at_punct(P::Colon) {
            return Err(self
                .err(name.span, format!("`{}` needs a type", name.name))
                .note(
                    "a declaration is `var name: type = value;`, for example `var score: num = 0;`",
                ));
        }
        self.bump();
        let ty = self.ty()?;
        self.expect(P::Eq)?;
        let init = self.initializer()?;
        let semi = self.expect(P::Semi)?;
        Ok(VarDecl {
            public,
            name,
            ty,
            init,

            span: Span::new(
                start.pos,
                semi.span.pos.col.saturating_sub(start.pos.col) + 1,
            ),
        })
    }

    fn const_decl(&mut self, public: bool) -> Result<ConstDecl> {
        let start = self.expect_kw(Kw::Const)?.span;
        let name = self.expect_ident("a constant name")?;
        if !self.at_punct(P::Colon) {
            return Err(self
                .err(name.span, format!("`{}` needs a type", name.name))
                .note("a constant is `const NAME: type = literal;`, for example `const MAX: num = 10;`"));
        }
        self.bump();
        let ty = self.ty()?;
        self.expect(P::Eq)?;
        let value = self.literal("a literal")?;
        let semi = self.expect(P::Semi)?;
        Ok(ConstDecl {
            public,
            name,
            ty,
            value,

            span: Span::new(
                start.pos,
                semi.span.pos.col.saturating_sub(start.pos.col) + 1,
            ),
        })
    }

    fn broadcast_decl(&mut self) -> Result<BroadcastDecl> {
        let start = self.expect_kw(Kw::Broadcast)?.span;
        let (name, _) = self.expect_str("a broadcast name")?;
        self.expect(P::Semi)?;
        Ok(BroadcastDecl { name, span: start })
    }

    fn costume_decl(&mut self) -> Result<CostumeDecl> {
        let start = self.expect_kw(Kw::Costume)?.span;
        let (name, _) = self.expect_str("a costume name")?;
        self.expect(P::Eq)?;
        let (path, path_span) = self.expect_str("an asset path")?;
        let mut center = None;
        if let Tok::Ident(word) = self.peek().clone() {
            if word == "center" {
                self.bump();
                let (x, _) = self.expect_str("a centre x")?;
                let (y, _) = self.expect_str("a centre y")?;
                center = Some((x, y));
            }
        }
        self.expect(P::Semi)?;
        Ok(CostumeDecl {
            name,
            path,
            center,
            span: start,
            path_span,
        })
    }

    fn sound_decl(&mut self) -> Result<SoundDecl> {
        let start = self.expect_kw(Kw::Sound)?.span;
        let (name, _) = self.expect_str("a sound name")?;
        self.expect(P::Eq)?;
        let (path, path_span) = self.expect_str("an asset path")?;
        self.expect(P::Semi)?;
        Ok(SoundDecl {
            name,
            path,
            span: start,
            path_span,
        })
    }

    fn proc_decl(&mut self, public: bool) -> Result<ProcDecl> {
        let start = self.expect_kw(Kw::Proc)?.span;
        let name = self.expect_ident("a procedure name")?;
        self.expect(P::LParen)?;
        let mut params = Vec::new();
        if !self.at_punct(P::RParen) {
            loop {
                let param_span = self.span();
                let param_name = self.expect_ident("a parameter name")?;
                self.expect(P::Colon)?;
                let ty = self.scalar("a parameter type")?;
                params.push(Param {
                    name: param_name,
                    ty,
                    span: param_span,
                });
                if !self.eat(P::Comma) {
                    break;
                }
            }
        }
        self.expect(P::RParen)?;
        self.check_duplicates(
            &params.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "parameter",
        )?;
        let ret = if self.eat(P::Arrow) {
            Some(self.ty()?)
        } else {
            None
        };
        let warp = self.eat_kw(Kw::Warp);
        let body = self.block()?;
        Ok(ProcDecl {
            public,
            name,
            params,
            ret,
            warp,
            body,

            span: start,
        })
    }

    fn fn_decl(&mut self, public: bool) -> Result<FnDecl> {
        let start = self.expect_kw(Kw::Fn)?.span;
        let name = self.expect_ident("a function name")?;
        self.expect(P::LParen)?;
        let mut params = Vec::new();
        if !self.at_punct(P::RParen) {
            loop {
                let param_span = self.span();
                let param_name = self.expect_ident("a parameter name")?;
                self.expect(P::Colon).map_err(|_| {
                    self.err(
                        param_name.span,
                        "every `fn` parameter states its type; write `name: num`",
                    )
                })?;
                let ty = self.ty()?;
                params.push(TypedParam {
                    name: param_name,
                    ty,
                    span: param_span,
                });
                if !self.eat(P::Comma) {
                    break;
                }
            }
        }
        self.expect(P::RParen)?;
        self.check_duplicates(
            &params.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "parameter",
        )?;
        self.expect(P::Arrow)?;
        let ret = self.ty()?;
        self.expect(P::LBrace)?;
        let body = self.expr()?;
        self.expect(P::RBrace)?;
        Ok(FnDecl {
            public,
            name,
            params,
            ret,
            body,

            span: start,
        })
    }

    fn macro_decl(&mut self, public: bool) -> Result<MacroDecl> {
        let start = self.expect_kw(Kw::Macro)?.span;
        let name = self.expect_ident("a macro name")?;
        self.expect(P::LParen)?;
        let mut params = Vec::new();
        if !self.at_punct(P::RParen) {
            loop {
                let param_span = self.span();
                let param_name = match self.peek().clone() {
                    Tok::Param(raw) => {
                        let token = self.bump();
                        Ident::new(raw, token.span)
                    }
                    other => {
                        return Err(self.expected(&format!(
                            "a macro parameter starting with `$`, found {}",
                            other.describe()
                        )))
                    }
                };
                self.expect(P::Colon).map_err(|_| {
                    self.err(
                        param_name.span,
                        "a macro parameter states its kind; write `$x: expr`",
                    )
                })?;
                let kind = self.macro_param_kind()?;
                params.push(MacroParam {
                    name: param_name,
                    kind,
                    span: param_span,
                });
                if !self.eat(P::Comma) {
                    break;
                }
            }
        }
        self.expect(P::RParen)?;
        self.check_duplicates(
            &params.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "macro parameter",
        )?;
        self.expect(P::Arrow)?;
        let (result, body) = if matches!(self.peek(), Tok::Ident(word) if word == "stmts") {
            self.bump();
            let body = self.block()?;
            (MacroResult::Stmts, MacroBody::Stmts(body))
        } else {
            let ty = self.ty()?;
            self.expect(P::LBrace)?;
            let expr = self.expr()?;
            self.expect(P::RBrace)?;
            (MacroResult::Expr(ty), MacroBody::Expr(expr))
        };
        Ok(MacroDecl {
            public,
            name,
            params,
            result,
            body,

            span: start,
        })
    }

    fn macro_param_kind(&mut self) -> Result<MacroParamKind> {
        let span = self.span();
        let word = self.expect_ident("a parameter kind")?;
        Ok(match word.name.as_str() {
            "expr" => {
                let ty = if self.at_punct(P::Lt) {
                    self.bump();
                    let ty = self.ty()?;
                    self.expect(P::Gt)?;
                    Some(ty)
                } else {
                    None
                };
                MacroParamKind::Expr(ty)
            }
            "ident" => MacroParamKind::Ident,
            "block" => MacroParamKind::Block,
            other => {
                return Err(self
                    .err(span, format!("unknown macro parameter kind `{other}`"))
                    .note("the kinds are `expr`, `expr<T>`, `ident` and `block`"))
            }
        })
    }

    fn script(&mut self) -> Result<ScriptDecl> {
        let start = self.expect_kw(Kw::On)?.span;
        let name = self.expect_ident("a hat name")?;
        let mut args = Vec::new();
        if self.eat(P::LParen) {
            if !self.at_punct(P::RParen) {
                loop {
                    args.push(self.expr()?);
                    if !self.eat(P::Comma) {
                        break;
                    }
                }
            }
            self.expect(P::RParen)?;
        }
        let body = self.block()?;
        Ok(ScriptDecl {
            hat: Hat {
                span: name.span,
                name,
                args,
            },
            body,

            span: start,
        })
    }

    fn check_duplicates(&self, names: &[&Ident], what: &str) -> Result<()> {
        for (index, name) in names.iter().enumerate() {
            if names[..index].iter().any(|other| other.name == name.name) {
                return Err(self.err(name.span, format!("duplicate {what} `{}`", name.name)));
            }
        }
        Ok(())
    }

    // -- statements -------------------------------------------------------

    fn block(&mut self) -> Result<Block> {
        self.expect(P::LBrace)?;
        let mut stmts = Vec::new();
        while !self.at_punct(P::RBrace) {
            if matches!(self.peek(), Tok::Eof) {
                return Err(self.err(self.span(), "unclosed `{`"));
            }
            stmts.push(self.stmt()?);
        }
        self.expect(P::RBrace)?;
        Ok(stmts)
    }

    fn stmt(&mut self) -> Result<Stmt> {
        let start = self.span();
        if self.at_kw(Kw::Let) {
            return self.let_stmt();
        }
        if self.at_kw(Kw::If) {
            return self.if_stmt();
        }
        if self.at_kw(Kw::Repeat) {
            self.bump();
            let times = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::Loop(LoopStmt {
                kind: LoopKind::Repeat(times),
                body,
                span: start,
            }));
        }
        if self.at_kw(Kw::RepeatUntil) {
            self.bump();
            let cond = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::Loop(LoopStmt {
                kind: LoopKind::RepeatUntil(cond),
                body,
                span: start,
            }));
        }
        if self.at_kw(Kw::Forever) {
            self.bump();
            let body = self.block()?;
            return Ok(Stmt::Loop(LoopStmt {
                kind: LoopKind::Forever,
                body,
                span: start,
            }));
        }
        if self.at_kw(Kw::While) {
            self.bump();
            let cond = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::Macro(MacroCall {
                name: Ident::new(sugar::WHILE, start),
                args: vec![MacroArg::Expr(cond), MacroArg::Block(body)],
                span: start,
            }));
        }
        if self.at_kw(Kw::For) {
            return self.for_stmt();
        }
        if self.at_kw(Kw::Match) {
            return self.match_stmt();
        }
        if self.at_kw(Kw::Var) {
            // A `var` statement is a macro temporary; the checker rejects it
            // anywhere else, because Scratch variables belong to a target.
            let decl = self.var_decl(false)?;
            return Ok(Stmt::Var(decl));
        }
        if let Tok::Param(name) = self.peek().clone() {
            // `$i = e;` and `$i += e;` assign to the name the caller supplied; a
            // bare `$body;` is the block a `block` parameter stands for.
            if self.at_punct_n(1, P::Eq) || self.compound_op_at(1).is_some() {
                let token = self.bump();
                let target = LValue {
                    name: Ident::new(format!("${name}"), token.span),
                    path: Vec::new(),
                    span: start,
                };
                return self.assign_or_compound(start, target);
            }
            self.bump();
            self.expect(P::Semi)?;
            return Ok(Stmt::Param(Ident::new(name, start)));
        }
        if self.at_kw(Kw::Return) {
            let start = self.bump().span;
            let value = if self.at_punct(P::Semi) {
                None
            } else {
                Some(self.expr()?)
            };
            self.expect(P::Semi)?;
            return Ok(Stmt::Return(ReturnStmt { value, span: start }));
        }
        self.call_or_assign()
    }

    fn let_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect_kw(Kw::Let)?.span;
        let name = match self.peek().clone() {
            Tok::Param(raw) => {
                let token = self.bump();
                Ident::new(format!("${raw}"), token.span)
            }
            _ => self.expect_ident("a name")?,
        };
        let ty = if self.eat(P::Colon) {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(P::Eq)?;
        let value = self.struct_value()?;
        self.expect(P::Semi)?;
        Ok(Stmt::Let(LetStmt {
            name,
            ty,
            value,
            span: start,
        }))
    }

    fn if_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect_kw(Kw::If)?.span;
        let cond = self.expr()?;
        let then_branch = self.block()?;
        let else_branch = if self.eat_kw(Kw::Else) {
            if self.at_kw(Kw::If) {
                // `else if` is `else { if … }`.
                Some(vec![self.if_stmt()?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If(IfStmt {
            cond,
            then_branch,
            else_branch,
            span: start,
        }))
    }

    fn for_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect_kw(Kw::For)?.span;
        let var = self.expect_ident("a loop variable")?;
        self.expect_kw(Kw::In)?;
        let from = self.expr()?;
        self.expect(P::DotDot).map_err(|_| {
            self.err(
                self.span(),
                "a `for` loop counts a range; write `for i in 0..10 { … }`",
            )
        })?;
        let to = self.expr()?;
        let body = self.block()?;
        Ok(Stmt::Macro(MacroCall {
            name: Ident::new(sugar::FOR, start),
            args: vec![
                MacroArg::Ident(var),
                MacroArg::Expr(from),
                MacroArg::Expr(to),
                MacroArg::Block(body),
            ],
            span: start,
        }))
    }

    fn match_stmt(&mut self) -> Result<Stmt> {
        let start = self.expect_kw(Kw::Match)?.span;
        let subject = self.expr()?;
        self.expect(P::LBrace)?;
        let mut arms = Vec::new();
        while !self.at_punct(P::RBrace) {
            let arm_start = self.span();
            let pattern = if self.at_punct(P::Underscore) {
                self.bump();
                None
            } else if matches!(self.peek(), Tok::Ident(_)) {
                // A name in a pattern is a `const`; the checker resolves it.
                let mut segments = vec![self.expect_ident("a pattern")?];
                while self.at_punct(P::ColonColon) {
                    self.bump();
                    segments.push(self.expect_segment()?);
                }
                Some(Pattern::Name(Path {
                    span: arm_start,
                    segments,
                }))
            } else {
                Some(Pattern::Literal(self.literal("a pattern")?))
            };
            self.expect(P::FatArrow)?;
            let body = self.block()?;
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_start,
            });
            if !self.eat(P::Comma) {
                break;
            }
        }
        self.expect(P::RBrace)?;
        if arms.is_empty() {
            return Err(self.err(start, "a `match` needs at least one arm"));
        }
        Ok(Stmt::Match(MatchStmt {
            subject,
            arms,
            span: start,
        }))
    }

    fn call_or_assign(&mut self) -> Result<Stmt> {
        let start = self.span();
        let mut segments = vec![match self.peek().clone() {
            // `sound::play(…)`: a module whose name is also a declaration
            // keyword, told apart from the declaration by the `::`.
            Tok::Kw(kw) if self.at_punct_n(1, P::ColonColon) => {
                let token = self.bump();
                Ident::new(kw.text(), token.span)
            }
            _ => self.expect_ident("a statement")?,
        }];
        while self.at_punct(P::ColonColon) {
            self.bump();
            segments.push(self.expect_segment()?);
        }
        let callee = Path {
            span: start,
            segments,
        };

        // The places inside the name this statement may write: `p.pos.x`,
        // `trail[1]`. A call never has one, so this is read before the
        // statement's own shape is known.
        let path = self.accessors()?;

        // `receiver.method(args);` — a VMS method used for its effect.
        if let [Accessor::Field(name)] = path.as_slice() {
            if self.at_punct(P::LParen) {
                if !callee.is_single() {
                    return Err(self.err(start, "a method is called on one value"));
                }
                let args = self.args()?;
                self.expect(P::Semi)?;
                return Ok(Stmt::Method(MethodStmt {
                    receiver: Expr::Name(callee),
                    name: name.clone(),
                    args,
                    span: start,
                }));
            }
        }

        // A compound assignment: `x += e`.
        if let Some(op) = self.eat_compound_op() {
            if !callee.is_single() {
                return Err(self.err(start, "only a variable can be assigned to"));
            }
            let target = LValue {
                name: callee.last().clone(),
                path,
                span: start,
            };
            let value = self.expr()?;
            self.expect(P::Semi)?;
            return Ok(Stmt::CompoundAssign(CompoundAssignStmt {
                target,
                op,
                value,
                span: start,
            }));
        }

        // `x = e`, `l[i] = e` or `p.field = e`.
        if self.at_punct(P::Eq) {
            return self.assign(start, callee, path);
        }

        if !path.is_empty() {
            return Err(self.err(start, format!("expected `=` after `{}`", callee.display())));
        }

        if !self.at_punct(P::LParen) {
            return Err(self
                .err(
                    start,
                    format!("expected `(` or `=` after `{}`", callee.display()),
                )
                .note(
                    "a statement is either a call `name(args);` or an assignment `name = value;`",
                ));
        }

        let args = self.args()?;
        let body_span = self.span();
        let body = if self.at_punct(P::LBrace) {
            Some(self.block()?)
        } else {
            None
        };
        if body.is_some() {
            self.eat(P::Semi);
        } else {
            self.expect(P::Semi)?;
        }
        Ok(Stmt::Call(CallStmt {
            callee,
            args,
            body_span,
            body,
            span: start,
        }))
    }

    /// `+=`, `-=`, `*=`, `/=`, `%=`, only when the two tokens are adjacent.
    fn compound_op_at(&self, offset: usize) -> Option<BinOp> {
        let Tok::Punct(p) = self.peek_n(offset) else {
            return None;
        };
        let op = match p {
            P::Plus => BinOp::Add,
            P::Minus => BinOp::Sub,
            P::Star => BinOp::Mul,
            P::Slash => BinOp::Div,
            P::Percent => BinOp::Rem,
            _ => return None,
        };
        if !self.at_punct_n(offset + 1, P::Eq)
            || !self.adjacent(self.span_n(offset), self.span_n(offset + 1))
        {
            return None;
        }
        Some(op)
    }

    fn compound_op_here(&self) -> Option<BinOp> {
        self.compound_op_at(0)
    }

    fn eat_compound_op(&mut self) -> Option<BinOp> {
        let op = self.compound_op_here()?;
        self.bump();
        self.bump();
        Some(op)
    }

    /// The rest of `x = e;` or `x += e;`, once the target has been read.
    fn assign_or_compound(&mut self, start: Span, target: LValue) -> Result<Stmt> {
        if let Some(op) = self.eat_compound_op() {
            let value = self.expr()?;
            self.expect(P::Semi)?;
            return Ok(Stmt::CompoundAssign(CompoundAssignStmt {
                target,
                op,
                value,
                span: start,
            }));
        }
        self.expect(P::Eq)?;
        let value = self.expr()?;
        self.expect(P::Semi)?;
        Ok(Stmt::Assign(AssignStmt {
            target,
            value,
            span: start,
        }))
    }

    fn assign(&mut self, start: Span, callee: Path, path: Vec<Accessor>) -> Result<Stmt> {
        if !callee.is_single() {
            return Err(self.err(start, "only a variable can be assigned to"));
        }
        let name = callee.last().clone();
        self.expect(P::Eq)?;
        let value = self.expr()?;
        self.expect(P::Semi)?;
        Ok(Stmt::Assign(AssignStmt {
            target: LValue {
                name,
                path,
                span: start,
            },
            value,
            span: start,
        }))
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        self.expect(P::LParen)?;
        let mut args = Vec::new();
        if !self.at_punct(P::RParen) {
            loop {
                args.push(self.expr()?);
                if !self.eat(P::Comma) {
                    break;
                }
            }
        }
        self.expect(P::RParen)?;
        Ok(args)
    }

    // -- expressions ------------------------------------------------------

    fn literal(&mut self, what: &str) -> Result<Literal> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Number(text) => {
                self.bump();
                LiteralKind::Number(text)
            }
            Tok::Str(text) => {
                self.bump();
                LiteralKind::Str(text)
            }
            Tok::Punct(P::Minus) => {
                self.bump();
                match self.peek().clone() {
                    Tok::Number(text) => {
                        self.bump();
                        LiteralKind::Number(format!("-{text}"))
                    }
                    other => {
                        return Err(self.expected(&format!(
                            "{what}, found `-` followed by {}",
                            other.describe()
                        )))
                    }
                }
            }
            Tok::Kw(Kw::True) => {
                self.bump();
                LiteralKind::Bool(true)
            }
            Tok::Kw(Kw::False) => {
                self.bump();
                LiteralKind::Bool(false)
            }
            other => return Err(self.expected(&format!("{what}, found {}", other.describe()))),
        };
        Ok(Literal { kind, span })
    }

    fn expr(&mut self) -> Result<Expr> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr> {
        let mut lhs = self.and()?;
        while self.at_punct(P::OrOr) {
            let span = self.bump().span;
            let rhs = self.and()?;
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn and(&mut self) -> Result<Expr> {
        let mut lhs = self.comparison()?;
        while self.at_punct(P::AndAnd) {
            let span = self.bump().span;
            let rhs = self.comparison()?;
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn comparison(&mut self) -> Result<Expr> {
        let lhs = self.sum()?;
        let op = match self.peek() {
            Tok::Punct(P::EqEq) => BinOp::Eq,
            Tok::Punct(P::NotEq) => BinOp::Ne,
            Tok::Punct(P::Lt) => BinOp::Lt,
            Tok::Punct(P::Le) => BinOp::Le,
            Tok::Punct(P::Gt) => BinOp::Gt,
            Tok::Punct(P::Ge) => BinOp::Ge,
            _ => return Ok(lhs),
        };
        let span = self.bump().span;
        let rhs = self.sum()?;
        if matches!(
            self.peek(),
            Tok::Punct(P::EqEq | P::NotEq | P::Lt | P::Le | P::Gt | P::Ge)
        ) {
            return Err(self
                .err(self.span(), "comparison operators do not chain")
                .note("write `a < b && b < c` instead of `a < b < c`"));
        }
        Ok(Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        })
    }

    fn sum(&mut self) -> Result<Expr> {
        let mut lhs = self.product()?;
        loop {
            let op = match self.peek() {
                Tok::Punct(P::Plus) => BinOp::Add,
                Tok::Punct(P::Minus) => BinOp::Sub,
                _ => return Ok(lhs),
            };
            let span = self.bump().span;
            let rhs = self.product()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
    }

    fn product(&mut self) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Punct(P::Star) => BinOp::Mul,
                Tok::Punct(P::Slash) => BinOp::Div,
                Tok::Punct(P::Percent) => BinOp::Rem,
                _ => return Ok(lhs),
            };
            let span = self.bump().span;
            let rhs = self.unary()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        let span = self.span();
        if self.at_punct(P::Bang) {
            self.bump();
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: UnOp::Not,
                operand: Box::new(operand),
                span,
            });
        }
        if self.at_punct(P::Minus) {
            self.bump();
            // A `-` directly before a number is part of the literal, so `-2.5`
            // reaches the project spelled the way it was written.
            if let Tok::Number(text) = self.peek().clone() {
                self.bump();
                let end = self.span();
                return Ok(Expr::Number {
                    text: format!("-{text}"),
                    span: Span::new(span.pos, end.pos.col.saturating_sub(span.pos.col) + end.len),
                });
            }
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op: UnOp::Neg,
                operand: Box::new(operand),
                span,
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut expr = self.primary()?;
        loop {
            if self.at_punct(P::LBracket) {
                let span = self.bump().span;
                let index = self.expr()?;
                self.expect(P::RBracket)?;
                expr = Expr::Index {
                    list: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else if self.at_punct(P::Dot) {
                let span = self.bump().span;
                let name = self.expect_ident("a field or method name")?;
                if self.at_punct(P::LParen) {
                    let args = self.args()?;
                    expr = Expr::Method {
                        receiver: Box::new(expr),
                        name,
                        args,
                        span,
                    };
                } else {
                    expr = Expr::Field {
                        base: Box::new(expr),
                        name,
                        span,
                    };
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }

    /// The `[…]` and `.field` accessors after a name in an assignment target.
    fn accessors(&mut self) -> Result<Vec<Accessor>> {
        let mut path = Vec::new();
        loop {
            if self.at_punct(P::LBracket) {
                self.bump();
                let index = self.expr()?;
                self.expect(P::RBracket)?;
                path.push(Accessor::Index(Box::new(index)));
            } else if self.at_punct(P::Dot) {
                self.bump();
                path.push(Accessor::Field(self.expect_ident("a field name")?));
            } else {
                break;
            }
        }
        Ok(path)
    }

    fn primary(&mut self) -> Result<Expr> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Number(text) => {
                self.bump();
                Ok(Expr::Number { text, span })
            }
            Tok::Str(text) => {
                self.bump();
                Ok(Expr::Str { text, span })
            }
            Tok::Kw(Kw::True) => {
                self.bump();
                Ok(Expr::Bool { value: true, span })
            }
            Tok::Kw(Kw::False) => {
                self.bump();
                Ok(Expr::Bool { value: false, span })
            }
            Tok::Param(name) => {
                self.bump();
                Ok(Expr::Param(Ident::new(name, span)))
            }
            Tok::Interp(parts) => {
                self.bump();
                let mut out = Vec::new();
                for part in parts {
                    match part {
                        LexPart::Text(text) => out.push(InterpPart::Text(text)),
                        LexPart::Hole(tokens) => {
                            let hole = self.sub_parse(&tokens)?;
                            out.push(InterpPart::Hole(hole));
                        }
                    }
                }
                Ok(Expr::Interpolated { parts: out, span })
            }
            Tok::Punct(P::LParen) => {
                self.bump();
                let expr = self.expr()?;
                self.expect(P::RParen)?;
                Ok(expr)
            }
            Tok::Ident(_) => {
                let mut segments = vec![self.expect_ident("an expression")?];
                while self.at_punct(P::ColonColon) {
                    self.bump();
                    segments.push(self.expect_segment()?);
                }
                let path = Path { span, segments };
                if self.at_punct(P::LParen) {
                    let args = self.args()?;
                    Ok(Expr::Call(CallExpr {
                        callee: path,
                        args,
                        span,
                    }))
                } else {
                    Ok(Expr::Name(path))
                }
            }
            // `sound::volume()`: the module name is a declaration keyword, and
            // the `::` is what says a path is meant.
            Tok::Kw(kw) if self.at_punct_n(1, P::ColonColon) => {
                let token = self.bump();
                let mut segments = vec![Ident::new(kw.text(), token.span)];
                while self.at_punct(P::ColonColon) {
                    self.bump();
                    segments.push(self.expect_segment()?);
                }
                let path = Path { span, segments };
                if self.at_punct(P::LParen) {
                    let args = self.args()?;
                    Ok(Expr::Call(CallExpr {
                        callee: path,
                        args,
                        span,
                    }))
                } else {
                    Ok(Expr::Name(path))
                }
            }
            // `num(x)` and `str(x)`: the two free conversions. They are spelled
            // with a type keyword, so only a call can be meant — `num` on its own
            // is a type, and there is no expression `num`.
            Tok::Kw(kw @ (Kw::Num | Kw::Str)) if self.at_punct_n(1, P::LParen) => {
                let token = self.bump();
                let args = self.args()?;
                Ok(Expr::Call(CallExpr {
                    callee: Path {
                        span,
                        segments: vec![Ident::new(kw.text(), token.span)],
                    },
                    args,
                    span,
                }))
            }
            other => Err(self.expected(&format!("an expression, found {}", other.describe()))),
        }
    }

    /// Parse an expression from a nested token slice, as an interpolation hole.
    fn sub_parse(&self, tokens: &[Token]) -> Result<Expr> {
        let mut sub = Parser {
            src: self.src,
            toks: tokens.to_vec(),
            at: 0,
        };
        if sub.toks.is_empty() {
            return Err(self.err(self.span(), "empty `{}` in an interpolated string"));
        }
        let expr = sub.expr()?;
        if !matches!(sub.peek(), Tok::Eof) {
            let what = sub.peek().describe();
            return Err(self.err(
                sub.span(),
                format!("unexpected {what} in an interpolation hole"),
            ));
        }
        Ok(expr)
    }

    /// A literal used as a `var` or `const` initializer.
    fn initializer(&mut self) -> Result<Initializer> {
        if self.at_punct(P::LBracket) {
            self.bump();
            let mut items = Vec::new();
            if !self.at_punct(P::RBracket) {
                loop {
                    items.push(self.literal("a literal")?);
                    if !self.eat(P::Comma) {
                        break;
                    }
                }
            }
            self.expect(P::RBracket)?;
            return Ok(Initializer::Items(items));
        }
        // `Point { x: 0, y: 0 }`: a struct is built by naming its fields. The
        // name is read and dropped here; the declaration's type says which
        // struct it is, and the lowerer checks the two agree.
        if matches!(self.peek(), Tok::Ident(_)) && self.at_punct_n(1, P::LBrace) {
            self.bump();
            return Ok(Initializer::Fields(self.struct_fields()?));
        }
        Ok(Initializer::Value(self.literal("a literal")?))
    }

    /// The `{ field: value, … }` of a struct literal, after the struct's name.
    fn struct_fields(&mut self) -> Result<Vec<(Ident, Expr)>> {
        self.expect(P::LBrace)?;
        let mut fields = Vec::new();
        while !self.at_punct(P::RBrace) {
            let name = self.expect_ident("a field name")?;
            self.expect(P::Colon)?;
            let value = self.struct_value()?;
            if fields
                .iter()
                .any(|(f, _): &(Ident, Expr)| f.name == name.name)
            {
                return Err(self.err(name.span, format!("`{}` is given twice", name.name)));
            }
            fields.push((name, value));
            if !self.eat(P::Comma) {
                break;
            }
        }
        self.expect(P::RBrace)?;
        Ok(fields)
    }

    /// `Point { x: 0, y: 0 }`, in the positions where a struct literal is the
    /// only thing that could be meant: a declaration's initializer and a field
    /// of another struct literal.
    ///
    /// It is not parsed in expression position on purpose: `if ready { … }`
    /// ends a condition with a name, and `ready {` must not become a literal.
    fn struct_literal(&mut self) -> Result<Expr> {
        let span = self.span();
        let name = self.expect_ident("a struct name")?;
        let fields = self.struct_fields()?;
        Ok(Expr::Struct { name, fields, span })
    }

    /// A field value: a nested struct literal, or an expression.
    fn struct_value(&mut self) -> Result<Expr> {
        if matches!(self.peek(), Tok::Ident(_)) && self.at_punct_n(1, P::LBrace) {
            return self.struct_literal();
        }
        self.expr()
    }
}
