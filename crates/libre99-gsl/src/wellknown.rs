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

//! Well-known TI-99/4A machine facts, for annotating decompiled output.
//!
//! Everything here is an interface fact about the *machine* (console
//! ROM/GROM contract, TMS9918A layout), not about any particular program,
//! sourced from this project's clean-room recon dossiers:
//! `original-content/system-roms/RECON.md`, `rom/RECON.md`,
//! `rom/KSCAN-SPEC.md`, and the firmware sources they cite. Names given
//! here become decompiled variable names; descriptions become declaration
//! comments. All of it is advisory — it never affects emitted bytes.

/// One documented scratchpad cell: optional variable names for byte- and
/// word-wide access at this address, plus a one-line description.
struct Cell {
    addr: u16,
    byte_name: Option<&'static str>,
    word_name: Option<&'static str>,
    desc: &'static str,
}

/// The documented `>8300–>83FF` cells (exact addresses; ranges are handled
/// by [`describe`]). Only cells whose meaning is a fixed console contract
/// get a *name* — cells programs commonly reuse as scratch keep hex names.
const CELLS: &[Cell] = &[
    Cell {
        addr: 0x8300,
        byte_name: None,
        word_name: None,
        desc: "GPL variable space; also the XML >F0 machine-language launch vector",
    },
    Cell {
        addr: 0x834A,
        byte_name: None,
        word_name: None,
        desc: "FAC, floating-point accumulator (>834A-8351; free when no XML FP is used)",
    },
    Cell {
        addr: 0x8354,
        byte_name: None,
        word_name: None,
        desc: "FP error code / DSR device-name length",
    },
    Cell {
        addr: 0x8355,
        byte_name: None,
        word_name: None,
        desc: "DSR search-name length (0 = match every node)",
    },
    Cell {
        addr: 0x8356,
        byte_name: None,
        word_name: Some("dsr_name_ptr"),
        desc: "DSR: VDP pointer past the device name",
    },
    Cell {
        addr: 0x835C,
        byte_name: None,
        word_name: None,
        desc: "ARG, floating-point argument (>835C-8363; free when no XML FP is used)",
    },
    Cell {
        addr: 0x836C,
        byte_name: None,
        word_name: None,
        desc: "FP-error GROM warp address",
    },
    Cell {
        addr: 0x836D,
        byte_name: Some("dsr_opcode"),
        word_name: None,
        desc: "DSR chain opcode (>04 power-up, >06 program, >08 DSR, >0A subprogram)",
    },
    Cell {
        addr: 0x836E,
        byte_name: None,
        word_name: Some("vstack_ptr"),
        desc: "VDP value-stack pointer (top element's base)",
    },
    Cell {
        addr: 0x8370,
        byte_name: None,
        word_name: Some("vram_top"),
        desc: "top of free VDP RAM (init >3FFF; DSR power-ups lower it)",
    },
    Cell {
        addr: 0x8372,
        byte_name: Some("dstack_ptr"),
        word_name: None,
        desc: "GPL data-stack byte pointer (low byte of >83xx)",
    },
    Cell {
        addr: 0x8373,
        byte_name: Some("substack_ptr"),
        word_name: None,
        desc: "GPL subroutine-stack byte pointer (low byte of >83xx)",
    },
    Cell {
        addr: 0x8374,
        byte_name: Some("kscan_mode"),
        word_name: None,
        desc: "KSCAN mode (0 default, 1/2 split+joystick, 3-5 full scan)",
    },
    Cell {
        addr: 0x8375,
        byte_name: Some("key_code"),
        word_name: None,
        desc: "KSCAN result key (>FF = no key)",
    },
    Cell {
        addr: 0x8376,
        byte_name: Some("joy_y"),
        word_name: None,
        desc: "KSCAN joystick Y deflection (>04 up, >00, >FC down)",
    },
    Cell {
        addr: 0x8377,
        byte_name: Some("joy_x"),
        word_name: None,
        desc: "KSCAN joystick X deflection",
    },
    Cell {
        addr: 0x8378,
        byte_name: Some("rand_byte"),
        word_name: None,
        desc: "last RAND result",
    },
    Cell {
        addr: 0x8379,
        byte_name: Some("vdp_timer"),
        word_name: None,
        desc: "ISR frame timer (+SPEED per VDP interrupt)",
    },
    Cell {
        addr: 0x837A,
        byte_name: Some("sprite_count"),
        word_name: None,
        desc: "number of sprites the ISR auto-moves",
    },
    Cell {
        addr: 0x837B,
        byte_name: Some("vdp_status_copy"),
        word_name: None,
        desc: "ISR copy of the VDP status register",
    },
    Cell {
        addr: 0x837C,
        byte_name: Some("gpl_status"),
        word_name: None,
        desc: "GPL status byte (H >80, GT >40, cond >20, carry >10, ovf >08)",
    },
    Cell {
        addr: 0x837D,
        byte_name: None,
        word_name: None,
        desc: "interpreter character buffer",
    },
    Cell {
        addr: 0x837E,
        byte_name: Some("cursor_row"),
        word_name: None,
        desc: "screen cursor row (FMT / >837D echo)",
    },
    Cell {
        addr: 0x837F,
        byte_name: Some("cursor_col"),
        word_name: None,
        desc: "screen cursor column",
    },
    Cell {
        addr: 0x83C0,
        byte_name: None,
        word_name: Some("rand_seed"),
        desc: "random seed (seed' = seed*>6FE5 + >7AB9 per RAND/tick)",
    },
    Cell {
        addr: 0x83C2,
        byte_name: Some("isr_disable"),
        word_name: None,
        desc: "ISR duty-disable bits (>80 all, >40 sprites, >20 sound, >10 QUIT)",
    },
    Cell {
        addr: 0x83C4,
        byte_name: None,
        word_name: Some("isr_hook"),
        desc: "user interrupt hook address (0 = none)",
    },
    Cell {
        addr: 0x83C6,
        byte_name: None,
        word_name: None,
        desc: "KSCAN persisted translation state / debounce area (>83C6-83CA)",
    },
    Cell {
        addr: 0x83C8,
        byte_name: None,
        word_name: None,
        desc: "KSCAN last raw scan code, unit 0 (>FF = none)",
    },
    Cell {
        addr: 0x83CC,
        byte_name: None,
        word_name: Some("snd_list_ptr"),
        desc: "ISR sound-list pointer (GROM address unless FLAGS bit >01)",
    },
    Cell {
        addr: 0x83CE,
        byte_name: Some("snd_countdown"),
        word_name: None,
        desc: "sound frame countdown (set 1 to start the list; 0 = inactive)",
    },
    Cell {
        addr: 0x83D0,
        byte_name: None,
        word_name: Some("dsr_search"),
        desc: "DSR search cursor / found-card CRU base (0 = fresh scan)",
    },
    Cell {
        addr: 0x83D2,
        byte_name: None,
        word_name: Some("dsr_entry"),
        desc: "DSR entry address (resume target)",
    },
    Cell {
        addr: 0x83D4,
        byte_name: Some("vdp_r1_shadow"),
        word_name: None,
        desc: "VDP register-1 shadow (ISR blank / KSCAN un-blank use it)",
    },
    Cell {
        addr: 0x83D6,
        byte_name: None,
        word_name: Some("blank_timer"),
        desc: "screen-blank timeout counter (+2 per tick; wrap to 0 blanks)",
    },
    Cell {
        addr: 0x83D8,
        byte_name: None,
        word_name: None,
        desc: "KSCAN saved caller return address",
    },
    Cell { addr: 0x83E0, byte_name: None, word_name: Some("gpl_r0"), desc: "GPLWS R0 (source data)" },
    Cell {
        addr: 0x83E2,
        byte_name: None,
        word_name: Some("gpl_r1"),
        desc: "GPLWS R1 (source address)",
    },
    Cell {
        addr: 0x83E4,
        byte_name: None,
        word_name: Some("gpl_r2"),
        desc: "GPLWS R2 (destination data)",
    },
    Cell {
        addr: 0x83E6,
        byte_name: None,
        word_name: Some("gpl_r3"),
        desc: "GPLWS R3 (destination address)",
    },
    Cell { addr: 0x83E8, byte_name: None, word_name: Some("gpl_r4"), desc: "GPLWS R4" },
    Cell { addr: 0x83EA, byte_name: None, word_name: Some("gpl_r5"), desc: "GPLWS R5" },
    Cell { addr: 0x83EC, byte_name: None, word_name: Some("gpl_r6"), desc: "GPLWS R6" },
    Cell { addr: 0x83EE, byte_name: None, word_name: Some("gpl_r7"), desc: "GPLWS R7" },
    Cell {
        addr: 0x83F0,
        byte_name: None,
        word_name: Some("gpl_r8"),
        desc: "GPLWS R8 (cleared by the ISR every tick)",
    },
    Cell {
        addr: 0x83F2,
        byte_name: None,
        word_name: Some("gpl_r9"),
        desc: "GPLWS R9 (high byte = current opcode)",
    },
    Cell { addr: 0x83F4, byte_name: None, word_name: Some("gpl_r10"), desc: "GPLWS R10" },
    Cell {
        addr: 0x83F6,
        byte_name: None,
        word_name: Some("gpl_r11"),
        desc: "GPLWS R11 (BL linkage)",
    },
    Cell {
        addr: 0x83F8,
        byte_name: None,
        word_name: Some("gpl_r12"),
        desc: "GPLWS R12 (CRU base)",
    },
    Cell {
        addr: 0x83FA,
        byte_name: None,
        word_name: Some("gpl_r13"),
        desc: "GPLWS R13 = >9800, the GROM port base",
    },
    Cell {
        addr: 0x83FC,
        byte_name: Some("gpl_speed"),
        word_name: Some("gpl_r14"),
        desc: "GPLWS R14: SPEED (high) / FLAGS (low), init >0100",
    },
    Cell {
        addr: 0x83FD,
        byte_name: Some("gpl_flags"),
        word_name: None,
        desc: "FLAGS (>20 cassette ISR, >10 verify, >08 16K, >02 multicolor, >01 sound in VDP)",
    },
    Cell {
        addr: 0x83FE,
        byte_name: None,
        word_name: Some("gpl_r15"),
        desc: "GPLWS R15 = >8C02, the VDP write-address port",
    },
];

