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

//! # The probe shell — a headless control surface over the emulated console
//!
//! One [`Session`] wraps a running [`Machine`] and executes line commands
//! against it: run frames, press keys, type text, read the screen back as an
//! ASCII grid, peek/poke memory, trace GROM fetches, record execution
//! coverage, and save/restore full machine states. The command language is
//! deliberately plain — one command per line, `#` comments — so the same
//! session script works for a human at a terminal, a shell here-document,
//! and an AI agent driving the emulator through a pipe. The binary
//! (`libre99probe`, see `src/main.rs`) is a thin REPL over this module; the
//! full manual is `docs/PROBE.md`.
//!
//! The machine is deterministic (no wall clock, no RNG outside the emulated
//! hardware), so a session script is an exact, replayable record of
//! everything that happened — the property the GSL decompiler's planned
//! evidence workflows build on.

use std::fmt::Write as _;
use std::fs;

use libre99_core::cartridge::Cartridge;
use libre99_core::keyboard::TiKey;
use libre99_core::machine::Machine;
use libre99_core::vdp::{HEIGHT, PALETTE, WIDTH};

/// The clean-room console ROM the probe boots by default — the same committed
/// artifact the desktop app embeds (`original-content/system-roms/`).
pub const CLEAN_ROM: &[u8] =
    include_bytes!("../../../original-content/system-roms/rom/console-rom.bin");
/// The clean-room console GROM (see [`CLEAN_ROM`]).
pub const CLEAN_GROM: &[u8] =
    include_bytes!("../../../original-content/system-roms/grom/console-grom.bin");
/// The clean-room disk-controller DSR, installed at startup so `DSK1`–`DSK3`
/// are live as soon as a disk is mounted.
pub const CLEAN_DISK_DSR: &[u8] =
    include_bytes!("../../../original-content/system-roms/disk-dsr/disk-dsr.bin");

/// The sample rate the probe synthesizes PSG audio at for the `audio`
/// command. 48 kHz divides evenly into 60 frames/s (800 samples per frame).
pub const AUDIO_RATE: u32 = 48_000;
const SAMPLES_PER_FRAME: usize = (AUDIO_RATE / 60) as usize;

/// How many consecutive unchanged-screen frames `settle` requires.
const SETTLE_WINDOW: u32 = 30;
/// Frames a `press` holds the key(s) down, and the total it accounts for
/// (hold + release-and-settle) — the same 3-down cadence the test suites use.
const PRESS_HOLD: u32 = 3;
const PRESS_SETTLE: u32 = 27;

/// The result of executing one command line.
pub enum Reply {
    /// Text to show (may be empty — comments and blank lines reply nothing).
    Text(String),
    /// The session asked to end (`quit` / `exit`).
    Quit,
}

/// A live probe session: the machine plus the shell's own bookkeeping.
pub struct Session {
    m: Machine,
    /// Total frames run since the session began (not part of machine state:
    /// `load` restores the machine but this session counter keeps counting).
    frames: u64,
    /// Keys currently held by `hold` (so bare `release` can undo them all).
    held: Vec<TiKey>,
    /// Title of the mounted cartridge, if one was mounted this session.
    cart_title: Option<String>,
    /// `source` nesting depth (bounded to keep scripts from recursing).
    source_depth: u32,
}

impl Session {
    /// Boot a console from the given firmware and install the disk DSR.
    pub fn new(console_rom: &[u8], console_grom: &[u8], disk_dsr: &[u8]) -> Self {
        let mut m = Machine::new(console_rom, console_grom);
        m.load_disk_controller(disk_dsr);
        m.set_audio_sample_rate(AUDIO_RATE);
        Session { m, frames: 0, held: Vec::new(), cart_title: None, source_depth: 0 }
    }

    /// Boot the default machine: the embedded clean-room firmware, exactly
    /// like the desktop app with no overrides.
    pub fn clean_room() -> Self {
        Self::new(CLEAN_ROM, CLEAN_GROM, CLEAN_DISK_DSR)
    }

    /// Read-only access to the machine (integration tests, embedders).
    pub fn machine(&self) -> &Machine {
        &self.m
    }

