// Modified MIT License
//
// Copyright (c) 2026 Joel Odom
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, and sublicense copies of the
// Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// "Commons Clause" License Condition v1.0
//
// The Software is provided to you by the Licensor under the License, subject to
// the following condition.
//
// Without limiting other conditions in the License, the grant of rights under the
// License will not include, and the License does not grant to you, the right to
// Sell the Software.
//
// For purposes of the foregoing, "Sell" means practicing any or all of the rights
// granted to you under the License to provide to third parties, for a fee or other
// consideration (including without limitation fees for hosting or consulting/
// support services related to the Software), a product or service whose value
// derives, entirely or substantially, from the functionality of the Software. Any
// license notice or attribution required by the License must also include this
// Commons Clause License Condition notice.
//
// Software: Libre99
//
// License: Modified MIT
//
// Licensor: Joel Odom
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The GSL recursive-descent parser: token stream → [`ast::Program`].
//! Grammar per `docs/GSL.md`; the parser is purely syntactic — name/width
//! resolution and the canonical-spelling rules live in `codegen`.

use crate::ast::*;
use crate::lexer::{lex, Tok, Token};

/// A parse (or lex) error with its source line.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Parse a whole `.gsl` source.
pub fn parse(src: &str) -> Result<Program, ParseError> {
    let toks = lex(src).map_err(|e| ParseError { line: e.line, message: e.message })?;
    let mut p = Parser { toks, pos: 0 };
    let mut items = Vec::new();
    while !p.at_eof() {
        items.push(p.item()?);
    }
    Ok(Program { items })
}

const ZERO_OPS: &[(&str, ZeroOp)] = &[
    ("scan", ZeroOp::Scan),
    ("exit", ZeroOp::Exit),
    ("cont", ZeroOp::Cont),
    ("exec", ZeroOp::Exec),
    ("rtnb", ZeroOp::Rtnb),
    ("rtgr", ZeroOp::Rtgr),
];
const IMM_OPS: &[(&str, ImmOp)] = &[
    ("back", ImmOp::Back),
    ("all", ImmOp::All),
    ("rand", ImmOp::Rand),
    ("parse", ImmOp::Parse),
    ("xml", ImmOp::Xml),
];
const ONE_OPS: &[(&str, OneOp)] = &[
    ("inct", OneOp::Inct),
    ("dect", OneOp::Dect),
    ("abs", OneOp::Abs),
    ("neg", OneOp::Neg),
    ("inv", OneOp::Inv),
    ("push", OneOp::Push),
    ("fetch", OneOp::Fetch),
    ("case", OneOp::Case),
];
const STATUS_OPS: &[(&str, StatusOp)] = &[
    ("carry", StatusOp::Carry),
    ("ovf", StatusOp::Ovf),
    ("gt", StatusOp::Gt),
    ("h", StatusOp::H),
];

