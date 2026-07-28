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

//! A grammar-exact scanner for GPL's `FMT` (`>08`) sub-language — the
//! screen-format interpreter documented (from the clean-room ROM work) in
//! `original-content/system-roms/rom/RECON.md` §7. The generic decoder's
//! scan-to-first-`>FB` is wrong for text payloads containing `>FB` and for
//! `RPTB` loop blocks (whose closing `FEND` is followed by a two-byte
//! loop-back address); this one follows the real grammar, so the decompiler
//! tiles the bytes *after* an FMT block correctly and can annotate the
//! sub-ops.

use libre99_gpl::disasm::format_operand;
use libre99_gpl::operand::{decode_gas, encode_gas, Operand as GOp};

/// One decoded FMT sub-op (counts are the actual 1-based counts; the wire
/// encoding is `count-1` in the low five bits).
#[derive(Debug, Clone)]
pub enum Ir {
    Text { vertical: bool, bytes: Vec<u8> },
    Chars { vertical: bool, count: u8, ch: u8 },
    Skip { vertical: bool, count: u8 },
    Row(u8),
    Col(u8),
    BiasImm(u8),
    BiasGas(GOp),
    HStr { count: u8, gas: GOp },
    /// Opens a repeat loop of `count` passes.
    Rptb { count: u8 },
    /// Closes the innermost loop; `back` is the recorded loop-back address.
    FendLoop { back: u16 },
    /// Terminates the block (always the last op).
    Fend,
}

/// A comment-friendly one-liner for a sub-op (the pre-`fmt { }` notation,
/// still used for blocks that fall back to raw bytes).
pub fn describe(ir: &Ir) -> String {
    match ir {
        Ir::Text { vertical, bytes } => {
            let text: String = bytes
                .iter()
                .map(|&c| if (0x20..0x7F).contains(&c) { c as char } else { '.' })
                .collect();
            format!("{} {} \"{text}\"", if *vertical { "VTEX" } else { "HTEX" }, bytes.len())
        }
        Ir::Chars { vertical, count, ch } => {
            format!("{} {count} x >{ch:02X}", if *vertical { "VCHA" } else { "HCHA" })
        }
        Ir::Skip { vertical, count } => {
            format!("{} {count}", if *vertical { "VMOVE" } else { "HMOVE" })
        }
        Ir::Row(v) => format!("ROW {v}"),
        Ir::Col(v) => format!("COL {v}"),
        Ir::BiasImm(v) => format!("BIAS >{v:02X}"),
        Ir::BiasGas(g) => format!("BIAS from {}", format_operand(g)),
        Ir::HStr { count, gas } => format!("HSTR {count} chars from {}", format_operand(gas)),
        Ir::Rptb { count } => format!("RPTB {count} passes {{"),
        Ir::FendLoop { back } => format!("}} FEND (loop back >{back:04X})"),
        Ir::Fend => "FEND".into(),
    }
}

/// A scanned FMT block: total length (including the `>08` opcode and the
/// terminating `FEND`) and each sub-op with its address.
#[derive(Debug, Clone)]
pub struct FmtBlock {
    pub len: usize,
    pub ops: Vec<(u16, Ir)>,
}

/// Scan the FMT block whose `>08` opcode is at `addr` in the 64 KiB GROM
/// image. Returns `None` if the stream runs off the image, exceeds an 8 KiB
/// budget (runaway — almost certainly mis-identified data), or contains a
/// malformed GAS operand.
pub fn scan(img: &[u8], addr: u16) -> Option<FmtBlock> {
    let start = addr as usize;
    debug_assert_eq!(img.get(start).copied(), Some(0x08));
    let mut i = start + 1;
    let mut depth = 0usize;
    let mut ops = Vec::new();
    loop {
        if i >= img.len() || i - start > 0x2000 {
            return None;
        }
        let at = i as u16;
        let b = img[i];
        match b {
            0x00..=0x3F => {
                let n = (b & 0x1F) as usize + 1;
                let bytes = img.get(i + 1..i + 1 + n)?.to_vec();
                ops.push((at, Ir::Text { vertical: b >= 0x20, bytes }));
                i += 1 + n;
            }
            0x40..=0x7F => {
                let ch = *img.get(i + 1)?;
                ops.push((at, Ir::Chars { vertical: b >= 0x60, count: (b & 0x1F) + 1, ch }));
                i += 2;
            }
            0x80..=0xBF => {
                ops.push((at, Ir::Skip { vertical: b >= 0xA0, count: (b & 0x1F) + 1 }));
                i += 1;
            }
            0xC0..=0xDF => {
                depth += 1;
                ops.push((at, Ir::Rptb { count: (b & 0x1F) + 1 }));
                i += 1;
            }
            0xE0..=0xFA => {
                let (gas, glen) = decode_gas(img, i + 1).ok()?;
                ops.push((at, Ir::HStr { count: (b & 0x1F) + 1, gas }));
                i += 1 + glen;
            }
            0xFB => {
                if depth > 0 {
                    depth -= 1;
                    let hi = *img.get(i + 1)? as u16;
                    let lo = *img.get(i + 2)? as u16;
                    ops.push((at, Ir::FendLoop { back: (hi << 8) | lo }));
                    i += 3;
                } else {
                    ops.push((at, Ir::Fend));
                    i += 1;
                    return Some(FmtBlock { len: i - start, ops });
                }
            }
            0xFC => {
                let v = *img.get(i + 1)?;
                ops.push((at, Ir::BiasImm(v)));
                i += 2;
            }
            0xFD => {
                let (gas, glen) = decode_gas(img, i + 1).ok()?;
                ops.push((at, Ir::BiasGas(gas)));
                i += 1 + glen;
            }
            0xFE => {
                let v = *img.get(i + 1)?;
                ops.push((at, Ir::Row(v)));
                i += 2;
            }
            0xFF => {
                let v = *img.get(i + 1)?;
                ops.push((at, Ir::Col(v)));
                i += 2;
            }
        }
    }
}