/// The standard variable name for a scratchpad cell, if the access width
/// matches a documented console-contract cell.
pub fn cell_name(addr: u16, word: bool) -> Option<&'static str> {
    let c = CELLS.iter().find(|c| c.addr == addr)?;
    if word { c.word_name } else { c.byte_name }
}

/// A human note for a scratchpad/system cell, if it is a well-known one.
/// Ranges cover multi-byte areas; exact entries win.
pub fn describe(addr: u16) -> Option<&'static str> {
    if let Some(c) = CELLS.iter().find(|c| c.addr == addr) {
        return Some(c.desc);
    }
    match addr {
        0x8300..=0x8349 => Some("GPL variable space (program-defined)"),
        0x834A..=0x836B => Some("console FP workspace (free when no XML FP is used)"),
        0x8380..=0x83BF => Some("GPL subroutine-stack area (frames grow up from >8380)"),
        0x83C0..=0x83DF => Some("console ISR / interpreter state area"),
        0x83E0..=0x83FF => Some("GPL interpreter workspace (R0..R15)"),
        _ => None,
    }
}

/// XML master table (nibble X of the operand selects a table, nibble Y an
/// entry): table 0 = console FP, table 1 = console utility, table F = the
/// `>8300` launch vector; the rest are RAM/ROM vector-table bases.
const XML_MASTER: [u16; 16] = [
    0x0D1A, 0x12A0, 0x2000, 0x3FC0, 0x3FE0, 0x4010, 0x4030, 0x6010, 0x6030, 0x7000, 0x8000,
    0xA000, 0xB000, 0xC000, 0xD000, 0x8300,
];