    /// Execute one command line. `Err` is a one-line message naming what was
    /// wrong; the machine is unchanged by a rejected command.
    pub fn exec(&mut self, line: &str) -> Result<Reply, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(Reply::Text(String::new()));
        }
        let mut words = trimmed.split_whitespace();
        let cmd = words.next().unwrap_or_default().to_ascii_lowercase();
        let args: Vec<&str> = words.collect();
        // The raw remainder after the command word — for arguments that may
        // contain spaces (file paths, text to type or echo).
        let rest = trimmed
            .split_once(char::is_whitespace)
            .map(|(_, r)| r.trim_start())
            .unwrap_or("");

        match cmd.as_str() {
            "help" => Ok(Reply::Text(HELP.trim().to_string())),
            "quit" | "exit" => Ok(Reply::Quit),
            "echo" => Ok(Reply::Text(rest.to_string())),
            "keys" => Ok(Reply::Text(KEY_LIST.trim().to_string())),
            "frames" | "f" => self.cmd_frames(&args),
            "settle" => self.cmd_settle(&args),
            "press" => self.cmd_press(&args),
            "hold" => self.cmd_hold(&args),
            "release" => self.cmd_release(&args),
            "type" => self.cmd_type(rest),
            "screen" => Ok(Reply::Text(self.screen_report())),
            "shot" => self.cmd_shot(rest),
            "state" => Ok(Reply::Text(self.state_report())),
            "regs" => Ok(Reply::Text(self.regs_report())),
            "peek" => self.cmd_peek(&args, false),
            "vpeek" => self.cmd_peek(&args, true),
            "poke" => self.cmd_poke(&args, false),
            "vpoke" => self.cmd_poke(&args, true),
            "cart" => self.cmd_cart(rest),
            "disk" => self.cmd_disk(rest),
            "eject" => self.cmd_eject(&args),
            "reset" => {
                self.m.reset();
                Ok(Reply::Text(format!("console reset (pc=>{:04X})", self.m.cpu().pc())))
            }
            "save" => self.cmd_save(rest),
            "load" => self.cmd_load(rest),
            "trace" => self.cmd_trace(&args, rest),
            "cover" => self.cmd_cover(&args, rest),
            "audio" => self.cmd_audio(&args),
            "source" => self.cmd_source(rest),
            other => Err(format!("unknown command {other:?} — try 'help'")),
        }
    }

    /// Run `n` frames, keeping the session frame counter in step.
    fn run(&mut self, n: u32) {
        for _ in 0..n {
            self.m.run_frame();
        }
        self.frames += n as u64;
    }

    // -- machine control ----------------------------------------------------

    fn cmd_frames(&mut self, args: &[&str]) -> Result<Reply, String> {
        let n = match args {
            [n] => parse_count(n, 1, 1_000_000)?,
            _ => return Err("usage: frames N".into()),
        };
        self.run(n as u32);
        Ok(Reply::Text(format!("ran {n} frames (session total {})", self.frames)))
    }

    fn cmd_settle(&mut self, args: &[&str]) -> Result<Reply, String> {
        let max = match args {
            [] => 600,
            [n] => parse_count(n, SETTLE_WINDOW as usize + 1, 1_000_000)?,
            _ => return Err("usage: settle [MAX_FRAMES]".into()),
        };
        let mut last = self.screen_bytes();
        let mut stable = 0u32;
        for i in 1..=max {
            self.run(1);
            let now = self.screen_bytes();
            if now == last {
                stable += 1;
                if stable >= SETTLE_WINDOW {
                    return Ok(Reply::Text(format!(
                        "settled after {i} frames (screen unchanged for {SETTLE_WINDOW})"
                    )));
                }
            } else {
                stable = 0;
                last = now;
            }
        }
        Ok(Reply::Text(format!("screen still changing after {max} frames")))
    }

    fn cmd_press(&mut self, args: &[&str]) -> Result<Reply, String> {
        if args.is_empty() {
            return Err("usage: press KEY [KEY ...]   (e.g. 'press 2', 'press fctn =')".into());
        }
        let keys = parse_keys(args)?;
        for &k in &keys {
            self.m.set_key(k, true);
        }
        self.run(PRESS_HOLD);
        for &k in keys.iter().rev() {
            self.m.set_key(k, false);
        }
        self.run(PRESS_SETTLE);
        Ok(Reply::Text(format!(
            "pressed {} (+{} frames)",
            join_keys(&keys),
            PRESS_HOLD + PRESS_SETTLE
        )))
    }

    fn cmd_hold(&mut self, args: &[&str]) -> Result<Reply, String> {
        if args.is_empty() {
            return Err("usage: hold KEY [KEY ...]".into());
        }
        let keys = parse_keys(args)?;
        for &k in &keys {
            self.m.set_key(k, true);
            if !self.held.contains(&k) {
                self.held.push(k);
            }
        }
        Ok(Reply::Text(format!("holding {}", join_keys(&self.held))))
    }

    fn cmd_release(&mut self, args: &[&str]) -> Result<Reply, String> {
        let keys = if args.is_empty() || args == ["all"] {
            std::mem::take(&mut self.held)
        } else {
            let keys = parse_keys(args)?;
            self.held.retain(|k| !keys.contains(k));
            keys
        };
        for &k in &keys {
            self.m.set_key(k, false);
        }
        Ok(Reply::Text(if keys.is_empty() {
            "nothing was held".into()
        } else {
            format!("released {}", join_keys(&keys))
        }))
    }

    fn cmd_type(&mut self, rest: &str) -> Result<Reply, String> {
        // An optionally quoted string, so leading/trailing spaces can be typed.
        let text = match rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            Some(inner) => inner,
            None => rest,
        };
        if text.is_empty() {
            return Err("usage: type TEXT   (or: type \"TEXT WITH EDGE SPACES\")".into());
        }
        let mut count = 0u32;
        for c in text.chars() {
            let (modifier, key) = char_to_press(c)
                .ok_or_else(|| format!("cannot type {c:?} on a TI-99/4A keyboard"))?;
            if let Some(m) = modifier {
                self.m.set_key(m, true);
            }
            self.m.set_key(key, true);
            self.run(PRESS_HOLD);
            self.m.set_key(key, false);
            if let Some(m) = modifier {
                self.m.set_key(m, false);
            }
            self.run(PRESS_HOLD);
            count += 1;
        }
        Ok(Reply::Text(format!(
            "typed {text:?} ({count} keys, +{} frames)",
            count * PRESS_HOLD * 2
        )))
    }

    // -- observation --------------------------------------------------------

    /// The name table decoded to bytes — `(columns, 24 rows of bytes)`.
    fn screen_cells(&self) -> (usize, Vec<u8>) {
        let v = self.m.vdp();
        let cols = if v.register(1) & 0x10 != 0 { 40 } else { 32 };
        let base = ((v.register(2) as usize) & 0x0F) << 10;
        let cells = (0..24 * cols)
            .map(|i| v.vram(((base + i) & 0x3FFF) as u16))
            .collect();
        (cols, cells)
    }

    /// The raw name-table bytes (for cheap change detection in `settle`).
    fn screen_bytes(&self) -> Vec<u8> {
        self.screen_cells().1
    }

    /// The current VDP mode, named.
    fn mode_name(&self) -> &'static str {
        let v = self.m.vdp();
        if v.register(1) & 0x10 != 0 {
            "text"
        } else if v.register(1) & 0x08 != 0 {
            "multicolor"
        } else if v.register(0) & 0x02 != 0 {
            "graphics2"
        } else {
            "graphics1"
        }
    }

    /// The screen as a bordered ASCII grid with row numbers. Name-table bytes
    /// are shown as ASCII where printable and `.` elsewhere — right for the
    /// standard character set; games with custom fonts may need `shot`.
    fn screen_report(&self) -> String {
        let (cols, cells) = self.screen_cells();
        let base = ((self.m.vdp().register(2) as usize) & 0x0F) << 10;
        let mut out = format!(
            "screen {cols}x24 ({}), name table at VRAM >{base:04X}\n",
            self.mode_name()
        );
        let _ = writeln!(out, "   +{}+", "-".repeat(cols));
        for row in 0..24 {
            let line: String = cells[row * cols..(row + 1) * cols]
                .iter()
                .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                .collect();
            let _ = writeln!(out, "{row:2} |{line}|");
        }
        let _ = write!(out, "   +{}+", "-".repeat(cols));
        out
    }

    /// A one-look health panel: where execution is, what the VDP is showing.
    fn state_report(&self) -> String {
        let cpu = self.m.cpu();
        let v = self.m.vdp();
        let regs: Vec<String> = (0..8).map(|r| format!("{:02X}", v.register(r))).collect();
        // Name-table fingerprint: a bonkers screen is a flood of one value or noise.
        let (_, cells) = self.screen_cells();
        let mut hist = [0u32; 256];
        for &b in &cells {
            hist[b as usize] += 1;
        }
        let distinct = hist.iter().filter(|&&c| c > 0).count();
        let (top, top_n) = hist
            .iter()
            .enumerate()
            .max_by_key(|&(_, &c)| c)
            .map(|(v, &c)| (v, c))
            .unwrap_or((0, 0));
        format!(
            "frame={} pc=>{:04X} wp=>{:04X} st=>{:04X} grom=>{:04X} cart={}\n\
             vdp mode={} regs=[{}] screen distinct={distinct} top=>{top:02X}x{top_n}",
            self.frames,
            cpu.pc(),
            cpu.wp(),
            cpu.st(),
            self.m.bus().grom_address(),
            self.cart_title.as_deref().map(|t| format!("{t:?}")).unwrap_or_else(|| "none".into()),
            self.mode_name(),
            regs.join(" "),
        )
    }

    fn regs_report(&self) -> String {
        let cpu = self.m.cpu();
        let mut out = format!(
            "pc=>{:04X} wp=>{:04X} st=>{:04X} grom=>{:04X}\n",
            cpu.pc(),
            cpu.wp(),
            cpu.st(),
            self.m.bus().grom_address()
        );
        for row in 0..4 {
            let regs: Vec<String> = (0..4)
                .map(|c| {
                    let n = row * 4 + c;
                    format!("r{n:<2}=>{:04X}", self.m.reg(n))
                })
                .collect();
            let _ = writeln!(out, "{}", regs.join("  "));
        }
        out.trim_end().to_string()
    }

    fn cmd_peek(&mut self, args: &[&str], vdp: bool) -> Result<Reply, String> {
        let (addr, len) = match args {
            [a] => (parse_addr(a)?, 32),
            [a, n] => (parse_addr(a)?, parse_count(n, 1, 4096)?),
            _ => return Err(format!("usage: {}peek ADDR [LEN]", if vdp { "v" } else { "" })),
        };
        let mut out = String::new();
        let mut at = addr;
        let mut left = len;
        while left > 0 {
            let n = left.min(16);
            let bytes: Vec<u8> = (0..n)
                .map(|i| {
                    let a = at.wrapping_add(i as u16);
                    if vdp {
                        self.m.vdp().vram(a & 0x3FFF)
                    } else {
                        self.m.bus().peek(a)
                    }
                })
                .collect();
            let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
            let ascii: String = bytes
                .iter()
                .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                .collect();
            let _ = writeln!(out, ">{at:04X}  {:<47}  |{ascii}|", hex.join(" "));
            at = at.wrapping_add(n as u16);
            left -= n;
        }
        Ok(Reply::Text(out.trim_end().to_string()))
    }

    fn cmd_poke(&mut self, args: &[&str], vdp: bool) -> Result<Reply, String> {
        if args.len() < 2 {
            return Err(format!("usage: {}poke ADDR BYTE [BYTE ...]", if vdp { "v" } else { "" }));
        }
        let addr = parse_addr(args[0])?;
        let bytes: Vec<u8> = args[1..]
            .iter()
            .map(|s| parse_hex_byte(s))
            .collect::<Result<_, _>>()?;
        for (i, &b) in bytes.iter().enumerate() {
            let a = addr.wrapping_add(i as u16);
            if vdp {
                self.m.vdp_mut().set_vram(a & 0x3FFF, b);
            } else {
                self.m.bus_mut().poke(a, b);
            }
        }
        Ok(Reply::Text(format!(
            "wrote {} byte(s) at {}>{addr:04X}",
            bytes.len(),
            if vdp { "VRAM " } else { "" }
        )))
    }

    fn cmd_shot(&mut self, rest: &str) -> Result<Reply, String> {
        if rest.is_empty() {
            return Err("usage: shot FILE.png".into());
        }
        let mut fb = vec![0u32; WIDTH * HEIGHT];
        self.m.render(&mut fb);
        let png = png_indexed_2x(&fb);
        fs::write(rest, &png).map_err(|e| format!("could not write {rest}: {e}"))?;
        Ok(Reply::Text(format!("wrote {rest} ({}x{}, {} bytes)", WIDTH * 2, HEIGHT * 2, png.len())))
    }

    // -- media --------------------------------------------------------------

    fn cmd_cart(&mut self, rest: &str) -> Result<Reply, String> {
        if rest.is_empty() {
            return Err("usage: cart FILE.ctg|FILE.bin".into());
        }
        let bytes = fs::read(rest).map_err(|e| format!("could not read {rest}: {e}"))?;
        let cart =
            Cartridge::parse(&bytes).map_err(|e| format!("could not parse {rest}: {e:?}"))?;
        self.m.mount_cartridge(&cart);
        self.m.reset();
        let title = cart.title.clone();
        self.cart_title = Some(cart.title);
        Ok(Reply::Text(format!(
            "mounted {title:?} ({} ROM bank(s), {} GROM page(s)); console reset",
            cart.rom_banks,
            cart.grom.len()
        )))
    }

    fn cmd_disk(&mut self, rest: &str) -> Result<Reply, String> {
        let (n, path) = rest
            .split_once(char::is_whitespace)
            .map(|(n, p)| (n, p.trim_start()))
            .ok_or("usage: disk N FILE.dsk   (N = 1..3)")?;
        let drive = parse_count(n, 1, 3)?;
        let bytes = fs::read(path).map_err(|e| format!("could not read {path}: {e}"))?;
        let len = bytes.len();
        self.m.mount_disk(drive - 1, bytes);
        Ok(Reply::Text(format!("mounted {path} in DSK{drive} ({len} bytes, live)")))
    }

    fn cmd_eject(&mut self, args: &[&str]) -> Result<Reply, String> {
        let n = match args {
            [n] => parse_count(n, 1, 3)?,
            _ => return Err("usage: eject N   (N = 1..3)".into()),
        };
        self.m.eject_disk(n - 1);
        Ok(Reply::Text(format!("ejected DSK{n}")))
    }

    // -- save states --------------------------------------------------------

    fn cmd_save(&mut self, rest: &str) -> Result<Reply, String> {
        if rest.is_empty() {
            return Err("usage: save FILE".into());
        }
        let bytes = self.m.save_state();
        fs::write(rest, &bytes).map_err(|e| format!("could not write {rest}: {e}"))?;
        Ok(Reply::Text(format!("saved state to {rest} ({} bytes)", bytes.len())))
    }

    fn cmd_load(&mut self, rest: &str) -> Result<Reply, String> {
        if rest.is_empty() {
            return Err("usage: load FILE".into());
        }
        let bytes = fs::read(rest).map_err(|e| format!("could not read {rest}: {e}"))?;
        self.m.load_state(&bytes).map_err(|e| format!("could not load {rest}: {e:?}"))?;
        self.held.clear();
        Ok(Reply::Text(format!(
            "state restored from {rest} (note: trace/cover recording is reset by a load)"
        )))
    }

    // -- tracing and coverage -----------------------------------------------

    fn cmd_trace(&mut self, args: &[&str], rest: &str) -> Result<Reply, String> {
        match args.first().copied() {
            None => {
                let log = self.m.bus().grom_log();
                Ok(Reply::Text(format!("trace log holds {} GROM fetches", log.len())))
            }
            Some("on") => {
                self.m.bus_mut().grom_record(true);
                Ok(Reply::Text("trace on (GROM fetch log restarted)".into()))
            }
            Some("off") => {
                self.m.bus_mut().grom_record(false);
                Ok(Reply::Text(format!(
                    "trace off ({} fetches retained until the next 'trace on')",
                    self.m.bus().grom_log().len()
                )))
            }
            Some("tail") => {
                let n = match args.get(1) {
                    Some(n) => parse_count(n, 1, 4096)?,
                    None => 32,
                };
                let log = self.m.bus().grom_log();
                let tail = &log[log.len().saturating_sub(n)..];
                if tail.is_empty() {
                    return Ok(Reply::Text("trace log is empty (use 'trace on')".into()));
                }
                let mut out = format!("last {} of {} fetches (>ADDR=BYTE):\n", tail.len(), log.len());
                for chunk in tail.chunks(8) {
                    let row: Vec<String> =
                        chunk.iter().map(|(a, b)| format!(">{a:04X}={b:02X}")).collect();
                    let _ = writeln!(out, "  {}", row.join(" "));
                }
                Ok(Reply::Text(out.trim_end().to_string()))
            }
            Some("summary") => {
                let log = self.m.bus().grom_log();
                if log.is_empty() {
                    return Ok(Reply::Text("trace log is empty (use 'trace on')".into()));
                }
                let mut pages = std::collections::BTreeMap::<u16, u64>::new();
                let (mut cart, mut console) = (0u64, 0u64);
                for &(a, _) in log {
                    *pages.entry(a & 0xFF00).or_default() += 1;
                    if a >= 0x6000 {
                        cart += 1;
                    } else {
                        console += 1;
                    }
                }
                let mut by_count: Vec<(u16, u64)> = pages.into_iter().collect();
                by_count.sort_by_key(|&(p, c)| (std::cmp::Reverse(c), p));
                let mut out = format!(
                    "grom fetch log: {} fetches (cart >6000+: {cart}, console: {console})\n\
                     top pages (256-byte, by fetch count):\n",
                    log.len()
                );
                for (page, count) in by_count.iter().take(16) {
                    let _ = writeln!(
                        out,
                        "  >{page:04X}-{:02X}FF  {count:>8}  ({})",
                        page >> 8,
                        if *page >= 0x6000 { "cart" } else { "console" }
                    );
                }
                if by_count.len() > 16 {
                    let _ = writeln!(out, "  ... {} more pages", by_count.len() - 16);
                }
                Ok(Reply::Text(out.trim_end().to_string()))
            }
            Some("save") => {
                let path = rest
                    .split_once(char::is_whitespace)
                    .map(|(_, p)| p.trim_start())
                    .unwrap_or("");
                if path.is_empty() {
                    return Err("usage: trace save FILE".into());
                }
                let log = self.m.bus().grom_log();
                let mut text = String::with_capacity(log.len() * 9);
                for &(a, b) in log {
                    let _ = writeln!(text, ">{a:04X} {b:02X}");
                }
                fs::write(path, text).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Reply::Text(format!("wrote {} fetches to {path}", log.len())))
            }
            Some(other) => Err(format!(
                "unknown trace subcommand {other:?} — on|off|tail [N]|summary|save FILE"
            )),
        }
    }

    fn cmd_cover(&mut self, args: &[&str], rest: &str) -> Result<Reply, String> {
        match args.first().copied() {
            Some("on") => {
                self.m.bus_mut().grom_record_coverage(true);
                self.m.record_pc_coverage(true);
                Ok(Reply::Text("coverage on (GROM reads + CPU PCs, restarted)".into()))
            }
            Some("off") => {
                self.m.bus_mut().grom_record_coverage(false);
                self.m.record_pc_coverage(false);
                Ok(Reply::Text("coverage off (bitmaps dropped)".into()))
            }
            None | Some("summary") => {
                let grom = self.m.bus().grom_coverage_addresses();
                let pc = self.m.pc_coverage_addresses();
                if grom.is_empty() && pc.is_empty() {
                    return Ok(Reply::Text("no coverage recorded (use 'cover on')".into()));
                }
                let cart = grom.iter().filter(|&&a| a >= 0x6000).count();
                let gr = ranges(&grom);
                let pr = ranges(&pc);
                Ok(Reply::Text(format!(
                    "grom: {} addresses read (cart >6000+: {cart}, console: {}) in {} ranges\n\
                     cpu:  {} distinct PCs executed in {} ranges",
                    grom.len(),
                    grom.len() - cart,
                    gr.len(),
                    pc.len(),
                    pr.len()
                )))
            }
            Some("save") => {
                let path = rest
                    .split_once(char::is_whitespace)
                    .map(|(_, p)| p.trim_start())
                    .unwrap_or("");
                if path.is_empty() {
                    return Err("usage: cover save FILE".into());
                }
                let grom = self.m.bus().grom_coverage_addresses();
                let pc = self.m.pc_coverage_addresses();
                let mut text = String::new();
                let _ = writeln!(text, "# libre99probe coverage (inclusive address ranges)");
                let _ = writeln!(text, "# grom addresses read: {}", grom.len());
                for (lo, hi) in ranges(&grom) {
                    let _ = writeln!(text, "grom >{lo:04X}->{hi:04X}");
                }
                let _ = writeln!(text, "# cpu program-counter words executed: {}", pc.len());
                for (lo, hi) in ranges(&pc) {
                    let _ = writeln!(text, "cpu >{lo:04X}->{hi:04X}");
                }
                fs::write(path, text).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Reply::Text(format!(
                    "wrote coverage to {path} (grom {} addrs, cpu {} pcs)",
                    grom.len(),
                    pc.len()
                )))
            }
            Some(other) => {
                Err(format!("unknown cover subcommand {other:?} — on|off|summary|save FILE"))
            }
        }
    }

    fn cmd_audio(&mut self, args: &[&str]) -> Result<Reply, String> {
        let n = match args {
            [] => 60,
            [n] => parse_count(n, 1, 36_000)?,
            _ => return Err("usage: audio [FRAMES]".into()),
        };
        let mut buf = vec![0f32; SAMPLES_PER_FRAME];
        let (mut peak, mut sum_sq, mut samples) = (0f32, 0f64, 0u64);
        for _ in 0..n {
            self.run(1);
            self.m.fill_audio(&mut buf);
            for &s in &buf {
                peak = peak.max(s.abs());
                sum_sq += (s as f64) * (s as f64);
            }
            samples += buf.len() as u64;
        }
        let rms = (sum_sq / samples as f64).sqrt();
        Ok(Reply::Text(format!(
            "audio over {n} frames: rms={rms:.4} peak={peak:.4} ({})",
            if peak > 0.01 { "audible" } else { "silent" }
        )))
    }

    // -- scripts ------------------------------------------------------------

    fn cmd_source(&mut self, rest: &str) -> Result<Reply, String> {
        if rest.is_empty() {
            return Err("usage: source FILE".into());
        }
        if self.source_depth >= 8 {
            return Err("source scripts nested too deeply (max 8)".into());
        }
        let text = fs::read_to_string(rest).map_err(|e| format!("could not read {rest}: {e}"))?;
        self.source_depth += 1;
        let result = self.run_source(rest, &text);
        self.source_depth -= 1;
        result
    }

    fn run_source(&mut self, path: &str, text: &str) -> Result<Reply, String> {
        let mut out = String::new();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let _ = writeln!(out, "> {t}");
            match self.exec(line) {
                Ok(Reply::Text(s)) => {
                    if !s.is_empty() {
                        let _ = writeln!(out, "{s}");
                    }
                }
                Ok(Reply::Quit) => {
                    return Err(format!("{path}:{}: quit is only valid at the top level", i + 1))
                }
                Err(e) => return Err(format!("{path}:{}: {e}", i + 1)),
            }
        }
        Ok(Reply::Text(out.trim_end().to_string()))
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers.
// ---------------------------------------------------------------------------

