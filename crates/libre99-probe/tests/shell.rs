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

//! The probe shell, driven end to end: boot the clean-room console with
//! commands only, reach the title and the menu, checkpoint and restore,
//! record evidence. One test mounts a commercial cartridge and is gated on
//! the third-party corpus (skips green when absent), like every other
//! authentic-media suite in the workspace.

use std::path::PathBuf;

use libre99_probe::{Reply, Session};

/// Execute one command, panicking on error, returning its reply text.
fn ok(s: &mut Session, line: &str) -> String {
    match s.exec(line) {
        Ok(Reply::Text(t)) => t,
        Ok(Reply::Quit) => panic!("{line:?} quit the session"),
        Err(e) => panic!("{line:?} failed: {e}"),
    }
}

/// A scratch file path in the target's temp dir, cleaned up by the caller.
fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("libre99probe-test-{}-{name}", std::process::id()))
}

#[test]
fn the_clean_room_console_boots_to_the_title_screen() {
    let mut s = Session::clean_room();
    ok(&mut s, "frames 180");
    ok(&mut s, "settle");
    let screen = ok(&mut s, "screen");
    assert!(
        screen.contains("READY-PRESS ANY KEY TO BEGIN"),
        "title text not on screen:\n{screen}"
    );
    // The health panel agrees this is a sane 32-column screen.
    let state = ok(&mut s, "state");
    assert!(state.contains("mode=graphics1"), "{state}");
}

#[test]
fn press_and_settle_walk_from_the_title_to_the_selection_menu() {
    let mut s = Session::clean_room();
    ok(&mut s, "frames 180");
    ok(&mut s, "press space");
    ok(&mut s, "settle");
    let screen = ok(&mut s, "screen");
    assert!(
        screen.contains("1 FOR TI PYTHON"),
        "selection menu not reached:\n{screen}"
    );
}

