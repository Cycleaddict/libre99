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

//! The GSL decompiler: a `.ctg`/`.bin` image → GSL source, **verified**.
//!
//! The pipeline (docs/GSL.md §10):
//!
//! 1. normalize the container into a [`Payload`] (GROM pages / ROM banks);
//! 2. discover entry points from the standard `>AA` GROM headers (power-up,
//!    program, DSR, subprogram, interrupt chains — with names) plus the
//!    console boot entry `>0020` on base-0 images;
//! 3. trace code from the entries (recursive traversal over `B`/`BR`/`BS`/
//!    `CALL` and fall-through, with a grammar-exact FMT scanner);
//! 4. emit each traced instruction as its unique GSL spelling — but only if
//!    re-encoding reproduces the original bytes; everything else (non-
//!    canonical encodings, opcodes with no GSL spelling, FMT blocks) becomes
//!    raw `BYTE` lines inside `asm { }`, and untraced bytes become `data`;
//! 5. compile the generated text and byte-compare against the input payload,
//!    demoting any still-mismatching statement to raw bytes and retrying —
//!    the returned text is **guaranteed byte-identical** or an error.
//!
//! Because the guarantee is enforced by construction, analysis quality only
//! affects readability, never correctness.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use libre99_gpl::decode::{decode_at, Decoded, Flow};
use libre99_gpl::isa::{decode_sig, MoveBits, Sig};
use libre99_gpl::operand::Operand as GOp;

use crate::ast::OutFormat;
use crate::codegen;
use crate::container::{self, InputKind, Payload};
use crate::fmtscan::{self, FmtBlock};
use crate::wellknown;

const PAGE: usize = 0x2000;

/// Decompiler options.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Shown in the header banner.
    pub input_name: String,
    /// Force the GROM base of a headerless raw dump.
    pub base_override: Option<u16>,
    /// Treat a raw `.bin` as a CPU-ROM dump.
    pub force_rom: bool,
}

/// Decompilation statistics (also embedded in the output banner).
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub fns: usize,
    pub stmt_instrs: usize,
    pub stmt_bytes: usize,
    pub fallback_instrs: usize,
    pub fallback_bytes: usize,
    pub demoted_instrs: usize,
    pub data_bytes: usize,
    pub elided_zero_bytes: usize,
    pub rounds: usize,
}

/// A verified decompilation.
#[derive(Debug, Clone)]
pub struct Decompiled {
    pub text: String,
    pub format: OutFormat,
    pub payload: Payload,
    pub stats: Stats,
}

