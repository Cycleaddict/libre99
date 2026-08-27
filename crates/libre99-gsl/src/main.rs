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

//! `libre99gsl` — the GSL command line (docs/GSL.md §11).
//!
//! ```text
//!   libre99gsl compile   <in.gsl> -o <out>  [--format ctg|grom|grom24|rombin]
//!   libre99gsl decompile <in> -o <out.gsl>  [--base 0xNNNN] [--rom] [--entries FILE] [--map FILE]
//!   libre99gsl roundtrip <in> [--keep <out.gsl>] [--base 0xNNNN] [--rom] [--entries FILE] [--map FILE]
//!   libre99gsl verify    <in.gsl> <against>  [--base 0xNNNN] [--rom]
//! ```

use std::process::ExitCode;

use libre99_gsl::ast::OutFormat;
use libre99_gsl::decompile::Options;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("compile") => cmd_compile(&args[1..]),
        Some("decompile") => cmd_decompile(&args[1..]),
        Some("roundtrip") => cmd_roundtrip(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        _ => Err(USAGE.to_string()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage:
  libre99gsl compile   <in.gsl> -o <out>  [--format ctg|grom|grom24|rombin]
  libre99gsl decompile <in.ctg|in.bin> -o <out.gsl>  [--base 0xNNNN] [--rom] [--entries FILE] [--map FILE]
  libre99gsl roundtrip <in.ctg|in.bin> [--keep <out.gsl>] [--base 0xNNNN] [--rom] [--entries FILE] [--map FILE]
  libre99gsl verify    <in.gsl> <against.ctg|.bin>  [--base 0xNNNN] [--rom]";

struct Flags {
    input: String,
    /// A second positional input (only `verify` takes one).
    second: Option<String>,
    output: Option<String>,
    format: Option<OutFormat>,
    base: Option<u16>,
    rom: bool,
    keep: Option<String>,
    entries: Option<String>,
    map: Option<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut f = Flags {
        input: String::new(),
        second: None,
        output: None,
        format: None,
        base: None,
        rom: false,
        keep: None,
        entries: None,
        map: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => f.output = Some(it.next().ok_or("-o needs a path")?.clone()),
            "--keep" => f.keep = Some(it.next().ok_or("--keep needs a path")?.clone()),
            "--entries" => f.entries = Some(it.next().ok_or("--entries needs a path")?.clone()),
            "--map" => f.map = Some(it.next().ok_or("--map needs a path")?.clone()),
            "--rom" => f.rom = true,
            "--base" => {
                let v = it.next().ok_or("--base needs an address")?;
                let v = v.trim_start_matches("0x").trim_start_matches('>');
                f.base = Some(
                    u16::from_str_radix(v, 16).map_err(|_| format!("bad --base '{v}'"))?,
                );
            }
            "--format" => {
                f.format = Some(match it.next().ok_or("--format needs a name")?.as_str() {
                    "ctg" => OutFormat::Ctg,
                    "grom" => OutFormat::Grom,
                    "grom24" => OutFormat::Grom24,
                    "rombin" => OutFormat::RomBin,
                    other => return Err(format!("unknown format '{other}'")),
                });
            }
            other if f.input.is_empty() && !other.starts_with('-') => f.input = other.to_string(),
            other if f.second.is_none() && !other.starts_with('-') => {
                f.second = Some(other.to_string())
            }
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    if f.input.is_empty() {
        return Err(USAGE.into());
    }
    Ok(f)
}

/// Reject a stray second positional on the single-input commands.
fn one_input(f: &Flags) -> Result<(), String> {
    match &f.second {
        Some(x) => Err(format!("unexpected extra argument '{x}'\n{USAGE}")),
        None => Ok(()),
    }
}

/// Read and compile a `.gsl` file, formatting errors for the terminal.
fn compile_gsl(path: &str) -> Result<libre99_gsl::Compiled, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    libre99_gsl::compile(&src).map_err(|errs| {
        let mut s = format!("{path}: {} error(s)\n", errs.len());
        for e in errs.iter().take(20) {
            s.push_str(&format!("  {e}\n"));
        }
        s
    })
}

fn parse_trace_entries(text: &str) -> Result<Vec<u16>, String> {
    let mut entries = Vec::new();
    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.split_once('#').map_or(raw, |(before, _)| before).trim();
        if line.is_empty() {
            continue;
        }
        if line.split_whitespace().count() != 1 {
            return Err(format!(
                "entries line {} must contain one hexadecimal GPL address",
                line_no + 1
            ));
        }
        let digits = line.trim_start_matches("0x").trim_start_matches('>');
        let addr = u16::from_str_radix(digits, 16)
            .map_err(|_| format!("bad GPL address '{line}' on entries line {}", line_no + 1))?;
        entries.push(addr);
    }
    entries.sort_unstable();
    entries.dedup();
    Ok(entries)
}

fn read_trace_entries(path: Option<&str>) -> Result<Vec<u16>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read entries file {path}: {e}"))?;
    parse_trace_entries(&text).map_err(|e| format!("{path}: {e}"))
}

