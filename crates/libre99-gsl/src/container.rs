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

//! Container handling for the GSL toolchain: sniffing/reading `.ctg` and raw
//! `.bin` inputs into a normalized [`Payload`], and writing a [`Compiled`]
//! program back out in any of the `format` targets. Byte-fidelity is judged on
//! the payload — every GROM page and ROM bank byte, plus the `.ctg` title and
//! CRU base — not on the container framing (`.ctg` RLE is normalized).

use std::collections::BTreeMap;

use libre99_core::cartridge::{self, Cartridge};

use crate::ast::OutFormat;
use crate::codegen::Compiled;

const PAGE: usize = 0x2000;

/// The normalized content of a cartridge/GROM image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub title: String,
    pub cru: u16,
    /// Consecutive 8 KiB ROM banks (empty when GROM-only).
    pub rom: Vec<u8>,
    /// GROM pages by page base address.
    pub grom: BTreeMap<u16, Vec<u8>>,
}

/// What an input file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Ctg,
    /// A raw GROM dump based at `base`.
    GromBin { base: u16 },
    /// A raw CPU-ROM dump.
    RomBin,
}

impl InputKind {
    /// The `format` a decompile of this input should declare.
    pub fn format(self) -> OutFormat {
        match self {
            InputKind::Ctg => OutFormat::Ctg,
            InputKind::GromBin { base: 0 } => OutFormat::Grom24,
            InputKind::GromBin { .. } => OutFormat::Grom,
            InputKind::RomBin => OutFormat::RomBin,
        }
    }
}

/// Sniff and parse an input image. `base_override` forces the GROM base of a
/// raw dump; `force_rom` treats a raw `.bin` as a CPU-ROM dump.
pub fn parse_input(
    bytes: &[u8],
    base_override: Option<u16>,
    force_rom: bool,
) -> Result<(Payload, InputKind), String> {
    if bytes.starts_with(b"TI-99/4A Module - ") {
        let c = Cartridge::parse(bytes).map_err(|e| format!("bad .ctg: {e:?}"))?;
        let mut grom = BTreeMap::new();
        for (addr, page) in &c.grom {
            let mut p = page.clone();
            p.resize(PAGE, 0);
            if grom.insert(*addr, p).is_some() {
                return Err(format!(
                    "GROM page >{addr:04X} appears twice — banked GROM is not supported"
                ));
            }
        }
        return Ok((
            Payload { title: c.title, cru: c.cru_base, rom: c.rom, grom },
            InputKind::Ctg,
        ));
    }
    if bytes.is_empty() {
        return Err("empty input".into());
    }
    if force_rom {
        let mut rom = bytes.to_vec();
        rom.resize(bytes.len().div_ceil(PAGE) * PAGE, 0);
        return Ok((
            Payload { title: String::new(), cru: 0, rom, grom: BTreeMap::new() },
            InputKind::RomBin,
        ));
    }
    // A raw GROM dump. Convention: a 24 KiB image is a console GROM (base
    // >0000); anything else is cartridge GROM (base >6000) unless overridden.
    let base = base_override.unwrap_or(if bytes.len() == 3 * PAGE { 0x0000 } else { 0x6000 });
    if !(base as usize).is_multiple_of(PAGE) {
        return Err(format!("GROM base 0x{base:04X} must be 8 KiB-aligned"));
    }
    let mut grom = BTreeMap::new();
    for (i, chunk) in bytes.chunks(PAGE).enumerate() {
        let addr = base as usize + i * PAGE;
        if addr >= 0x10000 {
            return Err("GROM dump runs past the 64 KiB GROM space".into());
        }
        let mut p = chunk.to_vec();
        p.resize(PAGE, 0);
        grom.insert(addr as u16, p);
    }
    Ok((
        Payload { title: String::new(), cru: 0, rom: Vec::new(), grom },
        InputKind::GromBin { base },
    ))
}