/// Parse a 16-bit address in the TI convention: `>8370`, `0x8370`, or bare
/// hex `8370`.
fn parse_addr(s: &str) -> Result<u16, String> {
    let t = s
        .strip_prefix('>')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u16::from_str_radix(t, 16).map_err(|_| format!("bad address {s:?} (hex, e.g. >8370)"))
}

/// Parse a hex byte (`>2A`, `0x2A`, or `2A`).
fn parse_hex_byte(s: &str) -> Result<u8, String> {
    let t = s
        .strip_prefix('>')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u8::from_str_radix(t, 16).map_err(|_| format!("bad byte {s:?} (hex, e.g. >2A)"))
}

/// Parse a decimal count within `lo..=hi`.
fn parse_count(s: &str, lo: usize, hi: usize) -> Result<usize, String> {
    let n: usize = s.parse().map_err(|_| format!("bad count {s:?} (decimal)"))?;
    if (lo..=hi).contains(&n) {
        Ok(n)
    } else {
        Err(format!("count {n} out of range {lo}..{hi}"))
    }
}

fn parse_keys(args: &[&str]) -> Result<Vec<TiKey>, String> {
    args.iter().map(|s| key_named(s)).collect()
}

fn join_keys(keys: &[TiKey]) -> String {
    keys.iter().map(|&k| key_name(k)).collect::<Vec<_>>().join("+")
}

