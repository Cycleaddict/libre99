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

//! Well-known TI-99/4A scratchpad cells, for annotating decompiled variable
//! declarations. Sourced from this project's clean-room recon dossiers
//! (`original-content/system-roms/RECON.md` and `rom/RECON.md`) — interface
//! facts about the *machine*, not about any particular program.

/// A human note for a scratchpad/system cell, if it is a well-known one.
/// Ranges cover multi-byte areas; exact entries win.
pub fn describe(addr: u16) -> Option<&'static str> {
    let exact = match addr {
        0x8370 => "top of free VDP RAM (lowered by DSR power-ups)",
        0x8372 => "GPL data-stack pointer (low byte of >83xx)",
        0x8373 => "GPL subroutine-stack pointer (low byte of >83xx)",
        0x8374 => "KSCAN: keyboard/joystick scan mode",
        0x8375 => "KSCAN: detected key (>FF = none)",
        0x8376 => "KSCAN: joystick Y",
        0x8377 => "KSCAN: joystick X",
        0x8378 => "RAND: last random number",
        0x8379 => "ISR: VDP-interrupt timer (increments each tick)",
        0x837A => "highest sprite number auto-moved by the ISR",
        0x837B => "ISR: copy of the VDP status register",
        0x837C => "GPL status byte (condition/carry/overflow/high/greater bits)",
        0x837D => "FMT: character/bias scratch",
        0x837E => "FMT: screen cursor row",
        0x837F => "FMT: screen cursor column",
        0x8354 => "DSR: peripheral CRU base / error code",
        0x8356 => "DSR: pointer to the device name in the PAB",
        0x836D => "DSR: operation code (e.g. >04 = power-up)",
        0x83C2 => "console ISR: disable flags (>80 all, >40 motion, >20 sound, >10 quit)",
        0x83C4 => "console ISR: user interrupt hook (0 = none)",
        0x83C6 => "KSCAN: debounce / last-key state",
        0x83C8 => "KSCAN: repeat/timeout state",
        0x83CC => "console ISR: sound-list pointer (GROM address)",
        0x83CE => "console ISR: sound bytes remaining / trigger",
        0x83D0 => "DSR scan: done flag / saved CRU",
        0x83D4 => "console ISR: VDP register 1 image (screen-blank timeout writes it)",
        0x83D6 => "console ISR: screen-blank timeout counter",
        0x83E0 => "GPL interpreter workspace (R0..R15 at >83E0-83FF)",
        _ => "",
    };
    if !exact.is_empty() {
        return Some(exact);
    }
    match addr {
        0x8300..=0x834F => Some("GPL variable space (program-defined)"),
        0x8380..=0x83BF => Some("GPL data-stack area"),
        0x83C0..=0x83DF => Some("console ISR / interpreter state area"),
        0x83E1..=0x83FF => Some("GPL interpreter workspace (R0..R15)"),
        _ => None,
    }
}
