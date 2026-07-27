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

//! The GSL compiler: AST → `libre99gpl` assembler source → GROM image.
//!
//! GSL never encodes ordinary instructions itself — it emits assembler
//! mnemonics and lets [`libre99_gpl::assemble_sized`] do the encoding, which
//! keeps inline `asm { }` blocks and GSL statements aligned by construction.
//! The one exception is the **byte path**: operand shapes the assembler's
//! grammar deliberately rejects (indexed places, GRAM `move` destinations) are
//! encoded here via [`libre99_gpl::encode`]/[`libre99_gpl::operand`] and
//! emitted as `BYTE` lines.
//!
//! Placement: items lay out at a running location counter; `@` pins emit
//! `GROM >addr` directives. A hidden `_GE<n>` label is planted before every
//! pin and checked after assembly, so a pin that would land *before* already-
//! emitted content (silent overlap) is a compile error instead.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use libre99_gpl::isa;
use libre99_gpl::operand::Operand as GOp;

use crate::ast::*;
use crate::parser;

/// One compile diagnostic, tied to a `.gsl` source line (0 = whole program).
#[derive(Debug, Clone)]
pub struct GslError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for GslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

/// A successful compile.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// Declared output format (`format …;`), if any.
    pub format: Option<OutFormat>,
    pub title: Option<String>,
    pub cru: u16,
    /// The assembled 64 KiB GROM space.
    pub image: Vec<u8>,
    /// 8 KiB GROM pages considered present: pages holding nonzero bytes plus
    /// `grompage` declarations.
    pub pages: BTreeSet<u16>,
    /// TMS9900 ROM banks (8 KiB each) from `rom N { }` blocks.
    pub rom_banks: Vec<Vec<u8>>,
    /// The generated assembler source (for tests and debugging).
    pub asm_source: String,
    /// Assembler symbol table (name → GROM address).
    pub symbols: Vec<(String, u16)>,
}

/// Compile GSL source text.
pub fn compile(src: &str) -> Result<Compiled, Vec<GslError>> {
    let prog = parser::parse(src)
        .map_err(|e| vec![GslError { line: e.line, message: e.message }])?;
    compile_program(&prog)
}

#[derive(Debug, Clone, Copy)]
struct VarInfo {
    space: Space,
    width: Width,
    addr: u16,
}

/// A resolved place: either expressible as assembler operand text, or (byte
/// path) as a concrete [`GOp`].
struct RPlace {
    space: Space,
    /// Numeric address, or a symbol left to the assembler.
    addr: Result<u16, String>,
    indirect: bool,
    /// Index cell as its `>83xx` offset byte.
    index: Option<u8>,
    claim: Option<Width>,
}

struct Gen {
    consts: HashMap<String, i64>,
    vars: HashMap<String, VarInfo>,
    /// All defined symbols (fns, data, labels) — for diagnostics only.
    code_syms: BTreeSet<String>,
    lines: Vec<String>,
    /// gsl source line for each generated asm line.
    map: Vec<usize>,
    errors: Vec<GslError>,
    /// (guard label, pin address, gsl line) for post-assembly overlap checks.
    pins: Vec<(String, u16, usize)>,
    fresh: usize,
}