/// Resolve a key name (`a`, `7`, `enter`, `fctn`, `joy1-fire`, `=` …).
fn key_named(s: &str) -> Result<TiKey, String> {
    use TiKey::*;
    let lower = s.to_ascii_lowercase();
    let key = match lower.as_str() {
        "a" => A, "b" => B, "c" => C, "d" => D, "e" => E, "f" => F, "g" => G,
        "h" => H, "i" => I, "j" => J, "k" => K, "l" => L, "m" => M, "n" => N,
        "o" => O, "p" => P, "q" => Q, "r" => R, "s" => S, "t" => T, "u" => U,
        "v" => V, "w" => W, "x" => X, "y" => Y, "z" => Z,
        "0" => Num0, "1" => Num1, "2" => Num2, "3" => Num3, "4" => Num4,
        "5" => Num5, "6" => Num6, "7" => Num7, "8" => Num8, "9" => Num9,
        "enter" | "return" => Enter,
        "space" => Space,
        "equals" | "=" => Equals,
        "period" | "." => Period,
        "comma" | "," => Comma,
        "semicolon" | ";" => Semicolon,
        "slash" | "/" => Slash,
        "fctn" => Fctn,
        "shift" => Shift,
        "ctrl" => Ctrl,
        "joy1-up" | "joy1up" => Joy1Up,
        "joy1-down" | "joy1down" => Joy1Down,
        "joy1-left" | "joy1left" => Joy1Left,
        "joy1-right" | "joy1right" => Joy1Right,
        "joy1-fire" | "joy1fire" => Joy1Fire,
        "joy2-up" | "joy2up" => Joy2Up,
        "joy2-down" | "joy2down" => Joy2Down,
        "joy2-left" | "joy2left" => Joy2Left,
        "joy2-right" | "joy2right" => Joy2Right,
        "joy2-fire" | "joy2fire" => Joy2Fire,
        _ => return Err(format!("unknown key {s:?} — see 'keys'")),
    };
    Ok(key)
}

