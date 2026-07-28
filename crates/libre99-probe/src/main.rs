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

//! `libre99probe` — the headless probe shell (manual: docs/PROBE.md).
//!
//! A REPL over [`libre99_probe::Session`]: boots the emulated console with no
//! window, runs an optional `--script` file, then reads commands from stdin.
//! At a terminal it behaves like an interactive shell (prompt, errors don't
//! exit); on a pipe it behaves like a batch tool (commands are echoed as
//! `> cmd` so the transcript is self-describing, and the first error stops
//! the run with exit code 1) — the mode an AI agent drives it in.

use std::io::{BufRead, IsTerminal, Write};

use libre99_probe::{Reply, Session};

const USAGE: &str = "\
usage: libre99probe [CARTRIDGE] [options]

Boot a headless TI-99/4A (the clean-room firmware by default), optionally
mount CARTRIDGE (.ctg or raw .bin), run --script, then read commands from
stdin. Type 'help' at the prompt for the command language; the manual is
docs/PROBE.md.

options:
  --script FILE       run FILE's commands before reading stdin
  --disk FILE.dsk     mount FILE in DSK1 at startup
  --system-rom FILE   boot this console ROM instead of the clean-room one
  --system-grom FILE  boot this console GROM instead of the clean-room one
  --disk-dsr FILE     install this disk-controller DSR instead of ours
  --version           print the version and exit
  --help, -h          this text";

struct Args {
    cart: Option<String>,
    script: Option<String>,
    disk: Option<String>,
    system_rom: Option<String>,
    system_grom: Option<String>,
    disk_dsr: Option<String>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        cart: None,
        script: None,
        disk: None,
        system_rom: None,
        system_grom: None,
        disk_dsr: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().ok_or(format!("{flag} needs a file argument"));
        match a.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--version" => {
                println!("libre99probe {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--script" => args.script = Some(value("--script")?),
            "--disk" => args.disk = Some(value("--disk")?),
            "--system-rom" => args.system_rom = Some(value("--system-rom")?),
            "--system-grom" => args.system_grom = Some(value("--system-grom")?),
            "--disk-dsr" => args.disk_dsr = Some(value("--disk-dsr")?),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            path => {
                if args.cart.replace(path.to_string()).is_some() {
                    return Err("more than one cartridge path given".into());
                }
            }
        }
    }
    Ok(Some(args))
}

/// Read an override file, or fall back to the embedded clean-room bytes.
fn firmware(path: &Option<String>, default: &[u8]) -> Result<Vec<u8>, String> {
    match path {
        Some(p) => std::fs::read(p).map_err(|e| format!("could not read {p}: {e}")),
        None => Ok(default.to_vec()),
    }
}

fn main() {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let interactive = std::io::stdin().is_terminal();
    let mut session = match build_session(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // Startup media, through the same commands a script would use (so the
    // transcript stays uniform). A path the user gave that doesn't load is a
    // startup error worth stopping for.
    for cmd in [
        args.cart.as_ref().map(|p| format!("cart {p}")),
        args.disk.as_ref().map(|p| format!("disk 1 {p}")),
    ]
    .into_iter()
    .flatten()
    {
        if !run_line(&mut session, &cmd, false) {
            std::process::exit(2);
        }
    }

    // The --script file: echo + execute each line, stop at the first error.
    if let Some(path) = &args.script {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("could not read --script {path}: {e}");
                std::process::exit(2);
            }
        };
        for (i, line) in text.lines().enumerate() {
            match step(&mut session, line, false) {
                Step::Ok => {}
                Step::Quit => return,
                Step::Err => {
                    eprintln!("({path}:{} stopped the run)", i + 1);
                    std::process::exit(1);
                }
            }
        }
    }

    // Stdin: an interactive REPL at a terminal, a batch stream on a pipe.
    if interactive {
        eprintln!(
            "libre99probe {} — type 'help' for commands, 'quit' to leave",
            env!("CARGO_PKG_VERSION")
        );
    }
    let stdin = std::io::stdin();
    loop {
        if interactive {
            eprint!("probe> ");
            let _ = std::io::stderr().flush();
        }
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("stdin: {e}");
                std::process::exit(1);
            }
        }
        match step(&mut session, line.trim_end_matches(['\r', '\n']), interactive) {
            Step::Ok => {}
            Step::Quit => break,
            Step::Err if interactive => {} // keep the REPL alive
            Step::Err => std::process::exit(1),
        }
    }
}

fn build_session(args: &Args) -> Result<Session, String> {
    let rom = firmware(&args.system_rom, libre99_probe::CLEAN_ROM)?;
    let grom = firmware(&args.system_grom, libre99_probe::CLEAN_GROM)?;
    let dsr = firmware(&args.disk_dsr, libre99_probe::CLEAN_DISK_DSR)?;
    Ok(Session::new(&rom, &grom, &dsr))
}

enum Step {
    Ok,
    Quit,
    Err,
}

/// Echo (when not at a terminal — the terminal already shows what was typed)
/// and execute one line; print its reply or error to stdout.
fn step(session: &mut Session, line: &str, interactive: bool) -> Step {
    let shown = line.trim();
    if !interactive && !shown.is_empty() {
        println!("> {shown}");
    }
    match session.exec(line) {
        Ok(Reply::Text(t)) => {
            if !t.is_empty() {
                println!("{t}");
            }
            Step::Ok
        }
        Ok(Reply::Quit) => Step::Quit,
        Err(e) => {
            println!("error: {e}");
            Step::Err
        }
    }
}

/// Run one startup command, reporting an error to stderr. Returns success.
fn run_line(session: &mut Session, line: &str, interactive: bool) -> bool {
    !matches!(step(session, line, interactive), Step::Err)
}