pub fn compile_program(prog: &Program) -> Result<Compiled, Vec<GslError>> {
    let mut g = Gen {
        consts: HashMap::new(),
        vars: HashMap::new(),
        code_syms: BTreeSet::new(),
        lines: Vec::new(),
        map: Vec::new(),
        errors: Vec::new(),
        pins: Vec::new(),
        fresh: 0,
    };

    g.collect_names(prog);
    if !g.errors.is_empty() {
        return Err(g.errors);
    }

    // Header + var EQUs first: GAS operands may only reference symbols defined
    // earlier in the source (the assembler's backward-reference rule).
    g.emit(0, "* generated by libre99-gsl -- do not edit".into());
    let mut vars: Vec<(&String, &VarInfo)> = g.vars.iter().collect();
    vars.sort_by_key(|(n, _)| (*n).clone());
    for (name, info) in vars {
        g.lines.push(format!("{name} EQU >{:04X}", info.addr));
        g.map.push(0);
    }

    let mut format = None;
    let mut title = None;
    let mut cru: u16 = 0;
    let mut grompages: BTreeSet<u16> = BTreeSet::new();
    let mut rom_banks: BTreeMap<usize, Vec<u8>> = BTreeMap::new();

    for item in &prog.items {
        match item {
            Item::Format(f) => format = Some(*f),
            Item::Cartridge(t) => title = Some(t.clone()),
            Item::Cru(e) => match g.eval(e) {
                Ok(v) => cru = v as u16,
                Err(m) => g.error(0, m),
            },
            Item::Origin(e) => {
                let line = 0;
                match g.eval(e) {
                    Ok(v) => {
                        let addr = v as u16;
                        let guard = g.fresh_label("_GE");
                        g.emit(line, guard.clone());
                        g.emit(line, format!("        GROM >{addr:04X}"));
                        g.pins.push((guard, addr, line));
                    }
                    Err(m) => g.error(line, m),
                }
            }
            Item::GromPage(e) => match g.eval(e) {
                Ok(v) => {
                    grompages.insert((v as u16) & 0xE000);
                }
                Err(m) => g.error(0, m),
            },
            Item::Const { .. } | Item::Var { .. } => {}
            Item::Func { name, pin, body, line } => {
                g.pin(*line, pin);
                g.emit(*line, name.clone());
                for s in body {
                    g.stmt(s);
                }
            }
            Item::Data { name, pin, items, line } => {
                g.pin(*line, pin);
                if let Some(n) = name {
                    g.emit(*line, n.clone());
                }
                g.data_items(*line, items);
            }
            Item::AsmBlock { lines, line } => {
                for l in lines {
                    g.emit(*line, l.clone());
                }
            }
            Item::Rom { bank, items, line } => {
                let idx = match g.eval(bank) {
                    Ok(v) if (0..=255).contains(&v) => v as usize,
                    Ok(v) => {
                        g.error(*line, format!("rom bank {v} out of range (0..=255)"));
                        continue;
                    }
                    Err(m) => {
                        g.error(*line, m);
                        continue;
                    }
                };
                let bytes = g.rom_bytes(*line, items);
                if rom_banks.insert(idx, bytes).is_some() {
                    g.error(*line, format!("rom bank {idx} defined twice"));
                }
            }
        }
    }

    if !g.errors.is_empty() {
        return Err(g.errors);
    }

    // Assemble the generated source over the full GROM space.
    let asm_source = g.lines.join("\n") + "\n";
    let asm = match libre99_gpl::assemble_sized(&asm_source, libre99_gpl::GROM_SPACE_LEN) {
        Ok(a) => a,
        Err(diags) => {
            let errs = diags
                .into_iter()
                .map(|d| GslError {
                    line: if d.line == 0 { 0 } else { *g.map.get(d.line - 1).unwrap_or(&0) },
                    message: format!("[asm] {}", d.message),
                })
                .collect();
            return Err(errs);
        }
    };

    // Pins must not land before already-emitted content.
    let sym = |n: &str| asm.symbols.iter().find(|(s, _)| s == n).map(|(_, v)| *v);
    let mut errors = Vec::new();
    for (guard, pin, line) in &g.pins {
        if let Some(end) = sym(guard) {
            if end > *pin {
                errors.push(GslError {
                    line: *line,
                    message: format!(
                        "@ 0x{pin:04X} pin overlaps preceding content (location counter is already at 0x{end:04X})"
                    ),
                });
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Contiguous ROM banks 0..n.
    let mut banks = Vec::new();
    for (i, (idx, b)) in rom_banks.into_iter().enumerate() {
        if idx != i {
            return Err(vec![GslError {
                line: 0,
                message: format!("rom banks must be contiguous from 0 (bank {idx} follows {i})",),
            }]);
        }
        banks.push(b);
    }

    // Pages: nonzero content plus declarations.
    let mut pages = grompages;
    for (i, b) in asm.image.iter().enumerate() {
        if *b != 0 {
            pages.insert((i as u16) & 0xE000);
        }
    }

    let symbols =
        asm.symbols.iter().filter(|(n, _)| !n.starts_with("_G")).cloned().collect::<Vec<_>>();
    Ok(Compiled {
        format,
        title,
        cru,
        image: asm.image,
        pages,
        rom_banks: banks,
        asm_source,
        symbols,
    })
}

impl Gen {
    fn error(&mut self, line: usize, message: impl Into<String>) {
        self.errors.push(GslError { line, message: message.into() });
    }

    fn emit(&mut self, line: usize, text: String) {
        self.lines.push(text);
        self.map.push(line);
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{:04}", self.fresh)
    }

    fn pin(&mut self, line: usize, pin: &Option<Expr>) {
        if let Some(e) = pin {
            match self.eval(e) {
                Ok(v) => {
                    let addr = v as u16;
                    let guard = self.fresh_label("_GE");
                    self.emit(line, guard.clone());
                    self.emit(line, format!("        GROM >{addr:04X}"));
                    self.pins.push((guard, addr, line));
                }
                Err(m) => self.error(line, m),
            }
        }
    }

    // ---- name collection ---------------------------------------------------

    fn collect_names(&mut self, prog: &Program) {
        // Consts first (with cycle detection), then vars, then code symbols.
        let mut exprs: HashMap<String, (Expr, usize)> = HashMap::new();
        let mut order = Vec::new();
        let mut all: HashMap<String, usize> = HashMap::new();
        let mut declare = |name: &String, line: usize, errors: &mut Vec<GslError>| {
            if let Some(prev) = all.insert(name.clone(), line) {
                errors.push(GslError {
                    line,
                    message: format!("'{name}' is already defined (line {prev})"),
                });
            }
        };
        for item in &prog.items {
            match item {
                Item::Const { name, value, line } => {
                    declare(name, *line, &mut self.errors);
                    exprs.insert(name.clone(), (value.clone(), *line));
                    order.push(name.clone());
                }
                Item::Var { name, line, .. } => declare(name, *line, &mut self.errors),
                Item::Func { name, line, body, .. } => {
                    declare(name, *line, &mut self.errors);
                    self.code_syms.insert(name.clone());
                    collect_labels(body, *line, &mut |l, ln| {
                        declare(&l, ln, &mut self.errors);
                        self.code_syms.insert(l);
                    });
                }
                Item::Data { name: Some(name), line, .. } => {
                    declare(name, *line, &mut self.errors);
                    self.code_syms.insert(name.clone());
                }
                _ => {}
            }
        }
        // Resolve consts (values may reference other consts, forward or back).
        for name in &order {
            let mut visiting = BTreeSet::new();
            if let Err(m) = self.const_value(name, &exprs, &mut visiting) {
                let line = exprs.get(name).map(|(_, l)| *l).unwrap_or(0);
                self.error(line, m);
            }
        }
        // Vars (addresses may use consts).
        for item in &prog.items {
            if let Item::Var { name, width, space, addr, line } = item {
                match self.eval(addr) {
                    Ok(v) => {
                        let addr = v as u16;
                        if *space == Space::Vdp && addr > 0x3FFF {
                            self.error(*line, format!("VDP address 0x{addr:04X} exceeds 0x3FFF"));
                        }
                        self.vars.insert(
                            name.clone(),
                            VarInfo { space: *space, width: *width, addr },
                        );
                    }
                    Err(m) => self.error(*line, m),
                }
            }
        }
    }

    fn const_value(
        &mut self,
        name: &str,
        exprs: &HashMap<String, (Expr, usize)>,
        visiting: &mut BTreeSet<String>,
    ) -> Result<i64, String> {
        if let Some(v) = self.consts.get(name) {
            return Ok(*v);
        }
        let Some((expr, _)) = exprs.get(name) else {
            return Err(format!("'{name}' is not a constant"));
        };
        if !visiting.insert(name.to_string()) {
            return Err(format!("const '{name}' is defined in terms of itself"));
        }
        let v = self.eval_with(expr, &|n| {
            // Nested lookup: resolve dependencies recursively.
            if let Some(v) = self.consts.get(n) {
                return Some(*v);
            }
            None
        });
        let v = match v {
            Ok(v) => v,
            Err(_) => {
                // Retry after resolving dependencies depth-first.
                let deps = names_in(expr);
                for d in deps {
                    if d != name && exprs.contains_key(&d) {
                        let val = self.const_value(&d, exprs, visiting)?;
                        self.consts.insert(d, val);
                    }
                }
                self.eval(expr)?
            }
        };
        visiting.remove(name);
        self.consts.insert(name.to_string(), v);
        Ok(v)
    }

    // ---- expression evaluation ----------------------------------------------

    fn eval(&self, e: &Expr) -> Result<i64, String> {
        self.eval_with(e, &|n| self.consts.get(n).copied())
    }

    fn eval_with(&self, e: &Expr, lookup: &dyn Fn(&str) -> Option<i64>) -> Result<i64, String> {
        match e {
            Expr::Num(v) => Ok(*v),
            Expr::Name(n) => {
                lookup(n).ok_or_else(|| format!("'{n}' is not a constant (labels are not allowed in expressions)"))
            }
            Expr::Unary('-', a) => Ok(self.eval_with(a, lookup)?.wrapping_neg()),
            Expr::Unary('~', a) => Ok(!self.eval_with(a, lookup)?),
            Expr::Unary(op, _) => Err(format!("unknown unary operator '{op}'")),
            Expr::Binary(op, a, b) => {
                let a = self.eval_with(a, lookup)?;
                let b = self.eval_with(b, lookup)?;
                Ok(match *op {
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "*" => a.wrapping_mul(b),
                    "/" => {
                        if b == 0 {
                            return Err("division by zero".into());
                        }
                        a / b
                    }
                    "%" => {
                        if b == 0 {
                            return Err("modulo by zero".into());
                        }
                        a % b
                    }
                    "&" => a & b,
                    "|" => a | b,
                    "^" => a ^ b,
                    "<<" => a.wrapping_shl(b as u32),
                    ">>" => a.wrapping_shr(b as u32),
                    other => return Err(format!("unknown operator '{other}'")),
                })
            }
        }
    }

    /// An immediate: a numeric value, or a bare symbol for the assembler.
    fn imm_text(&self, e: &Expr, width: Width) -> Result<String, String> {
        match e {
            // A non-const symbol (var address, label, or a name defined inside
            // an asm block) — leave it for the assembler to resolve.
            Expr::Name(n) if !self.consts.contains_key(n.as_str()) => Ok(n.clone()),
            _ => {
                let v = self.eval(e)?;
                Ok(match width {
                    Width::Byte => {
                        if !(-128..=255).contains(&v) {
                            return Err(format!("immediate {v} does not fit a byte"));
                        }
                        format!(">{:02X}", v as u8)
                    }
                    Width::Word => {
                        if !(-32768..=65535).contains(&v) {
                            return Err(format!("immediate {v} does not fit a word"));
                        }
                        format!(">{:04X}", v as u16)
                    }
                })
            }
        }
    }

    // ---- places -------------------------------------------------------------

    fn resolve_place(&self, p: &Place) -> Result<RPlace, String> {
        let mut space = p.space;
        let mut claim = p.cast;
        let addr: Result<u16, String>;
        match &p.base {
            PlaceBase::Var(name) => {
                if let Some(v) = self.vars.get(name.as_str()) {
                    // A bare var name adopts the var's own space; explicit
                    // `cpu[…]`/`vdp[…]` spellings must agree.
                    if matches!(p.space, Space::Vdp) && v.space == Space::Cpu && !p.indirect {
                        return Err(format!("'{name}' is a CPU var, not VDP"));
                    }
                    if p.indirect {
                        // The cell holding the pointer is always CPU-side.
                        if v.space != Space::Cpu {
                            return Err(format!("'{name}' must be a CPU cell to hold a pointer"));
                        }
                    } else {
                        space = v.space;
                        if claim.is_none() {
                            claim = Some(v.width);
                        }
                    }
                    addr = Ok(v.addr);
                } else if let Some(v) = self.consts.get(name.as_str()) {
                    addr = Ok(*v as u16);
                } else {
                    // A label/data symbol used as an address: only expressible
                    // through assembler text (backward references only).
                    addr = Err(name.clone());
                }
            }
            PlaceBase::Addr(e) => {
                addr = Ok(self.eval(e)? as u16);
            }
        }
        let index = match &p.index {
            None => None,
            Some(base) => {
                let v = match base {
                    PlaceBase::Var(n) => self
                        .vars
                        .get(n.as_str())
                        .map(|v| v.addr as i64)
                        .or_else(|| self.consts.get(n.as_str()).copied())
                        .ok_or_else(|| format!("index cell '{n}' is not a var or const"))?,
                    PlaceBase::Addr(e) => self.eval(e)?,
                };
                if !(0x8300..=0x83FF).contains(&v) {
                    return Err(format!(
                        "index cell 0x{v:04X} must be a scratchpad cell (0x8300..=0x83FF)"
                    ));
                }
                Some((v - 0x8300) as u8)
            }
        };
        if space == Space::Vdp && !p.indirect {
            if let Ok(a) = addr {
                if a > 0x3FFF {
                    return Err(format!("VDP address 0x{a:04X} exceeds 0x3FFF"));
                }
            }
        }
        Ok(RPlace { space, addr, indirect: p.indirect, index, claim })
    }

    /// Assembler operand text for a resolved place (no index — that is the
    /// byte path's job).
    fn place_text(&self, p: &Place, r: &RPlace) -> Result<String, String> {
        debug_assert!(r.index.is_none());
        let star = if r.indirect { "*" } else { "" };
        let prefix = match r.space {
            Space::Cpu => "@",
            Space::Vdp => "V@",
        };
        let addr = match (&r.addr, &p.base) {
            (_, PlaceBase::Var(n)) if self.vars.contains_key(n.as_str()) => n.clone(),
            (Ok(a), _) => format!(">{a:04X}"),
            (Err(sym), _) => sym.clone(),
        };
        Ok(format!("{star}{prefix}{addr}"))
    }

    /// A concrete [`GOp`] for the byte path (requires numeric addresses).
    fn place_gop(&self, r: &RPlace) -> Result<GOp, String> {
        let addr = r.addr.clone().map_err(|sym| {
            format!("'{sym}' has no compile-time address (needed for this operand shape)")
        })?;
        Ok(match r.space {
            Space::Cpu => GOp::Cpu { addr, indirect: r.indirect, index: r.index },
            Space::Vdp => GOp::Vdp { addr, indirect: r.indirect, index: r.index },
        })
    }

    fn merge_width(
        &self,
        line: usize,
        claims: &[Option<Width>],
    ) -> Width {
        let mut w = None;
        for c in claims.iter().flatten() {
            match w {
                None => w = Some(*c),
                Some(prev) if prev != *c => {
                    // Report once; byte wins deterministically after the error.
                    return Width::Byte;
                }
                _ => {}
            }
        }
        let _ = line;
        w.unwrap_or(Width::Byte)
    }

    fn widths_conflict(claims: &[Option<Width>]) -> bool {
        claims.iter().flatten().any(|c| *c == Width::Byte)
            && claims.iter().flatten().any(|c| *c == Width::Word)
    }

    // ---- statements ---------------------------------------------------------

    fn stmt(&mut self, s: &Stmt) {
        for l in &s.labels {
            self.emit(s.line, l.clone());
        }
        if let Err(m) = self.stmt_kind(s.line, &s.kind) {
            self.error(s.line, m);
        }
    }

    fn stmt_kind(&mut self, line: usize, k: &StmtKind) -> Result<(), String> {
        match k {
            StmtKind::Empty => Ok(()),
            StmtKind::Asm(lines) => {
                for l in lines {
                    self.emit(line, l.clone());
                }
                Ok(())
            }
            StmtKind::Assign { dst, op, src } => self.assign(line, dst, *op, src),
            StmtKind::Inc(p) => self.one_op(line, "INC", p),
            StmtKind::Dec(p) => self.one_op(line, "DEC", p),
            StmtKind::One { which, arg } => {
                let m = match which {
                    OneOp::Inct => "INCT",
                    OneOp::Dect => "DECT",
                    OneOp::Abs => "ABS",
                    OneOp::Neg => "NEG",
                    OneOp::Inv => "INV",
                    OneOp::Push => "PUSH",
                    OneOp::Fetch => "FETCH",
                    OneOp::Case => "CASE",
                };
                self.one_op(line, m, arg)
            }
            StmtKind::Rotr { dst, count } => self.two_op(line, "SRC", dst, count),
            StmtKind::Swap { a, b } => self.two_op(line, "EX", a, &Operand::Place(b.clone())),
            StmtKind::Move { dst, src, count } => self.move_stmt(line, dst, src, count),
            StmtKind::IfGoto { cond, target } => {
                self.cond_branch(line, cond, false, &self.target_text(target)?)
            }
            StmtKind::Test(cond) => self.test_only(line, cond),
            StmtKind::Goto(t) => {
                let t = self.target_text(t)?;
                self.emit(line, format!("        B    {t}"));
                Ok(())
            }
            StmtKind::Call(t) => {
                let t = self.target_text(t)?;
                self.emit(line, format!("        CALL {t}"));
                Ok(())
            }
            StmtKind::Return => {
                self.emit(line, "        RTN".into());
                Ok(())
            }
            StmtKind::ReturnC => {
                self.emit(line, "        RTNC".into());
                Ok(())
            }
            StmtKind::Zero(z) => {
                let m = match z {
                    ZeroOp::Scan => "SCAN",
                    ZeroOp::Exit => "EXIT",
                    ZeroOp::Cont => "CONT",
                    ZeroOp::Exec => "EXEC",
                    ZeroOp::Rtnb => "RTNB",
                    ZeroOp::Rtgr => "RTGR",
                };
                self.emit(line, format!("        {m}"));
                Ok(())
            }
            StmtKind::ImmArg { which, arg } => {
                let m = match which {
                    ImmOp::Back => "BACK",
                    ImmOp::All => "ALL",
                    ImmOp::Rand => "RAND",
                    ImmOp::Parse => "PARSE",
                    ImmOp::Xml => "XML",
                };
                let v = self.imm_text(arg, Width::Byte)?;
                self.emit(line, format!("        {m} {v}"));
                Ok(())
            }
            StmtKind::If { cond, then_, else_ } => {
                let end = self.fresh_label("_GL");
                let alt = if else_.is_empty() { end.clone() } else { self.fresh_label("_GL") };
                self.cond_branch(line, cond, true, &alt)?;
                for s in then_ {
                    self.stmt(s);
                }
                if !else_.is_empty() {
                    self.emit(line, format!("        B    {end}"));
                    self.emit(line, alt);
                    for s in else_ {
                        self.stmt(s);
                    }
                }
                self.emit(line, end);
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let top = self.fresh_label("_GL");
                let end = self.fresh_label("_GL");
                self.emit(line, top.clone());
                self.cond_branch(line, cond, true, &end)?;
                for s in body {
                    self.stmt(s);
                }
                self.emit(line, format!("        B    {top}"));
                self.emit(line, end);
                Ok(())
            }
        }
    }

    fn target_text(&self, t: &Target) -> Result<String, String> {
        match t {
            Target::Name(n) => {
                if let Some(v) = self.consts.get(n.as_str()) {
                    Ok(format!(">{:04X}", *v as u16))
                } else {
                    Ok(n.clone())
                }
            }
            Target::Addr(e) => Ok(format!(">{:04X}", self.eval(e)? as u16)),
        }
    }

    fn one_op(&mut self, line: usize, stem: &str, p: &Place) -> Result<(), String> {
        let r = self.resolve_place(p)?;
        let w = self.claimed(line, &[r.claim])?;
        let mnem = with_width(stem, w);
        if r.index.is_some() {
            let base = isa_one_base(stem)? | wbit(w);
            let bytes = libre99_gpl::encode::encode(base, isa::Sig::Gas, &[self.place_gop(&r)?])?;
            self.emit_bytes(line, &bytes, &mnem);
            return Ok(());
        }
        let t = self.place_text(p, &r)?;
        self.emit(line, format!("        {mnem} {t}"));
        Ok(())
    }

    fn assign(&mut self, line: usize, dst: &Place, op: AssignOp, src: &Operand) -> Result<(), String> {
        // Canonical rule: `P = 0;` (bare zero) is CLR.
        if op == AssignOp::Set {
            if let Operand::Imm(imm) = src {
                if imm.bare_zero {
                    return self.one_op(line, "CLR", dst);
                }
            }
        }
        let stem = match op {
            AssignOp::Set => "ST",
            AssignOp::Add => "ADD",
            AssignOp::Sub => "SUB",
            AssignOp::Mul => "MUL",
            AssignOp::Div => "DIV",
            AssignOp::And => "AND",
            AssignOp::Or => "OR",
            AssignOp::Xor => "XOR",
            AssignOp::Sll => "SLL",
            AssignOp::Sra => "SRA",
            AssignOp::Srl => "SRL",
        };
        self.two_op(line, stem, dst, src)
    }

    /// A *bare* name used where an operand may be either a place or an
    /// immediate: if it is not a declared var, it means the symbol's value
    /// (a const, or a label/function/data address for the assembler).
    fn bare_symbol(&self, p: &Place) -> Option<Expr> {
        if p.explicit || p.indirect || p.index.is_some() || p.cast.is_some() {
            return None;
        }
        match &p.base {
            PlaceBase::Var(n) if !self.vars.contains_key(n.as_str()) => {
                Some(Expr::Name(n.clone()))
            }
            _ => None,
        }
    }

    fn two_op(&mut self, line: usize, stem: &str, dst: &Place, src: &Operand) -> Result<(), String> {
        let coerced;
        let src = match src {
            Operand::Place(p) => match self.bare_symbol(p) {
                Some(expr) => {
                    coerced = Operand::Imm(Imm { expr, bare_zero: false });
                    &coerced
                }
                None => src,
            },
            _ => src,
        };
        let rd = self.resolve_place(dst)?;
        let (src_claim, src_indexed) = match src {
            Operand::Place(p) => {
                let r = self.resolve_place(p)?;
                (r.claim, r.index.is_some())
            }
            Operand::Imm(_) => (None, false),
        };
        let w = self.claimed(line, &[rd.claim, src_claim])?;
        let mnem = with_width(stem, w);

        if rd.index.is_some() || src_indexed {
            // Byte path.
            let base = isa_two_base(stem)? | wbit(w);
            let mut ops = vec![self.place_gop(&rd)?];
            let (opcode, sig) = match src {
                Operand::Place(p) => {
                    let rs = self.resolve_place(p)?;
                    ops.push(self.place_gop(&rs)?);
                    (base, isa::Sig::GasGas)
                }
                Operand::Imm(imm) => {
                    let v = self.eval(&imm.expr)?;
                    match w {
                        Width::Byte => {
                            if !(-128..=255).contains(&v) {
                                return Err(format!("immediate {v} does not fit a byte"));
                            }
                            ops.push(GOp::Imm8(v as u8));
                            (base | 0x02, isa::Sig::GasImm8)
                        }
                        Width::Word => {
                            if !(-32768..=65535).contains(&v) {
                                return Err(format!("immediate {v} does not fit a word"));
                            }
                            ops.push(GOp::Imm16(v as u16));
                            (base | 0x02, isa::Sig::GasImm16)
                        }
                    }
                }
            };
            let bytes = libre99_gpl::encode::encode(opcode, sig, &ops)?;
            self.emit_bytes(line, &bytes, &mnem);
            return Ok(());
        }

        let dt = self.place_text(dst, &rd)?;
        let st = match src {
            Operand::Place(p) => {
                let rs = self.resolve_place(p)?;
                self.place_text(p, &rs)?
            }
            Operand::Imm(imm) => self.imm_text(&imm.expr, w)?,
        };
        self.emit(line, format!("        {mnem} {dt},{st}"));
        Ok(())
    }

    fn claimed(&mut self, line: usize, claims: &[Option<Width>]) -> Result<Width, String> {
        if Self::widths_conflict(claims) {
            return Err("mixed byte/word operands (alias a var or add a byte()/word() cast)".into());
        }
        Ok(self.merge_width(line, claims))
    }

    /// Emit a byte-path instruction as a `BYTE` line.
    fn emit_bytes(&mut self, line: usize, bytes: &[u8], why: &str) {
        let list: Vec<String> = bytes.iter().map(|b| format!(">{b:02X}")).collect();
        self.emit(line, format!("        BYTE {} ; byte path: {why}", list.join(",")));
    }

    // ---- conditions ----------------------------------------------------------

    /// Emit the compare/status op (if any) and the branch. `invert` flips the
    /// branch sense (used by the structured forms, which branch *around* their
    /// bodies).
    fn cond_branch(
        &mut self,
        line: usize,
        cond: &Cond,
        invert: bool,
        target: &str,
    ) -> Result<(), String> {
        let negated = match cond {
            Cond::Cmp { a, op, b, negated } => {
                self.emit_compare(line, a, *op, b)?;
                *negated
            }
            Cond::Status { which, negated } => {
                let m = match which {
                    StatusOp::Carry => "CARRY",
                    StatusOp::Ovf => "OVF",
                    StatusOp::Gt => "GT",
                    StatusOp::H => "H",
                };
                self.emit(line, format!("        {m}"));
                *negated
            }
            Cond::CondBit { negated } => *negated,
        };
        let br = if negated != invert { "BR" } else { "BS" };
        self.emit(line, format!("        {br}   {target}"));
        Ok(())
    }

    fn test_only(&mut self, line: usize, cond: &Cond) -> Result<(), String> {
        match cond {
            Cond::Cmp { a, op, b, negated } => {
                if *negated {
                    return Err("test() takes the positive comparison (negation lives on the branch)".into());
                }
                self.emit_compare(line, a, *op, b)
            }
            Cond::Status { which, negated } => {
                if *negated {
                    return Err("test() takes the positive form".into());
                }
                let m = match which {
                    StatusOp::Carry => "CARRY",
                    StatusOp::Ovf => "OVF",
                    StatusOp::Gt => "GT",
                    StatusOp::H => "H",
                };
                self.emit(line, format!("        {m}"));
                Ok(())
            }
            Cond::CondBit { .. } => Err("test(cond()) has no meaning".into()),
        }
    }

    fn emit_compare(&mut self, line: usize, a: &Place, op: CmpOp, b: &Operand) -> Result<(), String> {
        // Canonical rule: `== 0` (bare zero) is CZ.
        if op == CmpOp::Eq {
            if let Operand::Imm(imm) = b {
                if imm.bare_zero {
                    return self.one_op(line, "CZ", a);
                }
            }
        }
        let stem = match op {
            CmpOp::Eq => "CEQ",
            CmpOp::Gt => "CGT",
            CmpOp::Ge => "CGE",
            CmpOp::HGt => "CH",
            CmpOp::HGe => "CHE",
            CmpOp::Log => "CLOG",
        };
        self.two_op(line, stem, a, b)
    }

    // ---- move ----------------------------------------------------------------

    fn move_stmt(
        &mut self,
        line: usize,
        dst: &MoveDst,
        src: &MoveSrc,
        count: &MoveCount,
    ) -> Result<(), String> {
        // Resolve shapes; decide mnemonic path vs byte path.
        let mut byte_path = false;
        let rdst = match dst {
            MoveDst::Place(p) => {
                let r = self.resolve_place(p)?;
                if r.index.is_some() {
                    byte_path = true;
                }
                Some(r)
            }
            MoveDst::Gram(_) => {
                byte_path = true;
                None
            }
            MoveDst::VReg(_) => None,
        };
        let rsrc = match src {
            MoveSrc::Place(p) | MoveSrc::GromVia(p) => {
                let r = self.resolve_place(p)?;
                if r.index.is_some() || (matches!(src, MoveSrc::GromVia(_)) && (r.indirect || r.space != Space::Cpu)) {
                    byte_path = true;
                }
                Some(r)
            }
            MoveSrc::Grom(_) => None,
        };
        let coerced_count;
        let count = match count {
            MoveCount::Place(p) => match self.bare_symbol(p) {
                Some(expr) => {
                    coerced_count = MoveCount::Imm(expr);
                    &coerced_count
                }
                None => count,
            },
            _ => count,
        };
        let rcnt = match count {
            MoveCount::Place(p) => {
                let r = self.resolve_place(p)?;
                if r.index.is_some() {
                    byte_path = true;
                }
                Some(r)
            }
            MoveCount::Imm(_) => None,
        };

        if !byte_path {
            let cnt = match (count, &rcnt) {
                (MoveCount::Imm(e), _) => self.imm_text(e, Width::Word)?,
                (MoveCount::Place(p), Some(r)) => self.place_text(p, r)?,
                _ => unreachable!(),
            };
            let s = match (src, &rsrc) {
                (MoveSrc::Grom(e), _) => match e {
                    Expr::Name(n) if !self.consts.contains_key(n.as_str()) => format!("G@{n}"),
                    _ => format!("G@>{:04X}", self.eval(e)? as u16),
                },
                (MoveSrc::GromVia(p), Some(r)) => format!("G*{}", self.place_text(p, r)?),
                (MoveSrc::Place(p), Some(r)) => self.place_text(p, r)?,
                _ => unreachable!(),
            };
            let d = match (dst, &rdst) {
                (MoveDst::VReg(e), _) => format!("#>{:02X}", self.eval(e)? as u8),
                (MoveDst::Place(p), Some(r)) => self.place_text(p, r)?,
                _ => unreachable!(),
            };
            self.emit(line, format!("        MOVE {cnt},{s},{d}"));
            return Ok(());
        }

        // Byte path: encode the whole MOVE here.
        let mut bits = isa::MoveBits {
            not_grom_dst: !matches!(dst, MoveDst::Gram(_)),
            reg_dst: matches!(dst, MoveDst::VReg(_)),
            ram_src: matches!(src, MoveSrc::Place(_)),
            cpu_held_grom_src: matches!(src, MoveSrc::GromVia(_)),
            imm_count: matches!(count, MoveCount::Imm(_)),
        };
        if !bits.not_grom_dst {
            bits.reg_dst = false;
        }
        let mut bytes = vec![bits.opcode()];
        match (count, &rcnt) {
            (MoveCount::Imm(e), _) => {
                let v = self.eval(e)? as u16;
                bytes.push((v >> 8) as u8);
                bytes.push(v as u8);
            }
            (MoveCount::Place(_), Some(r)) => {
                libre99_gpl::operand::encode_gas(&self.place_gop(r)?, &mut bytes)?
            }
            _ => unreachable!(),
        }
        match (dst, &rdst) {
            (MoveDst::VReg(e), _) => bytes.push(self.eval(e)? as u8),
            (MoveDst::Gram(e), _) => {
                let v = self.eval(e)? as u16;
                bytes.push((v >> 8) as u8);
                bytes.push(v as u8);
            }
            (MoveDst::Place(_), Some(r)) => {
                libre99_gpl::operand::encode_gas(&self.place_gop(r)?, &mut bytes)?
            }
            _ => unreachable!(),
        }
        match (src, &rsrc) {
            (MoveSrc::Grom(e), _) => {
                let v = self.eval(e)? as u16;
                bytes.push((v >> 8) as u8);
                bytes.push(v as u8);
            }
            (MoveSrc::GromVia(_) | MoveSrc::Place(_), Some(r)) => {
                libre99_gpl::operand::encode_gas(&self.place_gop(r)?, &mut bytes)?
            }
            _ => unreachable!(),
        }
        self.emit_bytes(line, &bytes, "MOVE");
        Ok(())
    }

    // ---- data ---------------------------------------------------------------

    fn data_items(&mut self, line: usize, items: &[DataItem]) {
        let mut bytes: Vec<String> = Vec::new();
        macro_rules! flush {
            () => {
                for chunk in bytes.chunks(8) {
                    let l = format!("        BYTE {}", chunk.join(","));
                    self.emit(line, l);
                }
                bytes.clear();
            };
        }
        for item in items {
            match item {
                DataItem::Byte(e) => match self.byte_item_text(e) {
                    Ok(t) => bytes.push(t),
                    Err(m) => self.error(line, m),
                },
                DataItem::Str(s) => {
                    for b in s {
                        bytes.push(format!(">{b:02X}"));
                    }
                }
                DataItem::Word(e) => {
                    flush!();
                    match e {
                        Expr::Name(n) if !self.consts.contains_key(n.as_str()) => {
                            self.emit(line, format!("        DATA {n}"));
                        }
                        _ => match self.eval(e) {
                            Ok(v) if (-32768..=65535).contains(&v) => {
                                self.emit(line, format!("        DATA >{:04X}", v as u16));
                            }
                            Ok(v) => self.error(line, format!("word value {v} out of range")),
                            Err(m) => self.error(line, m),
                        },
                    }
                }
            }
        }
        flush!();
    }

    fn byte_item_text(&self, e: &Expr) -> Result<String, String> {
        match e {
            Expr::Name(n) if !self.consts.contains_key(n.as_str()) => Ok(n.clone()),
            _ => {
                let v = self.eval(e)?;
                if !(-128..=255).contains(&v) {
                    return Err(format!("byte value {v} out of range"));
                }
                Ok(format!(">{:02X}", v as u8))
            }
        }
    }

    fn rom_bytes(&mut self, line: usize, items: &[DataItem]) -> Vec<u8> {
        let mut out = Vec::new();
        for item in items {
            match item {
                DataItem::Byte(e) => match self.eval(e) {
                    Ok(v) if (-128..=255).contains(&v) => out.push(v as u8),
                    Ok(v) => self.error(line, format!("rom byte value {v} out of range")),
                    Err(m) => self.error(line, format!("rom bytes must be numeric: {m}")),
                },
                DataItem::Str(s) => out.extend_from_slice(s),
                DataItem::Word(e) => match self.eval(e) {
                    Ok(v) if (-32768..=65535).contains(&v) => {
                        out.push(((v as u16) >> 8) as u8);
                        out.push(v as u8);
                    }
                    Ok(v) => self.error(line, format!("rom word value {v} out of range")),
                    Err(m) => self.error(line, format!("rom words must be numeric: {m}")),
                },
            }
        }
        if out.len() > 0x2000 {
            self.error(line, format!("rom bank is {} bytes (max 8192)", out.len()));
            out.truncate(0x2000);
        }
        out.resize(0x2000, 0);
        out
    }
}

fn collect_labels(body: &[Stmt], line: usize, f: &mut impl FnMut(String, usize)) {
    for s in body {
        for l in &s.labels {
            f(l.clone(), s.line.max(line));
        }
        match &s.kind {
            StmtKind::If { then_, else_, .. } => {
                collect_labels(then_, line, f);
                collect_labels(else_, line, f);
            }
            StmtKind::While { body, .. } => collect_labels(body, line, f),
            _ => {}
        }
    }
}

fn names_in(e: &Expr) -> Vec<String> {
    match e {
        Expr::Num(_) => Vec::new(),
        Expr::Name(n) => vec![n.clone()],
        Expr::Unary(_, a) => names_in(a),
        Expr::Binary(_, a, b) => {
            let mut v = names_in(a);
            v.extend(names_in(b));
            v
        }
    }
}

fn with_width(stem: &str, w: Width) -> String {
    match w {
        Width::Byte => stem.to_string(),
        Width::Word => format!("D{stem}"),
    }
}

fn wbit(w: Width) -> u8 {
    match w {
        Width::Byte => 0,
        Width::Word => 1,
    }
}

fn isa_two_base(stem: &str) -> Result<u8, String> {
    isa::TWO_OPS
        .iter()
        .find(|t| t.name == stem)
        .map(|t| t.base)
        .ok_or_else(|| format!("no two-op family '{stem}'"))
}

fn isa_one_base(stem: &str) -> Result<u8, String> {
    isa::ONE_OPS
        .iter()
        .find(|o| o.name == stem)
        .map(|o| o.base)
        .ok_or_else(|| format!("no one-op family '{stem}'"))
}