/// The canonical name for a key (inverse of [`key_named`]).
fn key_name(k: TiKey) -> &'static str {
    use TiKey::*;
    match k {
        A => "a", B => "b", C => "c", D => "d", E => "e", F => "f", G => "g",
        H => "h", I => "i", J => "j", K => "k", L => "l", M => "m", N => "n",
        O => "o", P => "p", Q => "q", R => "r", S => "s", T => "t", U => "u",
        V => "v", W => "w", X => "x", Y => "y", Z => "z",
        Num0 => "0", Num1 => "1", Num2 => "2", Num3 => "3", Num4 => "4",
        Num5 => "5", Num6 => "6", Num7 => "7", Num8 => "8", Num9 => "9",
        Enter => "enter", Space => "space", Equals => "=", Period => ".",
        Comma => ",", Semicolon => ";", Slash => "/",
        Fctn => "fctn", Shift => "shift", Ctrl => "ctrl",
        Joy1Up => "joy1-up", Joy1Down => "joy1-down", Joy1Left => "joy1-left",
        Joy1Right => "joy1-right", Joy1Fire => "joy1-fire",
        Joy2Up => "joy2-up", Joy2Down => "joy2-down", Joy2Left => "joy2-left",
        Joy2Right => "joy2-right", Joy2Fire => "joy2-fire",
    }
}