/// Decompile an input image to GSL text, verifying byte-identity before
/// returning.
pub fn decompile(bytes: &[u8], opts: &Options) -> Result<Decompiled, String> {
    let (payload, kind) = container::parse_input(bytes, opts.base_override, opts.force_rom)?;
    let format = kind.format();

    // ---- flat GROM space ---------------------------------------------------
    let mut img = vec![0u8; 0x10000];
    let mut pages: BTreeSet<u16> = BTreeSet::new();
    for (base, page) in &payload.grom {
        img[*base as usize..*base as usize + PAGE].copy_from_slice(page);
        pages.insert(*base);
    }
    let present = |addr: u16| pages.contains(&(addr & 0xE000));
    let word = |a: u16| ((img[a as usize] as u16) << 8) | img[a.wrapping_add(1) as usize] as u16;

    // ---- entry discovery ---------------------------------------------------
    struct Entry {
        addr: u16,
        name: String,
        desc: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut used_names: BTreeSet<String> = BTreeSet::new();
    let unique = |base: String, used: &mut BTreeSet<String>| -> String {
        let mut name = base.clone();
        let mut n = 1;
        while !used.insert(name.clone()) {
            n += 1;
            name = format!("{base}_{n}");
        }
        name
    };
    for &page in &pages {
        if img[page as usize] != 0xAA {
            continue;
        }
        let chains: [(u16, &str, bool, &str); 5] = [
            (4, "power-up", false, "pow"),
            (6, "program", true, "prog"),
            (8, "DSR", true, "dsr"),
            (0xA, "subprogram", true, "spr"),
            (0xC, "interrupt", false, "isr"),
        ];
        for (off, kindname, has_name, prefix) in chains {
            let mut node = word(page + off);
            let mut seen = BTreeSet::new();
            while node != 0 && present(node) && seen.insert(node) && seen.len() <= 64 {
                let next = word(node);
                let addr = word(node + 2);
                if addr != 0 && present(addr) {
                    let raw_name = if has_name {
                        let len = img[node as usize + 4] as usize;
                        img[node as usize + 5..]
                            .iter()
                            .take(len.min(32))
                            .map(|&b| b as char)
                            .filter(|c| c.is_ascii_graphic() || *c == ' ')
                            .collect::<String>()
                            .trim()
                            .to_string()
                    } else {
                        String::new()
                    };
                    let ident = if raw_name.is_empty() {
                        format!("{prefix}_{addr:04X}")
                    } else {
                        let mut s = String::new();
                        for c in raw_name.chars() {
                            if c.is_ascii_alphanumeric() {
                                s.push(c.to_ascii_lowercase());
                            } else if !s.ends_with('_') {
                                s.push('_');
                            }
                        }
                        format!("{prefix}_{}", s.trim_matches('_'))
                    };
                    let ident = unique(ident, &mut used_names);
                    let what = if raw_name.is_empty() {
                        format!("{kindname} entry (GROM header >{page:04X}, list node >{node:04X})")
                    } else {
                        format!(
                            "{kindname} entry \"{raw_name}\" (GROM header >{page:04X}, list node >{node:04X})"
                        )
                    };
                    entries.push(Entry { addr, name: ident, desc: what });
                }
                node = next;
            }
        }
    }
    if pages.contains(&0x0000) {
        let name = unique("console_boot".into(), &mut used_names);
        entries.push(Entry {
            addr: 0x0020,
            name,
            desc: "console power-up entry (the ROM starts GPL execution at >0020)".into(),
        });
    }

    // ---- trace -------------------------------------------------------------
    enum TKind {
        Stmt(Decoded),
        Fallback(Decoded, String),
        Fmt(FmtBlock),
    }
    struct Tile {
        len: u16,
        kind: TKind,
    }
    let mut tiles: BTreeMap<u16, Tile> = BTreeMap::new();
    let mut covered = vec![false; 0x10000];
    let mut callers: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    let mut gromrefs: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    let mut work: VecDeque<u16> = entries.iter().map(|e| e.addr).collect();

    while let Some(addr) = work.pop_front() {
        if tiles.contains_key(&addr) || covered[addr as usize] || !present(addr) {
            continue;
        }
        let (len, kind, flow) = if img[addr as usize] == 0x08 {
            match fmtscan::scan(&img, addr) {
                Some(b) if b.len < 0x2000 => (b.len as u16, TKind::Fmt(b), Flow::Fall),
                _ => continue,
            }
        } else {
            match decode_at(&img, addr as usize, addr) {
                Ok(d) => {
                    let len = d.len as u16;
                    let flow = d.flow;
                    let end = addr as usize + d.len;
                    let matches_bytes =
                        reencode(&d).is_some_and(|b| b == img[addr as usize..end]);
                    let kind = if !matches_bytes {
                        TKind::Fallback(d, "non-canonical encoding".into())
                    } else {
                        match spellable(&d) {
                            Ok(()) => TKind::Stmt(d),
                            Err(reason) => TKind::Fallback(d, reason),
                        }
                    };
                    (len, kind, flow)
                }
                Err(_) => continue,
            }
        };
        let end = addr as usize + len as usize;
        if end > 0x10000
            || !(addr..end as u16).all(&present)
            || covered[addr as usize..end].iter().any(|&c| c)
        {
            continue;
        }
        covered[addr as usize..end].fill(true);
        // Cross-references.
        if let TKind::Stmt(d) | TKind::Fallback(d, _) = &kind {
            if d.mnemonic == "MOVE" {
                if let Some(GOp::Grom(a)) = d.operands.get(2) {
                    gromrefs.entry(*a).or_default().push(addr);
                }
            }
            match d.flow {
                Flow::Call(t) => callers.entry(t).or_default().push(addr),
                Flow::Jump(_) | Flow::Cond(_) | Flow::Fall | Flow::Stop => {}
            }
        }
        let fall = (end < 0x10000).then_some(end as u16);
        match flow {
            Flow::Fall => work.extend(fall),
            Flow::Jump(t) => work.push_back(t),
            Flow::Call(t) | Flow::Cond(t) => {
                work.push_back(t);
                work.extend(fall);
            }
            Flow::Stop => {}
        }
        tiles.insert(addr, Tile { len, kind });
    }

    // ---- boundaries --------------------------------------------------------
    let ends: BTreeSet<u16> =
        tiles.iter().filter_map(|(a, t)| u16::try_from(*a as u32 + t.len as u32).ok()).collect();
    let mut fn_starts: BTreeSet<u16> = BTreeSet::new();
    for &a in tiles.keys() {
        if !ends.contains(&a) {
            fn_starts.insert(a); // run head
        }
    }
    for e in &entries {
        if tiles.contains_key(&e.addr) {
            fn_starts.insert(e.addr);
        }
    }
    for &t in callers.keys() {
        if tiles.contains_key(&t) {
            fn_starts.insert(t);
        }
    }
    let mut fn_names: BTreeMap<u16, String> = BTreeMap::new();
    for e in &entries {
        fn_names.entry(e.addr).or_insert_with(|| e.name.clone());
    }
    for &a in &fn_starts {
        fn_names.entry(a).or_insert_with(|| format!("sub_{a:04X}"));
    }
    let mut labels: BTreeMap<u16, String> = BTreeMap::new();
    for t in tiles.values() {
        if let TKind::Stmt(d) | TKind::Fallback(d, _) = &t.kind {
            if let Flow::Jump(t) | Flow::Cond(t) = d.flow {
                if tiles.contains_key(&t) && !fn_starts.contains(&t) {
                    labels.entry(t).or_insert_with(|| format!("L_{t:04X}"));
                }
            }
        }
    }

    // ---- data chunks (untraced bytes, zero runs elided) --------------------
    struct Chunk {
        addr: u16,
        len: usize,
    }
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut elided_zero_bytes = 0usize;
    for &page in &pages {
        let mut a = page as usize;
        let page_end = page as usize + PAGE;
        while a < page_end {
            if covered[a] {
                a += 1;
                continue;
            }
            let mut b = a;
            while b < page_end && !covered[b] {
                b += 1;
            }
            // Split [a, b) on zero runs of >= 16 bytes.
            let mut c = a;
            while c < b {
                if img[c] == 0 {
                    let mut z = c;
                    while z < b && img[z] == 0 {
                        z += 1;
                    }
                    if z - c >= 16 || (c == a && z == b) {
                        elided_zero_bytes += z - c;
                        c = z;
                        continue;
                    }
                }
                let mut d = c;
                let mut zrun = 0usize;
                while d < b {
                    if img[d] == 0 {
                        zrun += 1;
                        if zrun >= 16 {
                            d -= zrun - 1;
                            break;
                        }
                    } else {
                        zrun = 0;
                    }
                    d += 1;
                }
                chunks.push(Chunk { addr: c as u16, len: d - c });
                c = d;
            }
            a = b;
        }
    }
    let data_names: BTreeMap<u16, String> =
        chunks.iter().map(|c| (c.addr, format!("d_{:04X}", c.addr))).collect();

    // ---- emit tiles as GSL text -------------------------------------------
    let mut em = Emitter {
        vars: BTreeMap::new(),
        labels: &labels,
        fn_names: &fn_names,
        data_names: &data_names,
    };

    enum Piece {
        Fn { addr: u16, comments: Vec<String> },
        Stmt { addr: u16, len: u16, text: String, demoted: bool },
        Bytes { addr: u16, len: u16, notes: Vec<String> },
        Data { addr: u16, len: usize, comments: Vec<String> },
    }
    let mut pieces: Vec<Piece> = Vec::new();

    {
        // Merge tiles and chunks in address order; fuse compare+branch.
        let tile_list: Vec<(u16, &Tile)> = tiles.iter().map(|(a, t)| (*a, t)).collect();
        let mut ci = 0usize;
        let mut ti = 0usize;
        while ti < tile_list.len() || ci < chunks.len() {
            let next_tile = tile_list.get(ti).map(|(a, _)| *a);
            let next_chunk = chunks.get(ci).map(|c| c.addr);
            let do_tile = match (next_tile, next_chunk) {
                (Some(t), Some(c)) => t < c,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if !do_tile {
                let c = &chunks[ci];
                ci += 1;
                let mut comments = Vec::new();
                comments.push(format!(
                    "// >{:04X}..>{:04X}: {} data bytes",
                    c.addr,
                    c.addr as usize + c.len - 1,
                    c.len
                ));
                if c.addr & 0x1FFF == 0 && img[c.addr as usize] == 0xAA {
                    comments.push("// standard GROM header (>AA):".into());
                    for e in &entries {
                        if e.desc.contains(&format!(">{:04X}", c.addr)) {
                            comments.push(format!("//   -> >{:04X} {} ({})", e.addr, e.name, e.desc));
                        }
                    }
                }
                let last = c.addr + (c.len - 1) as u16;
                let refs: Vec<String> = gromrefs
                    .range(c.addr..=last)
                    .flat_map(|(_, v)| v.iter().map(|f| format!(">{f:04X}")))
                    .take(6)
                    .collect();
                if !refs.is_empty() {
                    comments.push(format!("// referenced by move() at {}", refs.join(", ")));
                }
                pieces.push(Piece::Data { addr: c.addr, len: c.len, comments });
                continue;
            }
            let (addr, tile) = tile_list[ti];
            ti += 1;
            if fn_starts.contains(&addr) {
                let mut comments = Vec::new();
                let mut what: Vec<&str> = Vec::new();
                for e in &entries {
                    if e.addr == addr {
                        what.push(&e.desc);
                    }
                }
                if what.is_empty() {
                    comments.push(format!("// >{addr:04X}: subroutine"));
                } else {
                    for w in what {
                        comments.push(format!("// >{addr:04X}: {w}"));
                    }
                }
                if let Some(cs) = callers.get(&addr) {
                    let list: Vec<String> =
                        cs.iter().take(6).map(|c| format!(">{c:04X}")).collect();
                    let more = if cs.len() > 6 { ", ..." } else { "" };
                    comments.push(format!("// called from {}{more}", list.join(", ")));
                }
                pieces.push(Piece::Fn { addr, comments });
            }
            match &tile.kind {
                TKind::Fmt(b) => {
                    let mut notes =
                        vec![format!("* >{addr:04X}  FMT block, {} sub-ops:", b.ops.len())];
                    for (a, d) in b.ops.iter().take(40) {
                        notes.push(format!("* >{a:04X}    {d}"));
                    }
                    if b.ops.len() > 40 {
                        notes.push("*          ...".into());
                    }
                    pieces.push(Piece::Bytes { addr, len: tile.len, notes });
                }
                TKind::Fallback(d, reason) => {
                    let notes = vec![format!(
                        "* >{addr:04X}  {} {} ; {reason}",
                        d.mnemonic,
                        describe_operands(d)
                    )];
                    pieces.push(Piece::Bytes { addr, len: tile.len, notes });
                }
                TKind::Stmt(d) => {
                    match em.render_tile(d) {
                        Rendered::Stmt(text) => {
                            pieces.push(Piece::Stmt { addr, len: tile.len, text, demoted: false })
                        }
                        Rendered::Cond { pos, neg } => {
                            // Try to fuse with an immediately following BR/BS.
                            let fused = tile_list.get(ti).and_then(|(baddr, btile)| {
                                if *baddr as u32 != addr as u32 + tile.len as u32
                                    || labels.contains_key(baddr)
                                    || fn_starts.contains(baddr)
                                {
                                    return None;
                                }
                                match &btile.kind {
                                    TKind::Stmt(bd) if (0x40..=0x7F).contains(&bd.opcode) => {
                                        let t = em.target_text(branch_target(bd)?);
                                        let c = if bd.opcode >= 0x60 { &pos } else { &neg };
                                        Some((btile.len, format!("if ({c}) goto {t};")))
                                    }
                                    _ => None,
                                }
                            });
                            match fused {
                                Some((blen, text)) => {
                                    ti += 1; // consume the branch tile
                                    pieces.push(Piece::Stmt {
                                        addr,
                                        len: tile.len + blen,
                                        text,
                                        demoted: false,
                                    });
                                }
                                None => pieces.push(Piece::Stmt {
                                    addr,
                                    len: tile.len,
                                    text: format!("test({pos});"),
                                    demoted: false,
                                }),
                            }
                        }
                        Rendered::No(reason) => {
                            let notes = vec![format!(
                                "* >{addr:04X}  {} {} ; {reason}",
                                d.mnemonic,
                                describe_operands(d)
                            )];
                            pieces.push(Piece::Bytes { addr, len: tile.len, notes });
                        }
                    }
                }
            }
        }
    }

    // ---- render + verify loop ---------------------------------------------
    let render = |pieces: &[Piece], em: &Emitter, stats: &Stats| -> String {
        let mut out = String::new();
        let push = |out: &mut String, s: &str| {
            out.push_str(s);
            out.push('\n');
        };
        // Banner.
        push(&mut out, "// ============================================================");
        push(
            &mut out,
            &format!(
                "// decompiled by libre99-gsl {} from {}",
                env!("CARGO_PKG_VERSION"),
                if opts.input_name.is_empty() { "<memory>" } else { &opts.input_name }
            ),
        );
        push(
            &mut out,
            &format!(
                "// input: {} ({} bytes); grom pages: {}; rom banks: {}",
                match kind {
                    InputKind::Ctg => "ti99sim .ctg".to_string(),
                    InputKind::GromBin { base } => format!("raw GROM dump @ >{base:04X}"),
                    InputKind::RomBin => "raw CPU-ROM dump".to_string(),
                },
                bytes.len(),
                if pages.is_empty() {
                    "none".to_string()
                } else {
                    pages.iter().map(|p| format!(">{p:04X}")).collect::<Vec<_>>().join(" ")
                },
                payload.rom.len() / PAGE,
            ),
        );
        if kind == InputKind::Ctg {
            push(&mut out, &format!("// title: {:?}, cru base: >{:04X}", payload.title, payload.cru));
        }
        push(
            &mut out,
            &format!(
                "// coverage: {} statements ({} bytes), {} raw-byte instrs ({} bytes, {} demoted), {} data bytes, {} zero bytes elided",
                stats.stmt_instrs,
                stats.stmt_bytes,
                stats.fallback_instrs + stats.demoted_instrs,
                stats.fallback_bytes,
                stats.demoted_instrs,
                stats.data_bytes,
                stats.elided_zero_bytes,
            ),
        );
        push(
            &mut out,
            "// round-trip: this file recompiles byte-identically to the input payload",
        );
        push(&mut out, "// (verified by the decompiler before writing). Regenerate with:");
        push(&mut out, "//   libre99gsl compile <this file> -o <out>");
        push(&mut out, "// ============================================================");
        push(&mut out, "");
        // Declarations.
        push(
            &mut out,
            &format!(
                "format {};",
                match format {
                    OutFormat::Ctg => "ctg",
                    OutFormat::Grom => "grom",
                    OutFormat::Grom24 => "grom24",
                    OutFormat::RomBin => "rombin",
                }
            ),
        );
        if kind == InputKind::Ctg {
            push(&mut out, &format!("cartridge {:?};", payload.title));
            push(&mut out, &format!("cru 0x{:04X};", payload.cru));
        }
        if matches!(format, OutFormat::Ctg | OutFormat::Grom) {
            for p in &pages {
                push(&mut out, &format!("grompage 0x{p:04X};"));
            }
        }
        // Vars.
        if !em.vars.is_empty() {
            push(&mut out, "");
            push(&mut out, "// ---- variables (every cell the statements touch) ----");
            for ((vdp, wordw, addr), name) in &em.vars {
                let space = if *vdp { "vdp" } else { "cpu" };
                let width = if *wordw { "word" } else { "byte" };
                let note = if *vdp { None } else { wellknown::describe(*addr) };
                let decl = format!("var {name}: {width} @ {space}[0x{addr:04X}];");
                match note {
                    Some(n) => push(&mut out, &format!("{decl:44} // {n}")),
                    None => push(&mut out, &decl),
                }
            }
        }
        // Body.
        let mut fn_open = false;
        let mut asm_open = false;
        let close_asm = |out: &mut String, asm_open: &mut bool| {
            if *asm_open {
                out.push_str("    }\n");
                *asm_open = false;
            }
        };
        let close_fn = |out: &mut String, fn_open: &mut bool, asm_open: &mut bool| {
            close_asm(out, asm_open);
            if *fn_open {
                out.push_str("}\n");
                *fn_open = false;
            }
        };
        for piece in pieces {
            match piece {
                Piece::Fn { addr, comments } => {
                    close_fn(&mut out, &mut fn_open, &mut asm_open);
                    push(&mut out, "");
                    for c in comments {
                        push(&mut out, c);
                    }
                    push(&mut out, &format!("fn {}() @ 0x{addr:04X} {{", em.fn_names[addr]));
                    fn_open = true;
                }
                Piece::Data { addr, len, comments } => {
                    close_fn(&mut out, &mut fn_open, &mut asm_open);
                    push(&mut out, "");
                    for c in comments {
                        push(&mut out, c);
                    }
                    push(&mut out, &format!("data d_{addr:04X} @ 0x{addr:04X} {{"));
                    let s = *addr as usize;
                    for row in (s..s + len).collect::<Vec<_>>().chunks(8) {
                        let items: Vec<String> =
                            row.iter().map(|&i| format!("0x{:02X}", img[i])).collect();
                        let ascii: String = row
                            .iter()
                            .map(|&i| {
                                let c = img[i];
                                if (0x20..0x7F).contains(&c) { c as char } else { '.' }
                            })
                            .collect();
                        push(
                            &mut out,
                            &format!(
                                "    {:47} // >{:04X} |{ascii}|",
                                items.join(", ") + ",",
                                row[0]
                            ),
                        );
                    }
                    push(&mut out, "}");
                }
                Piece::Stmt { addr, len, text, demoted } => {
                    if let Some(l) = em.labels.get(addr) {
                        close_asm(&mut out, &mut asm_open);
                        push(&mut out, &format!("{l}:"));
                    }
                    if *demoted {
                        if !asm_open {
                            push(&mut out, "    asm {");
                            asm_open = true;
                        }
                        push(
                            &mut out,
                            &format!("* >{addr:04X}  {text} ; demoted: recompiled bytes differed"),
                        );
                        emit_byte_rows(&mut out, &img, *addr, *len);
                    } else {
                        close_asm(&mut out, &mut asm_open);
                        push(&mut out, &format!("    {text}"));
                    }
                }
                Piece::Bytes { addr, len, notes } => {
                    if let Some(l) = em.labels.get(addr) {
                        close_asm(&mut out, &mut asm_open);
                        push(&mut out, &format!("{l}:"));
                    }
                    if !asm_open {
                        push(&mut out, "    asm {");
                        asm_open = true;
                    }
                    for n in notes {
                        push(&mut out, n);
                    }
                    emit_byte_rows(&mut out, &img, *addr, *len);
                }
            }
        }
        close_fn(&mut out, &mut fn_open, &mut asm_open);
        // ROM banks.
        for (i, bank) in payload.rom.chunks(PAGE).enumerate() {
            push(&mut out, "");
            push(&mut out, &format!("// ---- TMS9900 ROM bank {i} (not GPL; carried as data) ----"));
            push(&mut out, &format!("rom {i} {{"));
            // Trim trailing zeros (the compiler zero-pads banks back to 8 KiB).
            let used = bank.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            for row in (0..used).collect::<Vec<_>>().chunks(8) {
                let items: Vec<String> =
                    row.iter().map(|&i| format!("0x{:02X}", bank[i])).collect();
                let ascii: String = row
                    .iter()
                    .map(|&i| {
                        let c = bank[i];
                        if (0x20..0x7F).contains(&c) { c as char } else { '.' }
                    })
                    .collect();
                push(
                    &mut out,
                    &format!("    {:47} // +0x{:04X} |{ascii}|", items.join(", ") + ",", row[0]),
                );
            }
            push(&mut out, "}");
        }
        out
    };

    let compute_stats = |pieces: &[Piece], rounds: usize| -> Stats {
        let mut s = Stats { rounds, elided_zero_bytes, ..Default::default() };
        for p in pieces {
            match p {
                Piece::Fn { .. } => s.fns += 1,
                Piece::Stmt { len, demoted, .. } => {
                    if *demoted {
                        s.demoted_instrs += 1;
                        s.fallback_bytes += *len as usize;
                    } else {
                        s.stmt_instrs += 1;
                        s.stmt_bytes += *len as usize;
                    }
                }
                Piece::Bytes { len, .. } => {
                    s.fallback_instrs += 1;
                    s.fallback_bytes += *len as usize;
                }
                Piece::Data { len, .. } => s.data_bytes += len,
            }
        }
        s
    };

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if rounds > 8 {
            return Err("round-trip verification did not converge after 8 demotion rounds".into());
        }
        let stats = compute_stats(&pieces, rounds);
        let text = render(&pieces, &em, &stats);
        let compiled = match codegen::compile(&text) {
            Ok(c) => c,
            Err(errs) => {
                let head: Vec<String> = errs.iter().take(5).map(|e| e.to_string()).collect();
                return Err(format!(
                    "decompiler produced GSL that does not compile (internal bug):\n{}",
                    head.join("\n")
                ));
            }
        };
        let got = container::payload_of(&compiled, format)
            .map_err(|e| format!("internal: {e}"))?;
        let diffs = container::diff(&payload, &got, kind == InputKind::Ctg);
        if diffs.is_empty() {
            let stats = compute_stats(&pieces, rounds);
            let text = render(&pieces, &em, &stats);
            return Ok(Decompiled { text, format, payload, stats });
        }
        // Demote the statements covering the mismatched bytes and retry.
        let bad = container::grom_mismatches(&payload, &got);
        let mut demoted_any = false;
        for &b in &bad {
            for p in pieces.iter_mut() {
                if let Piece::Stmt { addr, len, demoted, .. } = p {
                    let (a, e) = (*addr as u32, *addr as u32 + *len as u32);
                    if !*demoted && (a..e).contains(&(b as u32)) {
                        *demoted = true;
                        demoted_any = true;
                    }
                }
            }
        }
        if !demoted_any {
            return Err(format!(
                "round-trip verification failed outside any statement (internal bug):\n{}",
                diffs.join("\n")
            ));
        }
    }
}

fn emit_byte_rows(out: &mut String, img: &[u8], addr: u16, len: u16) {
    let s = addr as usize;
    for row in (s..s + len as usize).collect::<Vec<_>>().chunks(8) {
        let items: Vec<String> = row.iter().map(|&i| format!(">{:02X}", img[i])).collect();
        out.push_str(&format!("        BYTE {}\n", items.join(",")));
    }
}

fn branch_target(d: &Decoded) -> Option<u16> {
    match d.flow {
        Flow::Cond(t) | Flow::Jump(t) => Some(t),
        _ => None,
    }
}

fn describe_operands(d: &Decoded) -> String {
    let ops: Vec<String> = d.operands.iter().map(libre99_gpl::disasm::format_operand).collect();
    ops.join(",")
}

/// Re-encode a decoded instruction; `None` when the shape has no encoder.
fn reencode(d: &Decoded) -> Option<Vec<u8>> {
    let (_, sig) = decode_sig(d.opcode);
    match sig {
        Sig::Branch => {
            let t = branch_target(d)?;
            Some(vec![(d.opcode & 0xE0) | ((t >> 8) & 0x1F) as u8, t as u8])
        }
        Sig::Move => {
            let bits = MoveBits::from_opcode(d.opcode);
            let mut out = vec![d.opcode];
            let gas = |op: &GOp, out: &mut Vec<u8>| -> Option<()> {
                libre99_gpl::operand::encode_gas(op, out).ok()
            };
            match (bits.imm_count, d.operands.first()?) {
                (true, GOp::Imm16(v)) => {
                    out.push((v >> 8) as u8);
                    out.push(*v as u8);
                }
                (false, op) => gas(op, &mut out)?,
                _ => return None,
            }
            match (bits.reg_dst, bits.not_grom_dst, d.operands.get(1)?) {
                (true, _, GOp::Imm8(r)) => out.push(*r),
                (false, false, GOp::Grom(a)) => {
                    out.push((a >> 8) as u8);
                    out.push(*a as u8);
                }
                (false, true, op) => gas(op, &mut out)?,
                _ => return None,
            }
            match (bits.ram_src || bits.cpu_held_grom_src, d.operands.get(2)?) {
                (true, op) => gas(op, &mut out)?,
                (false, GOp::Grom(a)) => {
                    out.push((a >> 8) as u8);
                    out.push(*a as u8);
                }
                _ => return None,
            }
            Some(out)
        }
        Sig::Fmt | Sig::Unknown => None,
        _ => libre99_gpl::encode::encode(d.opcode, sig, &d.operands).ok(),
    }
}

/// Can this decoded instruction be spelled as a GSL statement at all?
fn spellable(d: &Decoded) -> Result<(), String> {
    const SPELLED: &[&str] = &[
        "RTN", "RTNC", "RAND", "SCAN", "BACK", "B", "CALL", "ALL", "H", "GT", "EXIT", "CARRY",
        "OVF", "PARSE", "XML", "CONT", "EXEC", "RTNB", "RTGR", "BR", "BS", "MOVE", "ADD", "SUB",
        "MUL", "DIV", "AND", "OR", "XOR", "ST", "EX", "CH", "CHE", "CGT", "CGE", "CEQ", "CLOG",
        "SRA", "SLL", "SRL", "SRC", "ABS", "NEG", "INV", "CLR", "FETCH", "CASE", "PUSH", "CZ",
        "INC", "DEC", "INCT", "DECT",
    ];
    if !SPELLED.contains(&d.mnemonic) {
        return Err(format!("{} has no GSL spelling", d.mnemonic));
    }
    for op in &d.operands {
        if let GOp::Vdp { addr, indirect: false, .. } = op {
            if *addr > 0x3FFF {
                return Err("VDP address beyond >3FFF".into());
            }
        }
    }
    if d.mnemonic == "MOVE" {
        let bits = MoveBits::from_opcode(d.opcode);
        if bits.cpu_held_grom_src {
            match d.operands.get(2) {
                Some(GOp::Cpu { indirect: false, index: None, .. }) => {}
                _ => return Err("computed-GROM move through a non-simple cell".into()),
            }
        }
    }
    Ok(())
}

enum Rendered {
    Stmt(String),
    /// A compare/status op — the caller fuses it with a following branch or
    /// renders `test(pos);`.
    Cond {
        pos: String,
        neg: String,
    },
    No(&'static str),
}

/// Renders decoded instructions as GSL statements, registering every variable
/// it names. Key: `(is_vdp, is_word, addr) → name`.
struct Emitter<'a> {
    vars: BTreeMap<(bool, bool, u16), String>,
    labels: &'a BTreeMap<u16, String>,
    fn_names: &'a BTreeMap<u16, String>,
    data_names: &'a BTreeMap<u16, String>,
}

impl Emitter<'_> {
    fn var(&mut self, vdp: bool, word: bool, addr: u16) -> String {
        self.vars
            .entry((vdp, word, addr))
            .or_insert_with(|| {
                let prefix = match (vdp, word) {
                    (false, false) => "b",
                    (false, true) => "w",
                    (true, false) => "vb",
                    (true, true) => "vw",
                };
                format!("{prefix}_{addr:04X}")
            })
            .clone()
    }

    /// A place operand as GSL text (registering vars), for an op of width
    /// `word`. Indirect places get a `word(…)` cast when the op is a word op.
    fn place(&mut self, op: &GOp, word: bool) -> Option<String> {
        let ix_text = |em: &mut Self, index: &Option<u8>| -> Option<String> {
            index.map(|ix| {
                let cell = em.var(false, false, 0x8300 + ix as u16);
                format!("({cell})")
            })
        };
        match op {
            GOp::Cpu { addr, indirect: false, index } => {
                let base = self.var(false, word, *addr);
                match ix_text(self, index) {
                    Some(ix) => Some(format!("{base}{ix}")),
                    None => Some(base),
                }
            }
            GOp::Cpu { addr, indirect: true, index } => {
                let cell = self.var(false, true, *addr);
                let ix = ix_text(self, index).unwrap_or_default();
                let core = format!("*{cell}{ix}");
                Some(if word { format!("word({core})") } else { core })
            }
            GOp::Vdp { addr, indirect: false, index } => {
                if *addr > 0x3FFF {
                    return None;
                }
                let base = self.var(true, word, *addr);
                match ix_text(self, index) {
                    Some(ix) => Some(format!("{base}{ix}")),
                    None => Some(base),
                }
            }
            GOp::Vdp { addr, indirect: true, index } => {
                let cell = self.var(false, true, *addr);
                let ix = ix_text(self, index).unwrap_or_default();
                let core = format!("vdp[*{cell}]{ix}");
                Some(if word { format!("word({core})") } else { core })
            }
            _ => None,
        }
    }

    fn imm(op: &GOp) -> Option<String> {
        match op {
            GOp::Imm8(v) => Some(format!("0x{v:02X}")),
            GOp::Imm16(v) => Some(format!("0x{v:04X}")),
            _ => None,
        }
    }

    fn target_text(&self, t: u16) -> String {
        if let Some(l) = self.labels.get(&t) {
            return l.clone();
        }
        if let Some(f) = self.fn_names.get(&t) {
            return f.clone();
        }
        format!("0x{t:04X}")
    }

    fn render_tile(&mut self, d: &Decoded) -> Rendered {
        let w = d.opcode & 1 != 0; // W bit for the family ops
        macro_rules! place {
            ($i:expr) => {
                match self.place(&d.operands[$i], w) {
                    Some(p) => p,
                    None => return Rendered::No("operand not expressible"),
                }
            };
        }
        macro_rules! src {
            () => {
                match &d.operands[1] {
                    op @ (GOp::Imm8(_) | GOp::Imm16(_)) => Self::imm(op).unwrap(),
                    op => match self.place(op, w) {
                        Some(p) => p,
                        None => return Rendered::No("operand not expressible"),
                    },
                }
            };
        }
        match d.mnemonic {
            // ---- no-operand / immediate named ops ----
            "RTN" => Rendered::Stmt("return;".into()),
            "RTNC" => Rendered::Stmt("returnc;".into()),
            "SCAN" => Rendered::Stmt("scan();".into()),
            "EXIT" => Rendered::Stmt("exit();".into()),
            "CONT" => Rendered::Stmt("cont();".into()),
            "EXEC" => Rendered::Stmt("exec();".into()),
            "RTNB" => Rendered::Stmt("rtnb();".into()),
            "RTGR" => Rendered::Stmt("rtgr();".into()),
            "BACK" | "ALL" | "RAND" | "PARSE" | "XML" => {
                let arg = Self::imm(&d.operands[0]).unwrap();
                Rendered::Stmt(format!("{}({arg});", d.mnemonic.to_lowercase()))
            }
            "CARRY" => Rendered::Cond { pos: "carry()".into(), neg: "!carry()".into() },
            "OVF" => Rendered::Cond { pos: "ovf()".into(), neg: "!ovf()".into() },
            "GT" => Rendered::Cond { pos: "gt()".into(), neg: "!gt()".into() },
            "H" => Rendered::Cond { pos: "h()".into(), neg: "!h()".into() },
            // ---- control ----
            "B" => {
                let t = self.target_text(match d.flow {
                    Flow::Jump(t) => t,
                    _ => return Rendered::No("odd B"),
                });
                Rendered::Stmt(format!("goto {t};"))
            }
            "CALL" => {
                let t = match d.flow {
                    Flow::Call(t) => t,
                    _ => return Rendered::No("odd CALL"),
                };
                if let Some(f) = self.fn_names.get(&t) {
                    Rendered::Stmt(format!("{f}();"))
                } else {
                    Rendered::Stmt(format!("call(0x{t:04X});"))
                }
            }
            "BR" | "BS" => {
                let t = self.target_text(match d.flow {
                    Flow::Cond(t) => t,
                    _ => return Rendered::No("odd branch"),
                });
                let c = if d.opcode >= 0x60 { "cond()" } else { "!cond()" };
                Rendered::Stmt(format!("if ({c}) goto {t};"))
            }
            // ---- single-operand ----
            "INC" => Rendered::Stmt(format!("{}++;", place!(0))),
            "DEC" => Rendered::Stmt(format!("{}--;", place!(0))),
            "CLR" => Rendered::Stmt(format!("{} = 0;", place!(0))),
            "INCT" | "DECT" | "ABS" | "NEG" | "INV" | "PUSH" | "FETCH" | "CASE" => {
                Rendered::Stmt(format!("{}({});", d.mnemonic.to_lowercase(), place!(0)))
            }
            "CZ" => {
                let p = place!(0);
                Rendered::Cond { pos: format!("{p} == 0"), neg: format!("{p} != 0") }
            }
            // ---- two-operand ----
            "ST" => Rendered::Stmt(format!("{} = {};", place!(0), src!())),
            "ADD" => Rendered::Stmt(format!("{} += {};", place!(0), src!())),
            "SUB" => Rendered::Stmt(format!("{} -= {};", place!(0), src!())),
            "MUL" => Rendered::Stmt(format!("{} *= {};", place!(0), src!())),
            "DIV" => Rendered::Stmt(format!("{} /= {};", place!(0), src!())),
            "AND" => Rendered::Stmt(format!("{} &= {};", place!(0), src!())),
            "OR" => Rendered::Stmt(format!("{} |= {};", place!(0), src!())),
            "XOR" => Rendered::Stmt(format!("{} ^= {};", place!(0), src!())),
            "SLL" => Rendered::Stmt(format!("{} <<= {};", place!(0), src!())),
            "SRA" => Rendered::Stmt(format!("{} >>= {};", place!(0), src!())),
            "SRL" => Rendered::Stmt(format!("{} >>>= {};", place!(0), src!())),
            "SRC" => Rendered::Stmt(format!("rotr({}, {});", place!(0), src!())),
            "EX" => Rendered::Stmt(format!("swap({}, {});", place!(0), place!(1))),
            "CEQ" | "CH" | "CHE" | "CGT" | "CGE" | "CLOG" => {
                let a = place!(0);
                let b = src!();
                let (pos, neg) = match d.mnemonic {
                    "CEQ" => (format!("{a} == {b}"), format!("{a} != {b}")),
                    "CGT" => (format!("{a} > {b}"), format!("{a} <= {b}")),
                    "CGE" => (format!("{a} >= {b}"), format!("{a} < {b}")),
                    "CH" => (format!("{a} h> {b}"), format!("{a} h<= {b}")),
                    "CHE" => (format!("{a} h>= {b}"), format!("{a} h< {b}")),
                    _ => (format!("({a} & {b}) == 0"), format!("({a} & {b}) != 0")),
                };
                Rendered::Cond { pos, neg }
            }
            // ---- MOVE ----
            "MOVE" => {
                let bits = MoveBits::from_opcode(d.opcode);
                let count = if bits.imm_count {
                    Self::imm(&d.operands[0]).unwrap()
                } else {
                    match self.place(&d.operands[0], false) {
                        Some(p) => p,
                        None => return Rendered::No("operand not expressible"),
                    }
                };
                let dst = if bits.reg_dst {
                    match &d.operands[1] {
                        GOp::Imm8(r) => format!("vreg(0x{r:02X})"),
                        _ => return Rendered::No("odd MOVE register dest"),
                    }
                } else if !bits.not_grom_dst {
                    match &d.operands[1] {
                        GOp::Grom(a) => format!("gram[0x{a:04X}]"),
                        _ => return Rendered::No("odd MOVE GRAM dest"),
                    }
                } else {
                    match self.place(&d.operands[1], false) {
                        Some(p) => p,
                        None => return Rendered::No("operand not expressible"),
                    }
                };
                let src = if bits.ram_src {
                    match self.place(&d.operands[2], false) {
                        Some(p) => p,
                        None => return Rendered::No("operand not expressible"),
                    }
                } else if bits.cpu_held_grom_src {
                    match &d.operands[2] {
                        GOp::Cpu { addr, indirect: false, index: None } => {
                            let cell = self.var(false, true, *addr);
                            format!("grom[*{cell}]")
                        }
                        _ => return Rendered::No("computed-GROM move through a non-simple cell"),
                    }
                } else {
                    match &d.operands[2] {
                        GOp::Grom(a) => match self.data_names.get(a) {
                            Some(n) => format!("grom[{n}]"),
                            None => format!("grom[0x{a:04X}]"),
                        },
                        _ => return Rendered::No("odd MOVE source"),
                    }
                };
                Rendered::Stmt(format!("move({dst}, {src}, {count});"))
            }
            _ => Rendered::No("no GSL spelling"),
        }
    }
}
