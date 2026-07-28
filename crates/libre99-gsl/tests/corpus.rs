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

//! Round trips over the local third-party cartridge corpus. These are
//! development-machine tests: they skip (green, with a notice) when the
//! corpus is absent, exactly like the differential firmware suites.
//!
//! The corpus is found through `libre99-core`'s `third_party` gate
//! (`third-party/cartridges/`, or `$LIBRE99_THIRD_PARTY`), falling back to
//! the discontinued predecessor checkout's cartridge folder when present.

use std::path::PathBuf;

use libre99_gsl::decompile::Options;
use libre99_gsl::decompile;

/// The corpus cartridge directory, if any.
fn corpus_dir() -> Option<PathBuf> {
    if let Some(dir) = libre99_core::third_party::dir() {
        let d = dir.join("cartridges");
        if d.is_dir() {
            return Some(d);
        }
    }
    let legacy = PathBuf::from("/Users/Shared/ti-99-emulator/cartridges");
    legacy.is_dir().then_some(legacy)
}

fn corpus_file(name: &str) -> Option<Vec<u8>> {
    let p = corpus_dir()?.join(name);
    std::fs::read(p).ok()
}

macro_rules! skip {
    () => {{
        eprintln!("SKIPPED: third-party cartridge corpus not present");
        return;
    }};
}

#[test]
fn tunnels_of_doom_round_trips_byte_identically() {
    let Some(bytes) = corpus_file("tunnelsofdoom.ctg") else { skip!() };
    let d = decompile(
        &bytes,
        &Options { input_name: "tunnelsofdoom.ctg".into(), ..Default::default() },
    )
    .expect("decompile+verify");
    // decompile() only returns after proving the recompiled payload is
    // byte-identical; assert the analysis quality on top of that.
    assert_eq!(d.payload.title, "TUNNELS OF DOOM");
    assert_eq!(d.payload.grom.len(), 5, "ToD is five GROM pages");
    assert!(d.text.contains("fn prog_tunnels_of_doom()"), "program entry named");
    assert!(d.stats.stmt_instrs > 3000, "real code coverage: {:?}", d.stats);
    assert!(d.text.contains("fmt {"), "FMT blocks decompile to fmt statements");
    assert!(d.text.contains("// prints: "), "printed text lifted into fn headers");
}

/// The annotation-safety contract the `/annotate` skill (and any hand
/// editor) rests on: renaming symbols and adding comments to a decompiled
/// file cannot change the payload it compiles to — `libre99gsl verify`
/// stays green through any names-and-comments-only edit.
#[test]
fn renames_and_comments_cannot_change_the_payload() {
    let Some(bytes) = corpus_file("tunnelsofdoom.ctg") else { skip!() };
    let d = decompile(
        &bytes,
        &Options { input_name: "tunnelsofdoom.ctg".into(), ..Default::default() },
    )
    .expect("decompile+verify");
    // An annotation pass in miniature: rename a function everywhere it
    // appears (declaration, call sites, cross-reference comments) and
    // append an exploration-notes block.
    assert!(d.text.contains("sub_A000"), "expected symbol missing from the decompilation");
    let mut edited = d.text.replace("sub_A000", "annotated_demo_A000");
    edited.push_str("\n// EXPLORATION NOTES — a demo annotation pass.\n");
    let c = libre99_gsl::compile(&edited).expect("edited source still compiles");
    let (want, _) = libre99_gsl::parse_input(&bytes, None, false).expect("parse original");
    let got = libre99_gsl::payload_of(&c, libre99_gsl::OutFormat::Ctg).expect("payload");
    let diffs = libre99_gsl::container::diff(&want, &got, true);
    assert!(diffs.is_empty(), "annotation changed the payload: {diffs:?}");
}

/// Every `.ctg` in the corpus must round-trip byte-identically. Ignored by
/// default (137 cartridges is slow in debug builds):
/// `cargo test -p libre99-gsl --release -- --ignored corpus_sweep`.
#[test]
#[ignore]
fn corpus_sweep_every_cartridge_round_trips() {
    let Some(dir) = corpus_dir() else { skip!() };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ctg")))
        .collect();
    entries.sort();
    assert!(!entries.is_empty());
    let mut pass = 0;
    for p in &entries {
        let bytes = std::fs::read(p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        match decompile(&bytes, &Options { input_name: name.clone(), ..Default::default() }) {
            Ok(_) => pass += 1,
            Err(e) => panic!("{name}: {e}"),
        }
    }
    eprintln!("corpus sweep: {pass}/{} cartridges round-tripped byte-identically", entries.len());
    assert_eq!(pass, entries.len());
}