fn cmd_compile(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args)?;
    one_input(&f)?;
    if f.entries.is_some() {
        return Err("--entries applies only to decompile and roundtrip".into());
    }
    if f.map.is_some() {
        return Err("--map applies only to decompile and roundtrip".into());
    }
    let out_path = f.output.ok_or("compile needs -o <out>")?;
    let c = compile_gsl(&f.input)?;
    let format = f
        .format
        .or(c.format)
        .or_else(|| {
            if out_path.ends_with(".ctg") {
                Some(OutFormat::Ctg)
            } else if out_path.ends_with(".bin") {
                Some(if c.pages.is_empty() { OutFormat::RomBin } else { OutFormat::Grom })
            } else {
                None
            }
        })
        .ok_or("no format: use --format, a `format …;` declaration, or a .ctg/.bin output name")?;
    let bytes = libre99_gsl::write_output(&c, format)?;
    std::fs::write(&out_path, &bytes).map_err(|e| format!("cannot write {out_path}: {e}"))?;
    eprintln!(
        "wrote {out_path} ({} bytes, {:?}; {} grom page(s), {} rom bank(s))",
        bytes.len(),
        format,
        c.pages.len(),
        c.rom_banks.len()
    );
    Ok(())
}

fn cmd_decompile(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args)?;
    one_input(&f)?;
    let out_path = f.output.ok_or("decompile needs -o <out.gsl>")?;
    let bytes = std::fs::read(&f.input).map_err(|e| format!("cannot read {}: {e}", f.input))?;
    let trace_entries = read_trace_entries(f.entries.as_deref())?;
    let opts = Options {
        input_name: basename(&f.input),
        base_override: f.base,
        force_rom: f.rom,
        trace_entries,
    };
    let d = libre99_gsl::decompile(&bytes, &opts)?;
    std::fs::write(&out_path, &d.text).map_err(|e| format!("cannot write {out_path}: {e}"))?;
    if let Some(map_path) = f.map {
        std::fs::write(&map_path, d.instruction_map.to_json())
            .map_err(|e| format!("cannot write {map_path}: {e}"))?;
        eprintln!("wrote {map_path}");
    }
    eprintln!("wrote {out_path} ({:?})", d.format);
    print_stats(&d.stats);
    Ok(())
}