#[test]
fn save_and_load_are_an_exact_checkpoint() {
    let path = scratch("checkpoint.state");
    let mut s = Session::clean_room();
    ok(&mut s, "frames 180");
    ok(&mut s, "settle");
    let title = ok(&mut s, "screen");
    ok(&mut s, &format!("save {}", path.display()));

    // Walk away from the checkpointed screen...
    ok(&mut s, "press space");
    ok(&mut s, "settle");
    assert_ne!(ok(&mut s, "screen"), title, "the menu must differ from the title");

    // ...and restore: the exact title screen is back.
    ok(&mut s, &format!("load {}", path.display()));
    assert_eq!(ok(&mut s, "screen"), title);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn peek_poke_screen_shot_and_audio_observe_the_machine() {
    let mut s = Session::clean_room();
    ok(&mut s, "frames 60");

    // Scratchpad round trip through the shell.
    ok(&mut s, "poke 8300 AA 55");
    let dump = ok(&mut s, "peek >8300 2");
    assert!(dump.contains("AA 55"), "{dump}");

    // VDP RAM round trip.
    ok(&mut s, "vpoke 1000 12 34");
    let dump = ok(&mut s, "vpeek 1000 2");
    assert!(dump.contains("12 34"), "{dump}");

    // regs shows the live CPU; audio reports a measurement either way.
    let regs = ok(&mut s, "regs");
    assert!(regs.contains("pc=>") && regs.contains("r15=>"), "{regs}");
    let audio = ok(&mut s, "audio 3");
    assert!(audio.contains("rms="), "{audio}");

    // A PNG screenshot with the right magic.
    let path = scratch("shot.png");
    ok(&mut s, &format!("shot {}", path.display()));
    let png = std::fs::read(&path).expect("screenshot written");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn trace_and_coverage_record_the_evidence_channels() {
    let mut s = Session::clean_room();
    ok(&mut s, "trace on");
    ok(&mut s, "cover on");
    ok(&mut s, "frames 30");

    let summary = ok(&mut s, "trace summary");
    assert!(summary.contains("grom fetch log:"), "{summary}");
    let tail = ok(&mut s, "trace tail 4");
    assert!(tail.contains('='), "{tail}");

    let cover = ok(&mut s, "cover");
    assert!(cover.contains("grom:") && cover.contains("cpu:"), "{cover}");

    // Both save files land on disk with their expected shapes.
    let tpath = scratch("trace.txt");
    let cpath = scratch("cover.txt");
    ok(&mut s, &format!("trace save {}", tpath.display()));
    ok(&mut s, &format!("cover save {}", cpath.display()));
    let trace = std::fs::read_to_string(&tpath).unwrap();
    assert!(trace.lines().next().is_some_and(|l| l.starts_with('>')));
    let cover = std::fs::read_to_string(&cpath).unwrap();
    assert!(cover.contains("grom >") && cover.contains("cpu >"));
    let _ = std::fs::remove_file(&tpath);
    let _ = std::fs::remove_file(&cpath);
}

#[test]
fn vdp_write_trace_is_filtered_attributed_and_excludes_diagnostic_pokes() {
    let mut s = Session::clean_room();

    ok(&mut s, "vtrace on 1000 1000");
    ok(&mut s, "vpoke 1000 AA");
    let status = ok(&mut s, "vtrace show");
    assert!(status.contains("log holds 0 writes"), "{status}");

    ok(&mut s, "vtrace on 0000 3FFF");
    ok(&mut s, "frames 30");
    let tail = ok(&mut s, "vtrace tail 4");
    for field in ["cycle=", "frame=", "pc=>", "opcode=>", "r11=>", "r9=>", "grom=>", "gbyte=>", "op=write-data", "port=>8C00", "vram=>", "byte=>"] {
        assert!(tail.contains(field), "missing {field:?} in {tail}");
    }

    let path = scratch("vtrace.txt");
    ok(&mut s, &format!("vtrace save {}", path.display()));
    let trace = std::fs::read_to_string(&path).unwrap();
    assert!(trace.lines().next().is_some_and(|line| line.contains("pc=>")));
    ok(&mut s, "vtrace clear");
    assert!(ok(&mut s, "vtrace").contains("log holds 0 writes"));
    assert!(ok(&mut s, "vtrace off").contains("vtrace off"));
    assert!(ok(&mut s, "vtrace").contains("no log allocated"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mutable_state_trace_has_explicit_filters_and_machine_readable_attribution() {
    let mut s = Session::clean_room();

    ok(&mut s, "mtrace on vram rw 1000 1000");
    ok(&mut s, "vpoke 1000 AA");
    assert!(ok(&mut s, "mtrace show").contains("log holds 0 accesses"));
    assert!(s.exec("mtrace on grom rw 0000 FFFF").is_err());
    assert!(s.exec("mtrace on cpu r 3FFF 8000").is_err());

    let armed = ok(
        &mut s,
        "mtrace on cpu rw 8375 8375 vram rw 0000 3FFF grom r 0000 FFFF",
    );
    assert!(armed.contains("cpu:rw:>8375-8375"), "{armed}");
    ok(&mut s, "frames 30");
    let tail = ok(&mut s, "mtrace tail 8");
    for field in [
        "cycle=", "frame=", "pc=>", "opcode=>", "r11=>", "r9=>", "grom=>", "gbyte=>", "space=",
        "access=", "addr=>", "byte=>",
    ] {
        assert!(tail.contains(field), "missing {field:?} in {tail}");
    }

    let path = scratch("mtrace.txt");
    ok(&mut s, &format!("mtrace save {}", path.display()));
    let trace = std::fs::read_to_string(&path).unwrap();
    assert!(trace
        .lines()
        .next()
        .is_some_and(|line| line.contains("space=")));
    ok(&mut s, "mtrace clear");
    assert!(ok(&mut s, "mtrace").contains("log holds 0 accesses"));

    let state = scratch("mtrace.state");
    ok(&mut s, &format!("save {}", state.display()));
    ok(&mut s, &format!("load {}", state.display()));
    assert!(ok(&mut s, "mtrace").contains("no log allocated"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&state);
}

#[test]
fn source_scripts_run_with_echoed_transcripts() {
    let path = scratch("script.txt");
    std::fs::write(&path, "# a comment\nframes 2\necho done\n").unwrap();
    let mut s = Session::clean_room();
    let out = ok(&mut s, &format!("source {}", path.display()));
    assert!(out.contains("> frames 2"), "{out}");
    assert!(out.contains("ran 2 frames"), "{out}");
    assert!(out.ends_with("done"), "{out}");
    let _ = std::fs::remove_file(&path);
}

/// The corpus cartridge directory, if present on this machine (mirrors the
/// GSL corpus tests: `third-party/cartridges/`, falling back to the
/// discontinued predecessor checkout).
fn corpus_cartridge(name: &str) -> Option<PathBuf> {
    if let Some(dir) = libre99_core::third_party::dir() {
        let p = dir.join("cartridges").join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    let legacy = PathBuf::from("/Users/Shared/ti-99-emulator/cartridges").join(name);
    legacy.is_file().then_some(legacy)
}

#[test]
fn a_commercial_cartridge_reaches_its_menu_entry_through_the_shell() {
    let Some(path) = corpus_cartridge("tunnelsofdoom.ctg") else {
        eprintln!("SKIPPED: third-party cartridge corpus not present");
        return;
    };
    let mut s = Session::clean_room();
    let mounted = ok(&mut s, &format!("cart {}", path.display()));
    assert!(mounted.contains("TUNNELS OF DOOM"), "{mounted}");
    ok(&mut s, "frames 180");
    ok(&mut s, "press space");
    ok(&mut s, "settle");
    let screen = ok(&mut s, "screen");
    assert!(
        screen.contains("TUNNELS OF DOOM"),
        "cartridge menu entry not on the selection screen:\n{screen}"
    );
}