/// The payload a [`Compiled`] program produces under `format`.
pub fn payload_of(c: &Compiled, format: OutFormat) -> Result<Payload, String> {
    let page_of = |base: u16| c.image[base as usize..base as usize + PAGE].to_vec();
    match format {
        OutFormat::Ctg => {
            let grom: BTreeMap<u16, Vec<u8>> = c.pages.iter().map(|&p| (p, page_of(p))).collect();
            Ok(Payload {
                title: c.title.clone().unwrap_or_default(),
                cru: c.cru,
                rom: c.rom_banks.concat(),
                grom,
            })
        }
        OutFormat::Grom | OutFormat::Grom24 => {
            if !c.rom_banks.is_empty() {
                return Err("rom { } banks need format ctg or rombin".into());
            }
            let pages: Vec<u16> = match format {
                OutFormat::Grom24 => {
                    if let Some(&p) = c.pages.iter().find(|&&p| p >= 0x6000) {
                        return Err(format!(
                            "format grom24 covers >0000-5FFF, but page >{p:04X} is populated"
                        ));
                    }
                    vec![0x0000, 0x2000, 0x4000]
                }
                _ => {
                    if c.pages.is_empty() {
                        return Err("nothing to output (no GROM content)".into());
                    }
                    let lo = *c.pages.iter().next().unwrap();
                    let hi = *c.pages.iter().last().unwrap();
                    (lo..=hi).step_by(PAGE).collect()
                }
            };
            Ok(Payload {
                title: String::new(),
                cru: 0,
                rom: Vec::new(),
                grom: pages.into_iter().map(|p| (p, page_of(p))).collect(),
            })
        }
        OutFormat::RomBin => {
            if c.rom_banks.is_empty() {
                return Err("format rombin, but no rom { } banks are defined".into());
            }
            Ok(Payload {
                title: String::new(),
                cru: 0,
                rom: c.rom_banks.concat(),
                grom: BTreeMap::new(),
            })
        }
    }
}

/// Serialize a compiled program in `format`.
pub fn write_output(c: &Compiled, format: OutFormat) -> Result<Vec<u8>, String> {
    let p = payload_of(c, format)?;
    match format {
        OutFormat::Ctg => {
            let grom: Vec<(u16, Vec<u8>)> = p.grom.into_iter().collect();
            Ok(cartridge::write_v1(&p.title, p.cru, &p.rom, &grom))
        }
        OutFormat::Grom | OutFormat::Grom24 => {
            let lo = *p.grom.keys().next().unwrap() as usize;
            let hi = *p.grom.keys().last().unwrap() as usize + PAGE;
            let mut out = vec![0u8; hi - lo];
            for (base, page) in &p.grom {
                out[*base as usize - lo..*base as usize - lo + PAGE].copy_from_slice(page);
            }
            Ok(out)
        }
        OutFormat::RomBin => Ok(p.rom),
    }
}

/// Describe the differences between two payloads (empty = identical). Raw
/// GROM inputs are compared page-map to page-map (a dump whose file length is
/// not a page multiple is zero-padded on read, and reproduced padded).
pub fn diff(want: &Payload, got: &Payload, check_meta: bool) -> Vec<String> {
    let mut out = Vec::new();
    if check_meta {
        if want.title != got.title {
            out.push(format!("title: {:?} != {:?}", want.title, got.title));
        }
        if want.cru != got.cru {
            out.push(format!("cru: >{:04X} != >{:04X}", want.cru, got.cru));
        }
    }
    if want.rom.len() != got.rom.len() {
        out.push(format!("rom: {} bytes != {} bytes", want.rom.len(), got.rom.len()));
    } else if let Some(i) = (0..want.rom.len()).find(|&i| want.rom[i] != got.rom[i]) {
        out.push(format!(
            "rom: first mismatch at bank offset >{i:04X} (>{:02X} != >{:02X})",
            want.rom[i], got.rom[i]
        ));
    }
    let wp: Vec<u16> = want.grom.keys().copied().collect();
    let gp: Vec<u16> = got.grom.keys().copied().collect();
    if wp != gp {
        out.push(format!("grom pages: {wp:04X?} != {gp:04X?}"));
        return out;
    }
    for (base, wpage) in &want.grom {
        let gpage = &got.grom[base];
        if let Some(i) = (0..PAGE).find(|&i| wpage[i] != gpage[i]) {
            out.push(format!(
                "grom >{base:04X}: first mismatch at >{:04X} (>{:02X} != >{:02X})",
                *base as usize + i,
                wpage[i],
                gpage[i]
            ));
        }
    }
    out
}

/// The addresses of every mismatching GROM byte (for the decompiler's
/// demote-and-retry loop).
pub fn grom_mismatches(want: &Payload, got: &Payload) -> Vec<u16> {
    let mut out = Vec::new();
    for (base, wpage) in &want.grom {
        if let Some(gpage) = got.grom.get(base) {
            for i in 0..PAGE {
                if wpage[i] != gpage[i] {
                    out.push(*base + i as u16);
                }
            }
        }
    }
    out
}
