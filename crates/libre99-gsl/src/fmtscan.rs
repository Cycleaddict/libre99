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

use libre99_gpl::operand::decode_gas;

/// A scanned FMT block: total length (including the `>08` opcode and the
/// terminating `FEND`) and a description of each sub-op for comments.
#[derive(Debug, Clone)]
pub struct FmtBlock {
    pub len: usize,
    /// `(address, description)` per sub-op.
    pub ops: Vec<(u16, String)>,
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
                let dir = if b < 0x20 { "HTEX" } else { "VTEX" };
                let text: String = img
                    .get(i + 1..i + 1 + n)?
                    .iter()
                    .map(|&c| if (0x20..0x7F).contains(&c) { c as char } else { '.' })
                    .collect();
                ops.push((at, format!("{dir} {n} \"{text}\"")));
                i += 1 + n;
            }
            0x40..=0x7F => {
                let n = (b & 0x1F) as usize + 1;
                let dir = if b < 0x60 { "HCHA" } else { "VCHA" };
                let ch = *img.get(i + 1)?;
                ops.push((at, format!("{dir} {n} x >{ch:02X}")));
                i += 2;
            }
            0x80..=0xBF => {
                let n = (b & 0x1F) as usize + 1;
                let dir = if b < 0xA0 { "HMOVE" } else { "VMOVE" };
                ops.push((at, format!("{dir} {n}")));
                i += 1;
            }
            0xC0..=0xDF => {
                let n = (b & 0x1F) as usize + 1;
                depth += 1;
                ops.push((at, format!("RPTB {n} passes {{")));
                i += 1;
            }
            0xE0..=0xFA => {
                let n = (b & 0x1F) as usize + 1;
                let (_, glen) = decode_gas(img, i + 1).ok()?;
                ops.push((at, format!("HSTR {n} chars from a GAS operand")));
                i += 1 + glen;
            }
            0xFB => {
                if depth > 0 {
                    depth -= 1;
                    let hi = *img.get(i + 1)? as u16;
                    let lo = *img.get(i + 2)? as u16;
                    ops.push((at, format!("}} FEND (loop back >{:04X})", (hi << 8) | lo)));
                    i += 3;
                } else {
                    ops.push((at, "FEND".into()));
                    i += 1;
                    return Some(FmtBlock { len: i - start, ops });
                }
            }
            0xFC => {
                let v = *img.get(i + 1)?;
                ops.push((at, format!("BIAS >{v:02X}")));
                i += 2;
            }
            0xFD => {
                let (_, glen) = decode_gas(img, i + 1).ok()?;
                ops.push((at, "BIAS from a GAS operand".into()));
                i += 1 + glen;
            }
            0xFE => {
                let v = *img.get(i + 1)?;
                ops.push((at, format!("ROW {v}")));
                i += 2;
            }
            0xFF => {
                let v = *img.get(i + 1)?;
                ops.push((at, format!("COL {v}")));
                i += 2;
            }
        }
    }
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
        assert!(b.ops.iter().any(|(_, d)| d.contains(">1234")));
    }

    #[test]
    fn runaway_is_none() {
        let img = [0x08, 0x1F]; // HTEX of 32 chars, but the image ends
        assert!(scan(&img, 0).is_none());
    }
}
