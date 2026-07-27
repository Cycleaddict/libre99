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

//! The GSL abstract syntax tree — what `docs/GSL.md` specifies, structurally.
//!
//! Design notes that matter for round-tripping:
//!
//! * [`Imm::bare_zero`] records whether a zero literal was spelled as the bare
//!   token `0` — the canonical-spelling rule (`GSL.md` §6.1) that selects
//!   `CLR`/`CZ` over the general immediate forms.
//! * Every GPL opcode family has exactly one [`StmtKind`] spelling, so the
//!   decompiler can regenerate the byte it decoded (the mapping tables live in
//!   `codegen.rs`).

/// Operation width — GPL's `W` bit (`ST` vs `DST`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Byte,
    Word,
}

/// Writable address space of a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Cpu,
    Vdp,
}

/// A constant expression (C precedence, 64-bit), resolvable to a number from
/// `const` declarations — or a bare symbol left for the assembler to resolve
/// (function/label/data addresses).
#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Name(String),
    Unary(char, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
}

/// An immediate value: expression plus the bare-`0` spelling flag.
#[derive(Debug, Clone)]
pub struct Imm {
    pub expr: Expr,
    /// True iff the literal was exactly the token `0` (selects `CLR`/`CZ`).
    pub bare_zero: bool,
}

/// The base of a place: a declared var, or an anonymous address.
#[derive(Debug, Clone)]
pub enum PlaceBase {
    Var(String),
    Addr(Expr),
}

/// A memory operand (GPL "GAS"): space + base, optionally indirect, optionally
/// indexed by a `>83xx` CPU cell, optionally width-cast.
#[derive(Debug, Clone)]
pub struct Place {
    pub space: Space,
    pub base: PlaceBase,
    pub indirect: bool,
    pub index: Option<PlaceBase>,
    pub cast: Option<Width>,
    /// True when the space was spelled out (`cpu[…]`/`vdp[…]`). A *bare* name
    /// that turns out not to be a declared var is re-interpreted by codegen as
    /// an immediate symbol (a const's value, or a label address).
    pub explicit: bool,
}

/// A statement operand: a place or an immediate.
#[derive(Debug, Clone)]
pub enum Operand {
    Place(Place),
    Imm(Imm),
}

/// A control-transfer target: a symbol or a constant address.
#[derive(Debug, Clone)]
pub enum Target {
    Name(String),
    Addr(Expr),
}

/// Compound-assignment operators (format-1 two-operand families).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set, // ST (or CLR for bare 0)
    Add,
    Sub,
    Mul,
    Div,
    And,
    Or,
    Xor,
    Sll, // <<=
    Sra, // >>=
    Srl, // >>>=
}

/// Single-operand intrinsic statements (format-5 families).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneOp {
    Inct,
    Dect,
    Abs,
    Neg,
    Inv,
    Push,
    Fetch,
    Case,
}

/// No-operand intrinsic statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroOp {
    Scan,
    Exit,
    Cont,
    Exec,
    Rtnb,
    Rtgr,
}

/// Immediate-operand intrinsic statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImmOp {
    Back,
    All,
    Rand,
    Parse,
    Xml,
}

/// Comparison operators; each maps to one compare opcode. `Eq` with a bare-`0`
/// immediate maps to `CZ` instead of `CEQ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,  // CEQ / CZ
    Gt,  // CGT (arithmetic)
    Ge,  // CGE
    HGt, // CH  (unsigned "high")
    HGe, // CHE
    Log, // CLOG — condition set when (a & b) == 0
}

/// Status-bit test opcodes (load the condition bit from a status flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusOp {
    Carry,
    Ovf,
    Gt,
    H,
}

/// A branch condition. `negated == false` means "branch when the condition
/// bit is SET" (`BS`); `true` means `BR`.
#[derive(Debug, Clone)]
pub enum Cond {
    Cmp { a: Place, op: CmpOp, b: Operand, negated: bool },
    Status { which: StatusOp, negated: bool },
    CondBit { negated: bool },
}

/// `move()` destination.
#[derive(Debug, Clone)]
pub enum MoveDst {
    Place(Place),
    VReg(Expr),
    Gram(Expr),
}

/// `move()` source.
#[derive(Debug, Clone)]
pub enum MoveSrc {
    Place(Place),
    Grom(Expr),
    /// `grom[*cell]` — the GROM address is read from a CPU cell at run time.
    GromVia(Place),
}

/// `move()` count.
#[derive(Debug, Clone)]
pub enum MoveCount {
    Imm(Expr),
    Place(Place),
}

/// One statement, with its source line and any labels prefixed to it.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub line: usize,
    pub labels: Vec<String>,
    pub kind: StmtKind,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `P op= Q;` (including `P = Q;`).
    Assign { dst: Place, op: AssignOp, src: Operand },
    /// `P++;` / `P--;`
    Inc(Place),
    Dec(Place),
    /// `inct(P);` etc.
    One { which: OneOp, arg: Place },
    /// `rotr(P, Q);` — SRC (rotate right).
    Rotr { dst: Place, count: Operand },
    /// `swap(P, Q);` — EX.
    Swap { a: Place, b: Place },
    /// `move(dst, src, count);`
    Move { dst: MoveDst, src: MoveSrc, count: MoveCount },
    /// `if (cond) goto target;`
    IfGoto { cond: Cond, target: Target },
    /// `test(cond);` — the compare/status op alone.
    Test(Cond),
    Goto(Target),
    /// `name();` or `call(expr);`
    Call(Target),
    Return,
    ReturnC,
    Zero(ZeroOp),
    ImmArg { which: ImmOp, arg: Expr },
    /// Inline assembler lines, spliced verbatim.
    Asm(Vec<String>),
    /// Structured sugar (compiler-only; the decompiler emits flat forms).
    If { cond: Cond, then_: Vec<Stmt>, else_: Vec<Stmt> },
    While { cond: Cond, body: Vec<Stmt> },
    /// A trailing `label:` before `}` (labels carried in [`Stmt::labels`]).
    Empty,
}

/// Output container formats (`GSL.md` §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Ctg,
    Grom,
    Grom24,
    RomBin,
}

/// One item inside a `data { }` / `rom N { }` block.
#[derive(Debug, Clone)]
pub enum DataItem {
    Byte(Expr),
    Word(Expr),
    Str(Vec<u8>),
}

/// A top-level item, in file order.
#[derive(Debug, Clone)]
pub enum Item {
    Format(OutFormat),
    Cartridge(String),
    Cru(Expr),
    Origin(Expr),
    GromPage(Expr),
    Const { name: String, value: Expr, line: usize },
    Var { name: String, width: Width, space: Space, addr: Expr, line: usize },
    Func { name: String, pin: Option<Expr>, body: Vec<Stmt>, line: usize },
    Data { name: Option<String>, pin: Option<Expr>, items: Vec<DataItem>, line: usize },
    AsmBlock { lines: Vec<String>, line: usize },
    Rom { bank: Expr, items: Vec<DataItem>, line: usize },
}

/// A parsed program: the item list.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub items: Vec<Item>,
}