/// Words that cannot be user identifiers (they open statements/items or name
/// intrinsic forms).
pub const RESERVED: &[&str] = &[
    "format", "cartridge", "cru", "origin", "grompage", "const", "var", "fn", "data", "asm",
    "rom", "byte", "word", "cpu", "vdp", "grom", "gram", "vreg", "goto", "if", "else", "while",
    "return", "returnc", "test", "cond", "move", "call", "swap", "rotr", "scan", "exit", "cont",
    "exec", "rtnb", "rtgr", "back", "all", "rand", "parse", "xml", "inct", "dect", "abs", "neg",
    "inv", "push", "fetch", "case", "carry", "ovf", "gt", "h", "fmt", "htext", "vtext", "hchar",
    "vchar", "hmove", "vmove", "row", "col", "bias", "hstr", "repeat",
];

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    // ---- token plumbing -----------------------------------------------------

    fn cur(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    fn at_eof(&self) -> bool {
        matches!(self.cur(), Tok::Eof)
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if !matches!(t, Tok::Eof) {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError { line: self.line(), message: message.into() })
    }
    fn is_p(&self, p: &str) -> bool {
        matches!(self.cur(), Tok::P(q) if *q == p)
    }
    fn eat_p(&mut self, p: &str) -> bool {
        if self.is_p(p) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect_p(&mut self, p: &str) -> Result<(), ParseError> {
        if self.eat_p(p) {
            Ok(())
        } else {
            self.err(format!("expected '{p}', found {}", describe(self.cur())))
        }
    }
    fn is_ident(&self, k: &str) -> bool {
        matches!(self.cur(), Tok::Ident(s) if s == k)
    }
    fn eat_ident(&mut self, k: &str) -> bool {
        if self.is_ident(k) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn any_ident(&mut self) -> Result<String, ParseError> {
        match self.cur().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => self.err(format!("expected an identifier, found {}", describe(&other))),
        }
    }
    fn user_ident(&mut self) -> Result<String, ParseError> {
        let line = self.line();
        let s = self.any_ident()?;
        if RESERVED.contains(&s.as_str()) {
            return Err(ParseError { line, message: format!("'{s}' is a reserved word") });
        }
        if s.starts_with("_G") {
            return Err(ParseError {
                line,
                message: "identifiers starting with _G are reserved for the compiler".into(),
            });
        }
        Ok(s)
    }
    /// Peek the token after the current one.
    fn next_is_p(&self, p: &str) -> bool {
        matches!(self.toks.get(self.pos + 1).map(|t| &t.tok), Some(Tok::P(q)) if *q == p)
    }

    // ---- expressions --------------------------------------------------------

    /// Parse a constant expression; also reports whether it was exactly the
    /// bare token `0` (the canonical-spelling flag).
    fn expr_flagged(&mut self) -> Result<(Expr, bool), ParseError> {
        let start = self.pos;
        let e = self.expr_bin(0)?;
        let bare = self.pos == start + 1 && matches!(self.toks[start].tok, Tok::Num(0, true));
        Ok((e, bare))
    }
    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.expr_bin(0)
    }

    fn expr_bin(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_unary()?;
        loop {
            let (op, prec): (&'static str, u8) = match self.cur() {
                Tok::P("|") => ("|", 1),
                Tok::P("^") => ("^", 2),
                Tok::P("&") => ("&", 3),
                Tok::P("<<") => ("<<", 4),
                Tok::P(">>") => (">>", 4),
                Tok::P("+") => ("+", 5),
                Tok::P("-") => ("-", 5),
                Tok::P("*") => ("*", 6),
                Tok::P("/") => ("/", 6),
                Tok::P("%") => ("%", 6),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.expr_bin(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn expr_unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat_p("-") {
            return Ok(Expr::Unary('-', Box::new(self.expr_unary()?)));
        }
        if self.eat_p("~") {
            return Ok(Expr::Unary('~', Box::new(self.expr_unary()?)));
        }
        match self.cur().clone() {
            Tok::Num(v, _) => {
                self.bump();
                Ok(Expr::Num(v))
            }
            Tok::Ident(s) => {
                if RESERVED.contains(&s.as_str()) {
                    return self.err(format!("'{s}' is a reserved word, not a value"));
                }
                self.bump();
                Ok(Expr::Name(s))
            }
            Tok::P("(") => {
                self.bump();
                let e = self.expr_bin(0)?;
                self.expect_p(")")?;
                Ok(e)
            }
            other => self.err(format!("expected a value, found {}", describe(&other))),
        }
    }

    // ---- items --------------------------------------------------------------

    fn item(&mut self) -> Result<Item, ParseError> {
        let line = self.line();
        if let Tok::Ident(kw) = self.cur().clone() {
            match kw.as_str() {
                "format" => {
                    self.bump();
                    let which = self.any_ident()?;
                    let f = match which.as_str() {
                        "ctg" => OutFormat::Ctg,
                        "grom" => OutFormat::Grom,
                        "grom24" => OutFormat::Grom24,
                        "rombin" => OutFormat::RomBin,
                        other => {
                            return Err(ParseError {
                                line,
                                message: format!(
                                    "unknown format '{other}' (ctg, grom, grom24, rombin)"
                                ),
                            })
                        }
                    };
                    self.expect_p(";")?;
                    return Ok(Item::Format(f));
                }
                "cartridge" => {
                    self.bump();
                    let s = match self.bump() {
                        Tok::Str(b) => String::from_utf8_lossy(&b).into_owned(),
                        _ => return Err(ParseError { line, message: "cartridge needs a \"TITLE\" string".into() }),
                    };
                    self.expect_p(";")?;
                    return Ok(Item::Cartridge(s));
                }
                "cru" => {
                    self.bump();
                    let e = self.expr()?;
                    self.expect_p(";")?;
                    return Ok(Item::Cru(e));
                }
                "origin" => {
                    self.bump();
                    let e = self.expr()?;
                    self.expect_p(";")?;
                    return Ok(Item::Origin(e));
                }
                "grompage" => {
                    self.bump();
                    let e = self.expr()?;
                    self.expect_p(";")?;
                    return Ok(Item::GromPage(e));
                }
                "const" => {
                    self.bump();
                    let name = self.user_ident()?;
                    self.expect_p("=")?;
                    let value = self.expr()?;
                    self.expect_p(";")?;
                    return Ok(Item::Const { name, value, line });
                }
                "var" => {
                    self.bump();
                    let name = self.user_ident()?;
                    self.expect_p(":")?;
                    let width = self.width()?;
                    self.expect_p("@")?;
                    let space = if self.eat_ident("cpu") {
                        Space::Cpu
                    } else if self.eat_ident("vdp") {
                        Space::Vdp
                    } else {
                        return self.err("var address must be cpu[…] or vdp[…]");
                    };
                    self.expect_p("[")?;
                    let addr = self.expr()?;
                    self.expect_p("]")?;
                    self.expect_p(";")?;
                    return Ok(Item::Var { name, width, space, addr, line });
                }
                "fn" => {
                    self.bump();
                    let name = self.user_ident()?;
                    self.expect_p("(")?;
                    self.expect_p(")")?;
                    let pin = if self.eat_p("@") { Some(self.expr()?) } else { None };
                    self.expect_p("{")?;
                    let body = self.block_body()?;
                    return Ok(Item::Func { name, pin, body, line });
                }
                "data" => {
                    self.bump();
                    let name = match self.cur() {
                        Tok::Ident(_) => Some(self.user_ident()?),
                        _ => None,
                    };
                    let pin = if self.eat_p("@") { Some(self.expr()?) } else { None };
                    self.expect_p("{")?;
                    let items = self.data_items()?;
                    return Ok(Item::Data { name, pin, items, line });
                }
                "rom" => {
                    self.bump();
                    let bank = self.expr()?;
                    self.expect_p("{")?;
                    let items = self.data_items()?;
                    return Ok(Item::Rom { bank, items, line });
                }
                "asm" => {
                    self.bump();
                    match self.bump() {
                        Tok::AsmBody(lines) => return Ok(Item::AsmBlock { lines, line }),
                        _ => return Err(ParseError { line, message: "asm must open a { } block".into() }),
                    }
                }
                other => {
                    return Err(ParseError {
                        line,
                        message: format!("expected a top-level item, found '{other}'"),
                    })
                }
            }
        }
        self.err(format!("expected a top-level item, found {}", describe(self.cur())))
    }

    fn width(&mut self) -> Result<Width, ParseError> {
        if self.eat_ident("byte") {
            Ok(Width::Byte)
        } else if self.eat_ident("word") {
            Ok(Width::Word)
        } else {
            self.err("expected 'byte' or 'word'")
        }
    }

    fn data_items(&mut self) -> Result<Vec<DataItem>, ParseError> {
        let mut items = Vec::new();
        loop {
            if self.eat_p("}") {
                return Ok(items);
            }
            if self.eat_ident("word") {
                items.push(DataItem::Word(self.expr()?));
            } else if let Tok::Str(bytes) = self.cur().clone() {
                self.bump();
                items.push(DataItem::Str(bytes));
            } else {
                items.push(DataItem::Byte(self.expr()?));
            }
            if !self.eat_p(",") {
                self.expect_p("}")?;
                return Ok(items);
            }
        }
    }

    // ---- statements ---------------------------------------------------------

    /// Parse statements until the closing `}` (consumed).
    fn block_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        loop {
            // Labels: IDENT ':' (any number).
            let mut labels = Vec::new();
            while matches!(self.cur(), Tok::Ident(_)) && self.next_is_p(":") {
                labels.push(self.user_ident()?);
                self.expect_p(":")?;
            }
            if self.is_p("}") {
                self.bump();
                if !labels.is_empty() {
                    out.push(Stmt { line: self.line(), labels, kind: StmtKind::Empty });
                }
                return Ok(out);
            }
            let line = self.line();
            let kind = self.stmt_kind()?;
            out.push(Stmt { line, labels, kind });
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect_p("{")?;
        self.block_body()
    }

    // ---- fmt { } — the FMT screen-format sub-language -----------------------

    /// Parse `fmt { … }` sub-statements until the closing `}` (consumed).
    fn fmt_body(&mut self) -> Result<Vec<FmtStmt>, ParseError> {
        let mut out = Vec::new();
        loop {
            if self.eat_p("}") {
                return Ok(out);
            }
            let line = self.line();
            let op = self.fmt_op()?;
            out.push(FmtStmt { line, op });
        }
    }

    fn fmt_op(&mut self) -> Result<FmtOp, ParseError> {
        if self.eat_ident("repeat") {
            self.expect_p("(")?;
            let count = self.expr()?;
            self.expect_p(")")?;
            self.expect_p("{")?;
            let body = self.fmt_body()?;
            return Ok(FmtOp::Repeat { count, body });
        }
        for (name, vertical) in [("htext", false), ("vtext", true)] {
            if self.eat_ident(name) {
                self.expect_p("(")?;
                let bytes = match self.cur().clone() {
                    Tok::Str(b) => {
                        self.bump();
                        b
                    }
                    other => {
                        return self.err(format!(
                            "{name} takes a string literal, found {}",
                            describe(&other)
                        ))
                    }
                };
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(FmtOp::Text { vertical, bytes });
            }
        }
        for (name, vertical) in [("hchar", false), ("vchar", true)] {
            if self.eat_ident(name) {
                self.expect_p("(")?;
                let count = self.expr()?;
                self.expect_p(",")?;
                let ch = self.expr()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(FmtOp::Chars { vertical, count, ch });
            }
        }
        for (name, vertical) in [("hmove", false), ("vmove", true)] {
            if self.eat_ident(name) {
                self.expect_p("(")?;
                let count = self.expr()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(FmtOp::Skip { vertical, count });
            }
        }
        for row in [true, false] {
            if self.eat_ident(if row { "row" } else { "col" }) {
                self.expect_p("(")?;
                let e = self.expr()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(if row { FmtOp::Row(e) } else { FmtOp::Col(e) });
            }
        }
        if self.eat_ident("bias") {
            self.expect_p("(")?;
            let arg = self.operand()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(FmtOp::Bias(arg));
        }
        if self.eat_ident("hstr") {
            self.expect_p("(")?;
            let count = self.expr()?;
            self.expect_p(",")?;
            let place = self.place()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(FmtOp::HStr { count, place });
        }
        self.err(format!("expected an fmt sub-op, found {}", describe(self.cur())))
    }

    fn stmt_kind(&mut self) -> Result<StmtKind, ParseError> {
        // asm block as a statement.
        if self.is_ident("asm") {
            self.bump();
            return match self.bump() {
                Tok::AsmBody(lines) => Ok(StmtKind::Asm(lines)),
                _ => self.err("asm must open a { } block"),
            };
        }
        if self.eat_ident("fmt") {
            self.expect_p("{")?;
            return Ok(StmtKind::Fmt(self.fmt_body()?));
        }
        if self.eat_ident("goto") {
            let t = self.target()?;
            self.expect_p(";")?;
            return Ok(StmtKind::Goto(t));
        }
        if self.eat_ident("return") {
            self.expect_p(";")?;
            return Ok(StmtKind::Return);
        }
        if self.eat_ident("returnc") {
            self.expect_p(";")?;
            return Ok(StmtKind::ReturnC);
        }
        if self.eat_ident("if") {
            self.expect_p("(")?;
            let cond = self.cond()?;
            self.expect_p(")")?;
            if self.eat_ident("goto") {
                let t = self.target()?;
                self.expect_p(";")?;
                return Ok(StmtKind::IfGoto { cond, target: t });
            }
            let then_ = self.block()?;
            let else_ = if self.eat_ident("else") { self.block()? } else { Vec::new() };
            return Ok(StmtKind::If { cond, then_, else_ });
        }
        if self.eat_ident("while") {
            self.expect_p("(")?;
            let cond = self.cond()?;
            self.expect_p(")")?;
            let body = self.block()?;
            return Ok(StmtKind::While { cond, body });
        }
        if self.eat_ident("test") {
            self.expect_p("(")?;
            let cond = self.cond()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            if matches!(cond, Cond::CondBit { .. }) {
                return self.err("test(cond()) has no meaning — cond() only guards a goto");
            }
            return Ok(StmtKind::Test(cond));
        }
        if self.eat_ident("call") {
            self.expect_p("(")?;
            let e = self.expr()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(StmtKind::Call(Target::Addr(e)));
        }
        if self.eat_ident("move") {
            self.expect_p("(")?;
            let dst = self.move_dst()?;
            self.expect_p(",")?;
            let src = self.move_src()?;
            self.expect_p(",")?;
            let count = self.move_count()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(StmtKind::Move { dst, src, count });
        }
        if self.eat_ident("swap") {
            self.expect_p("(")?;
            let a = self.place()?;
            self.expect_p(",")?;
            let b = self.place()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(StmtKind::Swap { a, b });
        }
        if self.eat_ident("rotr") {
            self.expect_p("(")?;
            let dst = self.place()?;
            self.expect_p(",")?;
            let count = self.operand()?;
            self.expect_p(")")?;
            self.expect_p(";")?;
            return Ok(StmtKind::Rotr { dst, count });
        }
        if let Tok::Ident(name) = self.cur().clone() {
            if let Some(&(_, op)) = ZERO_OPS.iter().find(|(n, _)| *n == name) {
                self.bump();
                self.expect_p("(")?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(StmtKind::Zero(op));
            }
            if let Some(&(_, op)) = IMM_OPS.iter().find(|(n, _)| *n == name) {
                self.bump();
                self.expect_p("(")?;
                let arg = self.expr()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(StmtKind::ImmArg { which: op, arg });
            }
            if let Some(&(_, op)) = ONE_OPS.iter().find(|(n, _)| *n == name) {
                self.bump();
                self.expect_p("(")?;
                let arg = self.place()?;
                self.expect_p(")")?;
                self.expect_p(";")?;
                return Ok(StmtKind::One { which: op, arg });
            }
            // `name();` — a call. (A place-index `name(ix)` has a non-empty
            // parenthesis; a call's is empty.)
            if !RESERVED.contains(&name.as_str()) && self.next_is_p("(") {
                let after = self.toks.get(self.pos + 2).map(|t| &t.tok);
                if matches!(after, Some(Tok::P(")"))) {
                    self.bump();
                    self.bump();
                    self.bump();
                    self.expect_p(";")?;
                    return Ok(StmtKind::Call(Target::Name(name)));
                }
            }
        }
        // Assignment / ++ / --.
        let dst = self.place()?;
        if self.eat_p("++") {
            self.expect_p(";")?;
            return Ok(StmtKind::Inc(dst));
        }
        if self.eat_p("--") {
            self.expect_p(";")?;
            return Ok(StmtKind::Dec(dst));
        }
        let op = match self.cur() {
            Tok::P("=") => AssignOp::Set,
            Tok::P("+=") => AssignOp::Add,
            Tok::P("-=") => AssignOp::Sub,
            Tok::P("*=") => AssignOp::Mul,
            Tok::P("/=") => AssignOp::Div,
            Tok::P("&=") => AssignOp::And,
            Tok::P("|=") => AssignOp::Or,
            Tok::P("^=") => AssignOp::Xor,
            Tok::P("<<=") => AssignOp::Sll,
            Tok::P(">>=") => AssignOp::Sra,
            Tok::P(">>>=") => AssignOp::Srl,
            other => return self.err(format!("expected an assignment operator, found {}", describe(other))),
        };
        self.bump();
        let src = self.operand()?;
        self.expect_p(";")?;
        Ok(StmtKind::Assign { dst, op, src })
    }

    fn target(&mut self) -> Result<Target, ParseError> {
        match self.cur().clone() {
            Tok::Ident(s) if !RESERVED.contains(&s.as_str()) => {
                self.bump();
                Ok(Target::Name(s))
            }
            _ => Ok(Target::Addr(self.expr()?)),
        }
    }

    fn operand(&mut self) -> Result<Operand, ParseError> {
        if self.starts_place() {
            Ok(Operand::Place(self.place()?))
        } else {
            let (expr, bare_zero) = self.expr_flagged()?;
            Ok(Operand::Imm(Imm { expr, bare_zero }))
        }
    }

    /// Does the current token open a place (rather than an immediate)?
    fn starts_place(&self) -> bool {
        match self.cur() {
            Tok::P("*") => true,
            Tok::Ident(s) => match s.as_str() {
                "cpu" | "vdp" | "byte" | "word" => true,
                s => !RESERVED.contains(&s),
            },
            _ => false,
        }
    }

    fn place(&mut self) -> Result<Place, ParseError> {
        // Prefix `*`: CPU-target indirection shorthand.
        if self.eat_p("*") {
            let mut p = self.place_core()?;
            if p.indirect {
                return self.err("doubly-indirect place");
            }
            if p.space != Space::Cpu {
                return self.err("prefix * is the CPU-indirect form; write vdp[*cell] for VDP");
            }
            p.indirect = true;
            return Ok(p);
        }
        self.place_core()
    }

    fn place_core(&mut self) -> Result<Place, ParseError> {
        // Cast wrapper.
        if (self.is_ident("byte") || self.is_ident("word")) && self.next_is_p("(") {
            let w = self.width()?;
            self.expect_p("(")?;
            let mut p = self.place()?;
            self.expect_p(")")?;
            if p.cast.is_some() {
                return self.err("place is cast twice");
            }
            p.cast = Some(w);
            return Ok(p);
        }
        if self.is_ident("cpu") || self.is_ident("vdp") {
            let space = if self.eat_ident("cpu") { Space::Cpu } else {
                self.bump();
                Space::Vdp
            };
            self.expect_p("[")?;
            let indirect = self.eat_p("*");
            let base = self.place_base()?;
            self.expect_p("]")?;
            let index = self.opt_index()?;
            return Ok(Place { space, base, indirect, index, cast: None, explicit: true });
        }
        // A bare var name. Space/width resolved in codegen.
        let name = self.user_ident()?;
        let index = self.opt_index()?;
        Ok(Place {
            space: Space::Cpu,
            base: PlaceBase::Var(name),
            indirect: false,
            index,
            cast: None,
            explicit: false,
        })
    }

    fn place_base(&mut self) -> Result<PlaceBase, ParseError> {
        match self.cur().clone() {
            Tok::Ident(s) if !RESERVED.contains(&s.as_str()) => {
                // A bare name inside brackets may be a var (its address) or a
                // const; codegen decides. Longer expressions must be consts.
                if self.next_is_p("]") {
                    self.bump();
                    return Ok(PlaceBase::Var(s));
                }
                Ok(PlaceBase::Addr(self.expr()?))
            }
            _ => Ok(PlaceBase::Addr(self.expr()?)),
        }
    }

    fn opt_index(&mut self) -> Result<Option<PlaceBase>, ParseError> {
        if !self.is_p("(") {
            return Ok(None);
        }
        self.bump();
        let base = match self.cur().clone() {
            Tok::Ident(s) if !RESERVED.contains(&s.as_str()) && self.next_is_p(")") => {
                self.bump();
                PlaceBase::Var(s)
            }
            _ => PlaceBase::Addr(self.expr()?),
        };
        self.expect_p(")")?;
        Ok(Some(base))
    }

    fn move_dst(&mut self) -> Result<MoveDst, ParseError> {
        if self.eat_ident("vreg") {
            self.expect_p("(")?;
            let e = self.expr()?;
            self.expect_p(")")?;
            return Ok(MoveDst::VReg(e));
        }
        if self.eat_ident("gram") {
            self.expect_p("[")?;
            let e = self.expr()?;
            self.expect_p("]")?;
            return Ok(MoveDst::Gram(e));
        }
        Ok(MoveDst::Place(self.place()?))
    }

    fn move_src(&mut self) -> Result<MoveSrc, ParseError> {
        if self.eat_ident("grom") {
            self.expect_p("[")?;
            if self.eat_p("*") {
                let cell = self.place_via_cell()?;
                self.expect_p("]")?;
                return Ok(MoveSrc::GromVia(cell));
            }
            let e = self.expr_or_name_addr()?;
            self.expect_p("]")?;
            return Ok(MoveSrc::Grom(e));
        }
        Ok(MoveSrc::Place(self.place()?))
    }

    /// Inside `grom[*…]`: the CPU cell holding the address — a var name or a
    /// numeric cell address.
    fn place_via_cell(&mut self) -> Result<Place, ParseError> {
        let base = self.place_base()?;
        Ok(Place { space: Space::Cpu, base, indirect: false, index: None, cast: None, explicit: true })
    }

    /// `grom[X]` accepts a symbol (data/fn/label name) or a const expression.
    fn expr_or_name_addr(&mut self) -> Result<Expr, ParseError> {
        match self.cur().clone() {
            Tok::Ident(s) if !RESERVED.contains(&s.as_str()) && self.next_is_p("]") => {
                self.bump();
                Ok(Expr::Name(s))
            }
            _ => self.expr(),
        }
    }

    fn move_count(&mut self) -> Result<MoveCount, ParseError> {
        if self.starts_place() {
            Ok(MoveCount::Place(self.place()?))
        } else {
            Ok(MoveCount::Imm(self.expr()?))
        }
    }

    // ---- conditions ---------------------------------------------------------

    fn cond(&mut self) -> Result<Cond, ParseError> {
        // `!cond()` / `!carry()` …
        if self.eat_p("!") {
            let name = self.any_ident()?;
            self.expect_p("(")?;
            self.expect_p(")")?;
            if name == "cond" {
                return Ok(Cond::CondBit { negated: true });
            }
            if let Some(&(_, s)) = STATUS_OPS.iter().find(|(n, _)| *n == name) {
                return Ok(Cond::Status { which: s, negated: true });
            }
            return self.err(format!("'!{name}()' is not a condition"));
        }
        // `(a & b) == 0` / `!= 0` — CLOG.
        if self.is_p("(") {
            self.bump();
            let a = self.place()?;
            self.expect_p("&")?;
            let b = self.operand()?;
            self.expect_p(")")?;
            let negated = if self.eat_p("==") {
                false
            } else if self.eat_p("!=") {
                true
            } else {
                return self.err("expected == 0 or != 0 after (a & b)");
            };
            match self.bump() {
                Tok::Num(0, true) => {}
                _ => return self.err("(a & b) compares only against bare 0"),
            }
            return Ok(Cond::Cmp { a, op: CmpOp::Log, b, negated });
        }
        // `cond()` / status ops — only when spelled with call parens.
        if let Tok::Ident(name) = self.cur().clone() {
            if self.next_is_p("(") && (name == "cond" || STATUS_OPS.iter().any(|(n, _)| *n == name))
            {
                self.bump();
                self.expect_p("(")?;
                self.expect_p(")")?;
                if name == "cond" {
                    return Ok(Cond::CondBit { negated: false });
                }
                let (_, s) = *STATUS_OPS.iter().find(|(n, _)| *n == name).unwrap();
                return Ok(Cond::Status { which: s, negated: false });
            }
        }
        // A compare: place OP operand.
        let a = self.place()?;
        let (op, negated) = match self.cur() {
            Tok::P("==") => (CmpOp::Eq, false),
            Tok::P("!=") => (CmpOp::Eq, true),
            Tok::P(">") => (CmpOp::Gt, false),
            Tok::P("<=") => (CmpOp::Gt, true),
            Tok::P(">=") => (CmpOp::Ge, false),
            Tok::P("<") => (CmpOp::Ge, true),
            Tok::P("h>") => (CmpOp::HGt, false),
            Tok::P("h<=") => (CmpOp::HGt, true),
            Tok::P("h>=") => (CmpOp::HGe, false),
            Tok::P("h<") => (CmpOp::HGe, true),
            other => return self.err(format!("expected a comparison, found {}", describe(other))),
        };
        self.bump();
        let b = self.operand()?;
        Ok(Cond::Cmp { a, op, b, negated })
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("'{s}'"),
        Tok::Num(v, _) => format!("number {v}"),
        Tok::Str(_) => "a string".into(),
        Tok::AsmBody(_) => "an asm block".into(),
        Tok::P(p) => format!("'{p}'"),
        Tok::Eof => "end of file".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Program {
        parse(src).unwrap_or_else(|e| panic!("line {}: {}", e.line, e.message))
    }

    #[test]
    fn items_parse() {
        let prog = p(r#"
            format ctg;
            cartridge "BEEP";
            origin 0x6000;
            const K = 0x20 + 3 * 2;
            var key: byte @ cpu[0x8375];
            data d { 0xAA, word 0x6010, "HI", 'Q', }
            fn main() @ 0x6021 {
            }
        "#);
        assert_eq!(prog.items.len(), 7);
    }

    #[test]
    fn statements_parse() {
        let prog = p(r#"
            var k: byte @ cpu[0x8375];
            var w: word @ cpu[0x8340];
            fn f() {
            top:
                k = 0;
                k = 0x00;
                w += 2;
                inct(w);
                k++;
                if (k == 0xFF) goto top;
                if (w h> 0x100) goto top;
                if ((k & 0x80) != 0) goto top;
                if (!carry()) goto top;
                test(k == 0);
                if (cond()) goto top;
                move(vdp[0x22], grom[0x1600], 5);
                move(vdp[*w], k, w);
                swap(k, w);
                scan();
                back(0x17);
                goto 0x6000;
                f2();
                call(0x6100);
                return;
            }
            fn f2() {
                asm {
        ST   @>8300,>01
                }
                returnc;
            }
        "#);
        assert_eq!(prog.items.len(), 4);
        match &prog.items[2] {
            Item::Func { body, .. } => {
                assert_eq!(body.len(), 20);
                assert_eq!(body[0].labels, vec!["top".to_string()]);
                // `k = 0;` is a bare zero; `k = 0x00;` is not.
                match (&body[0].kind, &body[1].kind) {
                    (
                        StmtKind::Assign { src: Operand::Imm(z), .. },
                        StmtKind::Assign { src: Operand::Imm(nz), .. },
                    ) => {
                        assert!(z.bare_zero && !nz.bare_zero);
                    }
                    other => panic!("unexpected: {other:?}"),
                }
            }
            other => panic!("expected fn, got {other:?}"),
        }
    }

    #[test]
    fn structured_sugar_parses() {
        p(r#"
            var k: byte @ cpu[0x8375];
            fn f() {
                while (k != 0xFF) {
                    scan();
                }
                if (k == 0x0D) {
                    k = 0;
                } else {
                    k--;
                }
            }
        "#);
    }

    #[test]
    fn reserved_words_are_rejected_as_names() {
        assert!(parse("var move: byte @ cpu[0x8300];").is_err());
        assert!(parse("const _Gx = 1;").is_err());
    }

    #[test]
    fn indexed_and_indirect_places() {
        p(r#"
            var t: byte @ cpu[0x8400];
            var ix: byte @ cpu[0x83E0];
            var ptr: word @ cpu[0x8372];
            fn f() {
                t(ix) = 0x01;
                *ptr = 0x02;
                vdp[*ptr] = 0x03;
                word(*ptr) = 0x1234;
                cpu[0x9000](0x83E1) += t;
            }
        "#);
    }
}