/// ASCII → the TI keypress that produces it: `(modifier, key)`. The table
/// mirrors the desktop app's character layout (`libre99-app/src/input.rs`),
/// including the TI's FCTN glyphs (`"` is FCTN+P, `?` is FCTN+I, …).
/// Letters are sent unmodified — the TI displays capitals either way.
fn char_to_press(c: char) -> Option<(Option<TiKey>, TiKey)> {
    use TiKey::*;
    let plain = |k| Some((None, k));
    let shift = |k| Some((Some(Shift), k));
    let fctn = |k| Some((Some(Fctn), k));
    match c {
        'a'..='z' | 'A'..='Z' => {
            const LETTERS: [TiKey; 26] = [
                A, B, C, D, E, F, G, H, I, J, K, L, M,
                N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
            ];
            plain(LETTERS[(c.to_ascii_uppercase() as u8 - b'A') as usize])
        }
        '0'..='9' => {
            const DIGITS: [TiKey; 10] =
                [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
            plain(DIGITS[(c as u8 - b'0') as usize])
        }
        ' ' => plain(Space),
        '=' => plain(Equals),
        '.' => plain(Period),
        ',' => plain(Comma),
        ';' => plain(Semicolon),
        '/' => plain(Slash),
        '!' => shift(Num1),
        '@' => shift(Num2),
        '#' => shift(Num3),
        '$' => shift(Num4),
        '%' => shift(Num5),
        '^' => shift(Num6),
        '&' => shift(Num7),
        '*' => shift(Num8),
        '(' => shift(Num9),
        ')' => shift(Num0),
        '+' => shift(Equals),
        ':' => shift(Semicolon),
        '<' => shift(Comma),
        '>' => shift(Period),
        '-' => shift(Slash),
        '?' => fctn(I),
        '_' => fctn(U),
        '\'' => fctn(O),
        '"' => fctn(P),
        '~' => fctn(W),
        '[' => fctn(R),
        ']' => fctn(T),
        '{' => fctn(F),
        '}' => fctn(G),
        '\\' => fctn(Z),
        '|' => fctn(A),
        '`' => fctn(C),
        _ => None,
    }
}

/// Collapse a sorted address list into inclusive `(lo, hi)` ranges.
fn ranges(sorted: &[u16]) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    for &a in sorted {
        match out.last_mut() {
            Some((_, hi)) if a == hi.wrapping_add(1) => *hi = a,
            _ => out.push((a, a)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// PNG screenshots: 4-bit indexed color over the fixed TMS9918A palette,
// stored-uncompressed DEFLATE — small, dependency-free, deterministic. The
// same writer the README gallery uses (`readme_gallery.rs`), at 2x scale.
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut typed = kind.to_vec();
    typed.extend_from_slice(data);
    out.extend_from_slice(&typed);
    out.extend_from_slice(&crc32(&typed).to_be_bytes());
}

/// Encode the rendered frame (`WIDTH*HEIGHT` `0x00RRGGBB` pixels, every value
/// a `PALETTE` entry) as a 2x-scaled indexed PNG.
fn png_indexed_2x(fb: &[u32]) -> Vec<u8> {
    const SCALE: usize = 2;
    let to_index = |px: u32| PALETTE.iter().position(|&p| p == px).unwrap_or(0) as u8;
    let (w, h) = (WIDTH * SCALE, HEIGHT * SCALE);
    // Scanlines: filter byte 0, then two 4-bit indices per byte, high first.
    let mut raw = Vec::with_capacity(h * (1 + w / 2));
    for y in 0..h {
        raw.push(0);
        for x in (0..w).step_by(2) {
            let a = to_index(fb[(y / SCALE) * WIDTH + x / SCALE]);
            let b = to_index(fb[(y / SCALE) * WIDTH + (x + 1) / SCALE]);
            raw.push((a << 4) | (b & 0x0F));
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let mut i = 0;
    while i < raw.len() {
        let n = (raw.len() - i).min(0xFFFF);
        zlib.push(if i + n >= raw.len() { 1 } else { 0 });
        zlib.extend_from_slice(&(n as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(n as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[i..i + n]);
        i += n;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[4, 3, 0, 0, 0]); // 4-bit, indexed color
    chunk(&mut out, b"IHDR", &ihdr);
    let mut plte = Vec::with_capacity(48);
    for &c in &PALETTE {
        plte.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    chunk(&mut out, b"PLTE", &plte);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

// ---------------------------------------------------------------------------
// Reference text.
// ---------------------------------------------------------------------------

const HELP: &str = r#"
libre99probe commands (one per line; '#' starts a comment; docs/PROBE.md is the manual)

machine
  frames N            run N frames (60 frames = 1 emulated second); alias: f N
  settle [MAX]        run until the screen stops changing (default cap 600 frames)
  reset               console reset (CPU vectors through >0000)
  press KEY [KEY..]   tap keys together, e.g. 'press 2' or 'press fctn ='
  hold KEY [KEY..]    press and keep holding; 'release' lets go
  release [KEY..|all] release held keys (bare 'release' = all)
  type TEXT           type TEXT character by character ('keys' lists the keyboard)

observe
  screen              the screen as ASCII (VDP name table, 32/40 cols by mode)
  shot FILE.png       PNG screenshot (512x384, indexed, dependency-free)
  state               one-look health: frame, PC, GROM addr, VDP regs, screen shape
  regs                CPU PC/WP/ST, R0-R15, GROM address
  peek ADDR [LEN]     hex dump CPU RAM/ROM (side-effect-free; hex addr, e.g. >8370)
  vpeek ADDR [LEN]    hex dump VDP RAM
  poke ADDR B [B..]   write bytes to CPU RAM     vpoke ADDR B [B..]  to VDP RAM
  audio [FRAMES]      run frames measuring PSG output (rms/peak; is sound playing?)

media
  cart FILE           mount a cartridge (.ctg or raw .bin) and reset
  disk N FILE.dsk     mount a disk image in DSK N (1..3), live
  eject N             empty drive N

evidence
  trace on|off        record every GROM fetch (the GPL execution trace)
  trace tail [N]      show the last N fetches      trace summary   per-page counts
  trace save FILE     write the fetch log (one '>ADDR BYTE' per line)
  cover on|off        record GROM-read + CPU-PC coverage bitmaps
  cover [summary]     coverage counts and range counts
  cover save FILE     write coverage as inclusive address ranges
  save FILE           write a full machine save state (self-contained)
  load FILE           restore a save state (exact checkpoint, incl. media)

session
  source FILE         run a command script (nested up to 8 deep)
  echo TEXT           print TEXT (script markers)
  keys                list key names
  help                this text
  quit / exit         end the session
"#;

const KEY_LIST: &str = r#"
letters    a b c d e f g h i j k l m n o p q r s t u v w x y z
digits     0 1 2 3 4 5 6 7 8 9
named      enter space = . , ; /   (also: equals period comma semicolon slash)
modifiers  fctn shift ctrl   (combine: 'press fctn =' is the TI QUIT chord)
joysticks  joy1-up joy1-down joy1-left joy1-right joy1-fire (and joy2-*)
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use TiKey::*;

    #[test]
    fn addresses_parse_in_all_three_hex_spellings() {
        assert_eq!(parse_addr(">8370").unwrap(), 0x8370);
        assert_eq!(parse_addr("0x8370").unwrap(), 0x8370);
        assert_eq!(parse_addr("8370").unwrap(), 0x8370);
        assert!(parse_addr("xyzzy").is_err());
        assert!(parse_addr("12345").is_err(), "wider than 16 bits");
    }

    #[test]
    fn key_names_round_trip() {
        for name in ["a", "z", "0", "9", "enter", "space", "=", ".", ",", ";", "/",
                     "fctn", "shift", "ctrl", "joy1-fire", "joy2-up"] {
            let k = key_named(name).unwrap();
            assert_eq!(key_named(key_name(k)).unwrap(), k, "{name}");
        }
        assert_eq!(key_named("SPACE").unwrap(), Space, "names are case-insensitive");
        assert!(key_named("meta").is_err());
    }

    #[test]
    fn characters_map_to_the_ti_keyboard_like_the_desktop_app() {
        // The user-visible contract: same table as libre99-app/src/input.rs.
        assert_eq!(char_to_press('a'), Some((None, A)));
        assert_eq!(char_to_press('A'), Some((None, A)));
        assert_eq!(char_to_press('7'), Some((None, Num7)));
        assert_eq!(char_to_press('+'), Some((Some(Shift), Equals)));
        assert_eq!(char_to_press('-'), Some((Some(Shift), Slash)));
        assert_eq!(char_to_press('"'), Some((Some(Fctn), P)));
        assert_eq!(char_to_press('?'), Some((Some(Fctn), I)));
        assert_eq!(char_to_press('\u{e9}'), None, "no é on a TI-99/4A");
    }

    #[test]
    fn sorted_addresses_collapse_into_inclusive_ranges() {
        assert_eq!(
            ranges(&[1, 2, 3, 7, 9, 10]),
            vec![(1, 3), (7, 7), (9, 10)]
        );
        assert!(ranges(&[]).is_empty());
    }

    #[test]
    fn comments_and_blanks_reply_nothing() {
        let mut s = Session::clean_room();
        for line in ["", "   ", "# a comment", "  # indented"] {
            match s.exec(line).unwrap() {
                Reply::Text(t) => assert!(t.is_empty(), "{line:?}"),
                Reply::Quit => panic!("{line:?} must not quit"),
            }
        }
    }

    #[test]
    fn unknown_commands_and_bad_arguments_are_rejected_cleanly() {
        let mut s = Session::clean_room();
        assert!(s.exec("warp 9").is_err());
        assert!(s.exec("frames").is_err());
        assert!(s.exec("frames lots").is_err());
        assert!(s.exec("press meta").is_err());
        assert!(s.exec("type \u{263a}").is_err());
        assert!(s.exec("peek zz").is_err());
        assert!(s.exec("disk 4 x.dsk").is_err());
    }
}