/// Re-encode a scanned block the way the `fmt { }` compiler would: loop-back
/// words are derived from the block structure and GAS operands re-encode
/// canonically. Byte-equality with the original bytes proves the block can
/// be emitted as `fmt { }` statements and recompile identically.
pub fn reencode(block: &FmtBlock, start: u16) -> Option<Vec<u8>> {
    let mut out = vec![0x08u8];
    let mut loops: Vec<u16> = Vec::new();
    for (_, ir) in &block.ops {
        let at = start.wrapping_add(out.len() as u16);
        match ir {
            Ir::Text { vertical, bytes } => {
                if bytes.is_empty() || bytes.len() > 32 {
                    return None;
                }
                out.push(if *vertical { 0x20 } else { 0x00 } | (bytes.len() - 1) as u8);
                out.extend_from_slice(bytes);
            }
            Ir::Chars { vertical, count, ch } => {
                out.push(if *vertical { 0x60 } else { 0x40 } | (count - 1));
                out.push(*ch);
            }
            Ir::Skip { vertical, count } => {
                out.push(if *vertical { 0xA0 } else { 0x80 } | (count - 1));
            }
            Ir::Row(v) => out.extend_from_slice(&[0xFE, *v]),
            Ir::Col(v) => out.extend_from_slice(&[0xFF, *v]),
            Ir::BiasImm(v) => out.extend_from_slice(&[0xFC, *v]),
            Ir::BiasGas(g) => {
                out.push(0xFD);
                encode_gas(g, &mut out).ok()?;
            }
            Ir::HStr { count, gas } => {
                out.push(0xE0 | (count - 1));
                encode_gas(gas, &mut out).ok()?;
            }
            Ir::Rptb { count } => {
                out.push(0xC0 | (count - 1));
                loops.push(at.wrapping_add(1));
            }
            Ir::FendLoop { .. } => {
                let back = loops.pop()?;
                out.push(0xFB);
                out.push((back >> 8) as u8);
                out.push(back as u8);
            }
            Ir::Fend => out.push(0xFB),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_containing_fb_does_not_end_the_block() {
        // FMT, HTEX of 2 chars { >FB, >41 }, FEND.
        let img = [0x08, 0x01, 0xFB, 0x41, 0xFB];
        let b = scan(&img, 0).unwrap();
        assert_eq!(b.len, 5);
    }

    #[test]
    fn rptb_fend_carries_a_loopback_address() {
        // FMT, RPTB 1, HMOVE 1, FEND+addr (inside loop), FEND (terminates).
        let img = [0x08, 0xC0, 0x80, 0xFB, 0x12, 0x34, 0xFB];
        let b = scan(&img, 0).unwrap();
        assert_eq!(b.len, 7);
        assert!(b.ops.iter().any(|(_, d)| describe(d).contains(">1234")));
    }

    #[test]
    fn reencode_derives_structural_loopbacks() {
        // The recorded loop-back matches the structure: byte-identical.
        let good = [0x08, 0xC0, 0x80, 0xFB, 0x00, 0x02, 0xFB];
        let b = scan(&good, 0).unwrap();
        assert_eq!(reencode(&b, 0).unwrap(), good);
        // A perverse recorded loop-back re-encodes structurally — different
        // bytes, so the decompiler will keep this block as raw BYTEs.
        let odd = [0x08, 0xC0, 0x80, 0xFB, 0x12, 0x34, 0xFB];
        let b = scan(&odd, 0).unwrap();
        assert_ne!(reencode(&b, 0).unwrap(), odd);
    }

    #[test]
    fn runaway_is_none() {
        let img = [0x08, 0x1F]; // HTEX of 32 chars, but the image ends
        assert!(scan(&img, 0).is_none());
    }
}