/// What an `xml(>XY)` escape calls, per the console ROM dispatch.
pub fn xml_desc(code: u8) -> Option<String> {
    const FLTAB: [&str; 16] = [
        "reset vector (accident of the original table)",
        "ROUND1 (round FAC at digit >8354)",
        "ROUND (round FAC)",
        "STST (store FP status)",
        "OVEXP (overflow-exponent error)",
        "OV (overflow error)",
        "FADD (FAC += ARG)",
        "FSUB (FAC = ARG - FAC)",
        "FMUL (FAC *= ARG)",
        "FDIV (FAC = ARG / FAC)",
        "FCOMP (compare ARG vs FAC)",
        "SADD (value-stack add)",
        "SSUB (value-stack subtract)",
        "SMUL (value-stack multiply)",
        "SDIV (value-stack divide)",
        "SCOMP (value-stack compare)",
    ];
    let (table, entry) = ((code >> 4) as usize, (code & 0x0F) as usize);
    match table {
        0 => Some(format!("console FP: {}", FLTAB[entry])),
        1 => match code {
            0x10 => Some("console CSN (convert string to number)".into()),
            0x11 => Some("console CSNGR (CSN reading GROM text)".into()),
            0x12 => Some("console CFI (convert float to integer)".into()),
            0x19 => Some("console SROM (search ROM device list)".into()),
            0x1A => Some("console SGROM (search GROM device list)".into()),
            0x1B => Some("console PGMCH stub".into()),
            _ => Some("console utility table entry (stub/vestigial)".into()),
        },
        0xF => Some("machine-language launch via the vector at >8300".into()),
        _ => Some(format!(
            "machine language via the vector at >{:04X}",
            XML_MASTER[table] as usize + entry * 2
        )),
    }
}

/// TMS9918A register roles (for annotating `move(vreg(n), …)`).
pub fn vdp_reg_desc(r: u8) -> &'static str {
    match r & 7 {
        0 => "VDP R0: mode bits / external video",
        1 => "VDP R1: 16K, display enable, interrupt enable, mode, sprite size",
        2 => "VDP R2: screen table base = n x >400",
        3 => "VDP R3: color table base = n x >40",
        4 => "VDP R4: pattern table base = n x >800",
        5 => "VDP R5: sprite attribute base = n x >80",
        6 => "VDP R6: sprite pattern base = n x >800",
        _ => "VDP R7: text color / backdrop color",
    }
}

/// The console firmware's power-up VDP layout (VREGS: R2=>F0 R3=>0E R4=>F9
/// R5=>86 R6=>F8): screen >0000, sprite attributes >0300, colors >0380,
/// sprite motion >0780, patterns >0800. `(base, len, what)` — screen and
/// pattern tables are handled separately by the decompiler (row/col and
/// glyph math), so this table carries the rest.
pub const VDP_DEFAULT_REGIONS: &[(u16, u16, &str)] = &[
    (0x0300, 0x80, "sprite attribute list"),
    (0x0380, 0x20, "color table"),
    (0x0780, 0x80, "sprite motion table (4 bytes/sprite)"),
];

/// Console-default screen image table base.
pub const VDP_SCREEN_BASE: u16 = 0x0000;
/// Console-default pattern descriptor table base.
pub const VDP_PATTERN_BASE: u16 = 0x0800;
