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

    // ---- semantic pre-pass -------------------------------------------------
    // Effect signatures per function, literal VDP-register loads, pattern
    // uploads and sound-list starts. All of it feeds names and comments only —
    // the round-trip verification is indifferent to it.
    #[derive(Default)]
    struct FnInfo {
        fmt: bool,
        vdp: bool,
        gram: bool,
        key: bool,
        snd: bool,
        rand: bool,
        xml: BTreeSet<u8>,
        calls: BTreeSet<u16>,
    }
    let mut fninfo: BTreeMap<u16, FnInfo> = BTreeMap::new();
    let mut vreg_loads: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
    let mut vdp_uploads: Vec<(u16, u16, u16)> = Vec::new(); // (vdp dst, grom src, count)
    let mut snd_lists: BTreeMap<u16, u16> = BTreeMap::new(); // list addr -> store site
    {
        let mut cur: Option<u16> = None;
        for (&addr, tile) in &tiles {
            if fn_starts.contains(&addr) {
                cur = Some(addr);
            }
            let Some(fa) = cur else { continue };
            let info = fninfo.entry(fa).or_default();
            let d = match &tile.kind {
                TKind::Fmt(_) => {
                    info.fmt = true;
                    continue;
                }
                TKind::Stmt(d) | TKind::Fallback(d, _) => d,
            };
            if let Flow::Call(t) = d.flow {
                info.calls.insert(t);
            }
            let w = d.opcode & 1 != 0;
            // (operand, is-written) roles per mnemonic shape.
            let mut roles: Vec<(&GOp, bool)> = Vec::new();
            match d.mnemonic {
                "SCAN" => info.key = true,
                "RAND" => info.rand = true,
                "XML" => {
                    if let Some(GOp::Imm8(v)) = d.operands.first() {
                        info.xml.insert(*v);
                    }
                }
                "ST" | "ADD" | "SUB" | "MUL" | "DIV" | "AND" | "OR" | "XOR" | "SLL" | "SRA"
                | "SRL" | "SRC" => {
                    roles.push((&d.operands[0], true));
                    roles.push((&d.operands[1], false));
                }
                "EX" => {
                    roles.push((&d.operands[0], true));
                    roles.push((&d.operands[1], true));
                }
                "INC" | "DEC" | "INCT" | "DECT" | "CLR" | "ABS" | "NEG" | "INV" => {
                    roles.push((&d.operands[0], true));
                }
                "CZ" | "CASE" | "FETCH" | "PUSH" => roles.push((&d.operands[0], false)),
                "CEQ" | "CH" | "CHE" | "CGT" | "CGE" | "CLOG" => {
                    roles.push((&d.operands[0], false));
                    roles.push((&d.operands[1], false));
                }
                "MOVE" => {
                    let bits = MoveBits::from_opcode(d.opcode);
                    if let Some(op) = d.operands.first() {
                        roles.push((op, false));
                    }
                    match d.operands.get(1) {
                        Some(GOp::Imm8(_)) if bits.reg_dst => info.vdp = true,
                        Some(GOp::Grom(_)) if !bits.not_grom_dst => info.gram = true,
                        Some(op) => roles.push((op, true)),
                        None => {}
                    }
                    if let Some(op) = d.operands.get(2) {
                        roles.push((op, false));
                    }
                    // Literal GROM-sourced moves tell us VDP register values
                    // and pattern-table uploads.
                    if let (Some(GOp::Imm16(n)), Some(GOp::Grom(src))) =
                        (d.operands.first(), d.operands.get(2))
                    {
                        if !bits.ram_src && !bits.cpu_held_grom_src {
                            if bits.reg_dst {
                                if let Some(GOp::Imm8(r)) = d.operands.get(1) {
                                    for i in 0..(*n).min(8) {
                                        let reg = *r as u16 + i;
                                        if reg <= 7 && present(src.wrapping_add(i)) {
                                            vreg_loads
                                                .entry(reg as u8)
                                                .or_default()
                                                .insert(img[src.wrapping_add(i) as usize]);
                                        }
                                    }
                                }
                            } else if bits.not_grom_dst {
                                if let Some(GOp::Vdp { addr: va, indirect: false, index: None }) =
                                    d.operands.get(1)
                                {
                                    vdp_uploads.push((*va, *src, *n));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            // A literal word store to >83CC points the ISR at a sound list.
            if d.mnemonic == "ST" && w {
                if let (
                    Some(GOp::Cpu { addr: 0x83CC, indirect: false, index: None }),
                    Some(GOp::Imm16(v)),
                ) = (d.operands.first(), d.operands.get(1))
                {
                    if present(*v) {
                        snd_lists.entry(*v).or_insert(addr);
                    }
                }
            }
            for (op, wr) in roles {
                match op {
                    GOp::Cpu { addr: a, indirect: false, .. } => {
                        let lo = *a;
                        let hi = if w { a.wrapping_add(1) } else { *a };
                        let hit = |s: u16, e: u16| lo <= e && hi >= s;
                        if wr && (hit(0x83CC, 0x83CE) || hit(0x83FD, 0x83FD)) {
                            info.snd = true;
                        }
                        if !wr && hit(0x8374, 0x8377) {
                            info.key = true;
                        }
                        if !wr && (hit(0x8378, 0x8378) || hit(0x83C0, 0x83C1)) {
                            info.rand = true;
                        }
                    }
                    GOp::Vdp { .. } if wr => info.vdp = true,
                    _ => {}
                }
            }
        }
    }

    // Console-default VDP layout, refined by observed literal register loads.
    let mut screen_bases: BTreeSet<u32> = [wellknown::VDP_SCREEN_BASE as u32].into();
    let mut pattern_bases: BTreeSet<u32> = [wellknown::VDP_PATTERN_BASE as u32].into();
    let mut regions: Vec<(u32, u32, String)> = wellknown::VDP_DEFAULT_REGIONS
        .iter()
        .map(|&(s, l, what)| (s as u32, s as u32 + l as u32, what.to_string()))
        .collect();
    for (&r, vals) in &vreg_loads {
        for &v in vals {
            let v = v as u32;
            match r {
                2 => {
                    screen_bases.insert((v & 0x0F) * 0x400);
                }
                3 => {
                    let b = (v * 0x40) & 0x3FFF;
                    regions.push((b, b + 0x20, format!("color table (R3=>{v:02X})")));
                }
                4 => {
                    pattern_bases.insert((v & 7) * 0x800);
                }
                5 => {
                    let b = (v & 0x7F) * 0x80;
                    regions.push((b, b + 0x80, format!("sprite attribute list (R5=>{v:02X})")));
                }
                6 => {
                    let b = (v & 7) * 0x800;
                    if b != 0 {
                        regions.push((b, b + 0x800, format!("sprite patterns (R6=>{v:02X})")));
                    }
                }
                _ => {}
            }
        }
    }
    let vdp_note = |addr: u16| -> Option<String> {
        let a = addr as u32;
        for &b in &screen_bases {
            if (b..b + 768).contains(&a) {
                let off = a - b;
                return Some(format!("screen: row {}, col {}", off / 32, off % 32));
            }
        }
        for (s, e, what) in &regions {
            if (*s..*e).contains(&a) {
                return Some(what.clone());
            }
        }
        for &b in &pattern_bases {
            if (b..b + 0x800).contains(&a) {
                let off = a - b;
                let ch = off / 8;
                let printable = (0x20..0x7F).contains(&ch);
                return Some(format!(
                    "pattern table: char >{ch:02X}{}{}",
                    if printable { format!(" '{}'", ch as u8 as char) } else { String::new() },
                    if off.is_multiple_of(8) { String::new() } else { format!(", row {}", off % 8) },
                ));
            }
        }
        None
    };
    let in_sprite_attr = |addr: u16| {
        regions
            .iter()
            .any(|(s, e, what)| (*s..*e).contains(&(addr as u32)) && what.starts_with("sprite attribute"))
    };

    // Pattern-table uploads mark their GROM source as 8x8 glyph data.
    let mut glyphs: BTreeMap<u16, (u16, u16)> = BTreeMap::new(); // src -> (first char, count)
    for &(dst, src, n) in &vdp_uploads {
        let d = dst as u32;
        for &b in &pattern_bases {
            if d >= b && d + n as u32 <= b + 0x800 && (d - b).is_multiple_of(8) && n >= 8 {
                glyphs.entry(src).or_insert((((d - b) / 8) as u16, n / 8));
            }
        }
    }

    // Heuristic function prefixes — only for neutral sub_ names, and only
    // when the body shows exactly one kind of effect.
    for (&fa, info) in &fninfo {
        if let Some(name) = fn_names.get_mut(&fa) {
            if !name.starts_with("sub_") {
                continue;
            }
            let cats: Vec<&str> = [
                (info.snd, "snd"),
                (info.key, "key"),
                (info.vdp || info.fmt, "draw"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, p)| *p)
            .collect();
            if let [one] = cats[..] {
                *name = format!("{one}_{fa:04X}");
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

    // The enclosing function of a code address (for naming call/ref sites).
    let fn_of = |site: u16| -> Option<&String> {
        fn_starts.range(..=site).next_back().and_then(|f| fn_names.get(f))
    };
    // The data chunk containing an address, as (chunk base, offset).
    let chunk_at = |a: u16| -> Option<(u16, usize)> {
        let i = chunks.partition_point(|c| c.addr <= a);
        if i == 0 {
            return None;
        }
        let c = &chunks[i - 1];
        ((a as usize) < c.addr as usize + c.len).then_some((c.addr, (a - c.addr) as usize))
    };
    // A short ASCII preview of GROM bytes, if they are mostly printable.
    let preview = |a: u16, n: u16| -> Option<String> {
        let n = (n as usize).min(24);
        let s = a as usize;
        if n < 4 || s + n > 0x10000 {
            return None;
        }
        let bytes = &img[s..s + n];
        let printable = bytes.iter().filter(|&&b| (0x20..0x7F).contains(&b)).count();
        (printable * 10 >= n * 8).then(|| {
            bytes
                .iter()
                .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                .collect()
        })
    };
    let is_scan_tile = |t: u16| {
        matches!(tiles.get(&t), Some(Tile { kind: TKind::Stmt(sd), .. }) if sd.mnemonic == "SCAN")
    };
    // An advisory trailing comment for a statement, where we can say
    // something the spelling itself does not.
    let stmt_note = |d: &Decoded| -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        match d.mnemonic {
            "XML" => {
                if let Some(GOp::Imm8(v)) = d.operands.first() {
                    if let Some(t) = wellknown::xml_desc(*v) {
                        parts.push(t);
                    }
                }
            }
            "BACK" => parts.push("backdrop/border color".into()),
            "ALL" => {
                if let Some(GOp::Imm8(v)) = d.operands.first() {
                    let c = if (0x20..0x7F).contains(v) {
                        format!(" '{}'", *v as char)
                    } else {
                        String::new()
                    };
                    parts.push(format!("fill the screen with char >{v:02X}{c}"));
                }
            }
            "BR" | "BS" => {
                if let Flow::Cond(t) = d.flow {
                    if is_scan_tile(t) {
                        parts.push("loops back to the scan()".into());
                    }
                }
            }
            "ST" => {
                if let (
                    Some(GOp::Cpu { addr: 0x83CC, indirect: false, index: None }),
                    Some(GOp::Imm16(v)),
                ) = (d.operands.first(), d.operands.get(1))
                {
                    parts.push(format!("point the ISR at the sound list at >{v:04X}"));
                }
                if let (Some(GOp::Vdp { addr, indirect: false, .. }), Some(GOp::Imm8(0xD0))) =
                    (d.operands.first(), d.operands.get(1))
                {
                    if in_sprite_attr(*addr) {
                        parts.push(">D0 in a sprite Y = end of the sprite list".into());
                    }
                }
            }
            "MOVE" => {
                let bits = MoveBits::from_opcode(d.opcode);
                let count = match d.operands.first() {
                    Some(GOp::Imm16(n)) => Some(*n),
                    _ => None,
                };
                if bits.reg_dst {
                    if let Some(GOp::Imm8(r)) = d.operands.get(1) {
                        match count {
                            Some(n) if n > 1 => parts
                                .push(format!("VDP registers R{}..R{}", r, *r as u16 + n - 1)),
                            _ => parts.push(wellknown::vdp_reg_desc(*r).to_string()),
                        }
                        if let (Some(GOp::Grom(src)), Some(n)) = (d.operands.get(2), count) {
                            if !bits.ram_src && !bits.cpu_held_grom_src && n >= 1 && present(*src)
                            {
                                let vals: Vec<String> = (0..n.min(8))
                                    .map(|i| format!(">{:02X}", img[src.wrapping_add(i) as usize]))
                                    .collect();
                                parts.push(format!("value {}", vals.join(",")));
                            }
                        }
                    }
                } else if bits.not_grom_dst {
                    if let Some(GOp::Vdp { addr, indirect: false, index: None }) =
                        d.operands.get(1)
                    {
                        if let Some(n) = vdp_note(*addr) {
                            parts.push(format!("to the {n}"));
                        }
                    }
                }
                if let Some(GOp::Grom(src)) = d.operands.get(2) {
                    if !bits.ram_src && !bits.cpu_held_grom_src {
                        if let Some((base, off)) = chunk_at(*src) {
                            if off > 0 {
                                parts.push(format!("src = d_{base:04X}+0x{off:02X}"));
                            }
                        }
                        if !glyphs.contains_key(src) {
                            if let Some(n) = count {
                                if let Some(p) = preview(*src, n) {
                                    parts.push(format!("|{p}|"));
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    };

    enum Piece {
        Fn { addr: u16, comments: Vec<String> },
        Stmt { addr: u16, len: u16, text: String, note: Option<String>, demoted: bool },
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
                // Who reads this data, by function name.
                let mut ref_fns: Vec<String> = Vec::new();
                let mut sites = 0usize;
                for (_, v) in gromrefs.range(c.addr..=last) {
                    for &site in v {
                        sites += 1;
                        let n = match fn_of(site) {
                            Some(f) => f.clone(),
                            None => format!(">{site:04X}"),
                        };
                        if !ref_fns.contains(&n) {
                            ref_fns.push(n);
                        }
                    }
                }
                if !ref_fns.is_empty() {
                    let more = if ref_fns.len() > 6 { ", ..." } else { "" };
                    let shown = ref_fns.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
                    comments.push(format!(
                        "// read by move() in {shown}{more} ({sites} site{})",
                        if sites == 1 { "" } else { "s" }
                    ));
                }
                // Sound lists the code points the ISR at (format: [count]
                // [count sound-chip bytes] [duration frames], 0 duration ends).
                for (&sl, &site) in snd_lists.range(c.addr..=last) {
                    let mut p = sl as usize;
                    let mut blocks = 0usize;
                    let mut frames = 0usize;
                    while p < 0x10000 && blocks < 96 {
                        let n = img[p] as usize;
                        if n == 0 || n == 0xFF {
                            break; // control block: jump / toggle+jump
                        }
                        if p + n + 1 >= 0x10000 {
                            break;
                        }
                        let dur = img[p + n + 1] as usize;
                        blocks += 1;
                        frames += dur;
                        p += n + 2;
                        if dur == 0 {
                            break;
                        }
                    }
                    comments.push(format!(
                        "// >{sl:04X}: sound list ({blocks} block{}, ~{frames} frames) — started at >{site:04X}",
                        if blocks == 1 { "" } else { "s" }
                    ));
                }
                pieces.push(Piece::Data { addr: c.addr, len: c.len, comments });
                continue;
            }
            let (addr, tile) = tile_list[ti];
            ti += 1;
            if fn_starts.contains(&addr) {
                let mut comments = Vec::new();
                // Observed effects, from the body's own statements.
                let mut traits_: Vec<String> = Vec::new();
                let info = fninfo.get(&addr);
                if let Some(i) = info {
                    if i.fmt {
                        traits_.push("formats screen text (FMT)".into());
                    }
                    if i.vdp {
                        traits_.push("writes VDP".into());
                    }
                    if i.gram {
                        traits_.push("writes GRAM".into());
                    }
                    if i.key {
                        traits_.push("reads keyboard/joystick".into());
                    }
                    if i.snd {
                        traits_.push("drives sound".into());
                    }
                    if i.rand {
                        traits_.push("uses random numbers".into());
                    }
                    if !i.xml.is_empty() {
                        let list: Vec<String> =
                            i.xml.iter().take(4).map(|x| format!(">{x:02X}")).collect();
                        traits_.push(format!("XML {}", list.join(",")));
                    }
                }
                let mut what: Vec<&str> = Vec::new();
                for e in &entries {
                    if e.addr == addr {
                        what.push(&e.desc);
                    }
                }
                if what.is_empty() {
                    let t = if traits_.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", traits_.join("; "))
                    };
                    comments.push(format!("// >{addr:04X}: subroutine{t}"));
                } else {
                    for w in what {
                        comments.push(format!("// >{addr:04X}: {w}"));
                    }
                    if !traits_.is_empty() {
                        comments.push(format!("// {}", traits_.join("; ")));
                    }
                }
                if let Some(i) = info {
                    if !i.calls.is_empty() {
                        let list: Vec<String> = i
                            .calls
                            .iter()
                            .take(6)
                            .map(|t| match fn_names.get(t) {
                                Some(f) => f.clone(),
                                None => format!(">{t:04X}"),
                            })
                            .collect();
                        let more = if i.calls.len() > 6 { ", ..." } else { "" };
                        comments.push(format!("// calls: {}{more}", list.join(", ")));
                    }
                }
                if let Some(cs) = callers.get(&addr) {
                    let mut names: Vec<String> = Vec::new();
                    for &c in cs {
                        let n = match fn_of(c) {
                            Some(f) => f.clone(),
                            None => format!(">{c:04X}"),
                        };
                        if !names.contains(&n) {
                            names.push(n);
                        }
                    }
                    let more = if names.len() > 6 { ", ..." } else { "" };
                    let shown = names.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
                    comments.push(format!(
                        "// called from: {shown}{more} ({} site{})",
                        cs.len(),
                        if cs.len() == 1 { "" } else { "s" }
                    ));
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
                        Rendered::Stmt(text) => pieces.push(Piece::Stmt {
                            addr,
                            len: tile.len,
                            text,
                            note: stmt_note(d),
                            demoted: false,
                        }),
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
                                        Some((
                                            btile.len,
                                            format!("if ({c}) goto {t};"),
                                            stmt_note(bd),
                                        ))
                                    }
                                    _ => None,
                                }
                            });
                            match fused {
                                Some((blen, text, note)) => {
                                    ti += 1; // consume the branch tile
                                    pieces.push(Piece::Stmt {
                                        addr,
                                        len: tile.len + blen,
                                        text,
                                        note,
                                        demoted: false,
                                    });
                                }
                                None => pieces.push(Piece::Stmt {
                                    addr,
                                    len: tile.len,
                                    text: format!("test({pos});"),
                                    note: None,
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
        push(&mut out, "// annotations: names and comments are advisory analysis; addresses are");
        push(&mut out, "//   the ground truth. Vars at documented machine cells carry standard");
        push(&mut out, "//   names; draw_/key_/snd_ prefixes mark functions whose only observed");
        push(&mut out, "//   effect is display / input / sound; sub_/L_/d_ names are neutral.");
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
                let note: Option<String> = if *vdp {
                    vdp_note(*addr)
                } else {
                    wellknown::describe(*addr).map(str::to_string)
                };
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
                    let e = s + len;
                    let hex_rows = |out: &mut String, from: usize, to: usize| {
                        for row in (from..to).collect::<Vec<_>>().chunks(8) {
                            let items: Vec<String> =
                                row.iter().map(|&i| format!("0x{:02X}", img[i])).collect();
                            let ascii: String = row
                                .iter()
                                .map(|&i| {
                                    let c = img[i];
                                    if (0x20..0x7F).contains(&c) { c as char } else { '.' }
                                })
                                .collect();
                            out.push_str(&format!(
                                "    {:47} // >{:04X} |{ascii}|\n",
                                items.join(", ") + ",",
                                row[0]
                            ));
                        }
                    };
                    let mut i = s;
                    while i < e {
                        // Bytes a pattern-table upload identified as glyphs:
                        // render the 8x8 pixels above the rows.
                        let g = glyphs.range(..=(i as u16)).next_back().and_then(
                            |(&ga, &(c0, n))| {
                                let gs = ga as usize;
                                let ge = gs + n as usize * 8;
                                (i < ge && (i - gs).is_multiple_of(8))
                                    .then(|| (c0 + ((i - gs) / 8) as u16, ge.min(e)))
                            },
                        );
                        if let Some((mut ch, ge)) = g {
                            let mut p = i;
                            while ge - p >= 8 {
                                let band = ((ge - p) / 8).min(8);
                                let hi = ch + band as u16 - 1;
                                push(
                                    &mut out,
                                    &format!(
                                        "    // >{p:04X}: 8x8 patterns, chars >{ch:02X}..>{hi:02X}:"
                                    ),
                                );
                                for row in 0..8 {
                                    let mut line = String::from("    // ");
                                    for gi in 0..band {
                                        let b = img[p + gi * 8 + row];
                                        for bit in (0..8).rev() {
                                            line.push(if b >> bit & 1 != 0 { '#' } else { '.' });
                                        }
                                        line.push(' ');
                                    }
                                    push(&mut out, line.trim_end());
                                }
                                for gi in 0..band {
                                    let items: Vec<String> = (0..8)
                                        .map(|k| format!("0x{:02X}", img[p + gi * 8 + k]))
                                        .collect();
                                    let c = ch + gi as u16;
                                    let pc = if (0x20..0x7F).contains(&c) {
                                        format!(" '{}'", c as u8 as char)
                                    } else {
                                        String::new()
                                    };
                                    push(
                                        &mut out,
                                        &format!(
                                            "    {:47} // >{:04X} char >{:02X}{pc}",
                                            items.join(", ") + ",",
                                            p + gi * 8,
                                            c
                                        ),
                                    );
                                }
                                p += band * 8;
                                ch += band as u16;
                            }
                            if p < ge {
                                hex_rows(&mut out, p, ge);
                            }
                            i = ge;
                            continue;
                        }
                        // A printable run becomes a string literal.
                        let mut j = i;
                        while j < e && (0x20..0x7F).contains(&img[j]) {
                            j += 1;
                        }
                        if j - i >= 8 {
                            let mut p = i;
                            while p < j {
                                let n = (j - p).min(48);
                                let text: String =
                                    img[p..p + n].iter().map(|&b| b as char).collect();
                                push(
                                    &mut out,
                                    &format!("    {:47} // >{p:04X}", format!("{text:?},")),
                                );
                                p += n;
                            }
                            i = j;
                            continue;
                        }
                        // Plain hex until the next glyph or string run.
                        let mut j = i;
                        loop {
                            j += 1;
                            if j >= e || glyphs.contains_key(&(j as u16)) {
                                break;
                            }
                            if (0x20..0x7F).contains(&img[j]) {
                                let mut k = j;
                                while k < e && (0x20..0x7F).contains(&img[k]) {
                                    k += 1;
                                }
                                if k - j >= 8 {
                                    break;
                                }
                                j = k - 1;
                            }
                        }
                        hex_rows(&mut out, i, j);
                        i = j;
                    }
                    push(&mut out, "}");
                }
                Piece::Stmt { addr, len, text, note, demoted } => {
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
                        match note {
                            Some(n) => push(&mut out, &format!("    {text:<43} // {n}")),
                            None => push(&mut out, &format!("    {text}")),
                        }
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
                // Documented machine cells get their standard names; the
                // declaration keeps the address, so nothing is lost.
                if !vdp {
                    if let Some(n) = wellknown::cell_name(addr, word) {
                        return n.to_string();
                    }
                }
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