fn cmd_roundtrip(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args)?;
    one_input(&f)?;
    let bytes = std::fs::read(&f.input).map_err(|e| format!("cannot read {}: {e}", f.input))?;
    let trace_entries = read_trace_entries(f.entries.as_deref())?;
    let opts = Options {
        input_name: basename(&f.input),
        base_override: f.base,
        force_rom: f.rom,
        trace_entries,
    };
    // decompile() verifies byte-identity internally (it refuses to return
    // otherwise), so reaching here IS the round-trip proof.
    let d = libre99_gsl::decompile(&bytes, &opts)?;
    if let Some(keep) = f.keep {
        std::fs::write(&keep, &d.text).map_err(|e| format!("cannot write {keep}: {e}"))?;
        eprintln!("kept {keep}");
    }
    if let Some(map_path) = f.map {
        std::fs::write(&map_path, d.instruction_map.to_json())
            .map_err(|e| format!("cannot write {map_path}: {e}"))?;
        eprintln!("wrote {map_path}");
    }
    println!(
        "roundtrip OK: {} — {} grom page(s), {} rom byte(s) reproduced byte-identically ({:?})",
        f.input,
        d.payload.grom.len(),
        d.payload.rom.len(),
        d.format,
    );
    print_stats(&d.stats);
    Ok(())
}

/// `verify <in.gsl> <against>` — compile the (possibly hand- or AI-edited)
/// GSL source and prove it still reproduces the original image's payload
/// byte-identically. This is the annotation-safety gate: the comparison is
/// name-blind (renames and comments cannot affect it), so an editing pass
/// that keeps `verify` green can have produced a bad name, never a bad byte.
fn cmd_verify(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args)?;
    if f.entries.is_some() {
        return Err("--entries applies only to decompile and roundtrip".into());
    }
    if f.map.is_some() {
        return Err("--map applies only to decompile and roundtrip".into());
    }
    let against = f
        .second
        .clone()
        .ok_or_else(|| format!("verify needs two inputs: <in.gsl> <against.ctg|.bin>\n{USAGE}"))?;
    let c = compile_gsl(&f.input)?;
    let bytes = std::fs::read(&against).map_err(|e| format!("cannot read {against}: {e}"))?;
    let (want, kind) = libre99_gsl::parse_input(&bytes, f.base, f.rom)?;
    // The format the comparison runs under: an explicit flag, the file's own
    // `format …;` declaration, or whatever the reference image is.
    let format = f.format.or(c.format).unwrap_or_else(|| kind.format());
    let got = libre99_gsl::payload_of(&c, format)?;
    let check_meta = kind == libre99_gsl::container::InputKind::Ctg;
    let diffs = libre99_gsl::container::diff(&want, &got, check_meta);
    if !diffs.is_empty() {
        let mut s = format!(
            "verify FAILED: {} does not reproduce {against} — {} difference(s):\n",
            f.input,
            diffs.len()
        );
        for d in &diffs {
            s.push_str(&format!("  {d}\n"));
        }
        return Err(s.trim_end().to_string());
    }
    println!(
        "verify OK: {} reproduces {against} byte-identically — {} grom page(s), {} rom byte(s) ({format:?})",
        f.input,
        want.grom.len(),
        want.rom.len(),
    );
    Ok(())
}

fn print_stats(s: &libre99_gsl::decompile::Stats) {
    eprintln!(
        "  {} fn(s); {} statements ({} bytes); {} raw-byte instr(s) ({} bytes, {} demoted); {} inline operand bytes; {} data bytes; {} zero bytes elided; {} verify round(s)",
        s.fns,
        s.stmt_instrs,
        s.stmt_bytes,
        s.fallback_instrs + s.demoted_instrs,
        s.fallback_bytes,
        s.demoted_instrs,
        s.inline_bytes,
        s.data_bytes,
        s.elided_zero_bytes,
        s.rounds
    );
}

fn basename(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_flags, parse_trace_entries};

    #[test]
    fn trace_entries_are_hex_only_sorted_and_deduplicated() {
        assert_eq!(
            parse_trace_entries("# trace-confirmed GPL starts\n>6651\n0x6020 # entry\n6651\n")
                .unwrap(),
            vec![0x6020, 0x6651]
        );
        assert!(parse_trace_entries(">6020 extra\n").is_err());
    }

    #[test]
    fn map_flag_is_recorded_for_decompiler_commands() {
        let args = ["input.ctg", "-o", "output.gsl", "--map", "tiles.json"]
            .map(str::to_string);
        let flags = parse_flags(&args).unwrap();
        assert_eq!(flags.map.as_deref(), Some("tiles.json"));
    }
}
