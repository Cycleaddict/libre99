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

//! Decompile → recompile round trips over this repository's own committed
//! images. Byte-identity of the payload *is* the functional-equivalence
//! proof: identical GROM/ROM bytes execute identically.
//!
//! `decompile()` verifies byte-identity internally and refuses to return
//! otherwise; these tests additionally recompile the emitted text through the
//! public API and compare the serialized output themselves, so the guarantee
//! is checked end to end and not just trusted.

use std::path::PathBuf;

use libre99_gsl::decompile::Options;
use libre99_gsl::{compile, decompile, payload_of, write_output, OutFormat};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

#[test]
fn committed_console_grom_round_trips_byte_identically() {
    let original = std::fs::read(repo("original-content/system-roms/grom/console-grom.bin"))
        .expect("committed console-grom.bin");
    let d = decompile(
        &original,
        &Options { input_name: "console-grom.bin".into(), ..Default::default() },
    )
    .expect("decompile");
    assert_eq!(d.format, OutFormat::Grom24);

    // Independently recompile the emitted text and compare the output file.
    let c = compile(&d.text).expect("recompile");
    let bytes = write_output(&c, OutFormat::Grom24).expect("serialize");
    assert_eq!(bytes, original, "recompiled grom24 image must be byte-identical");

    // The tracer must actually lift code, not degenerate into all-data.
    assert!(d.stats.stmt_instrs > 500, "expected real coverage, got {:?}", d.stats);
    assert!(d.stats.fns > 10, "expected fn discovery, got {:?}", d.stats);
    assert!(d.text.contains("fn console_boot()"), "boot entry named");
}

#[test]
fn original_content_cartridges_round_trip() {
    for rel in [
        "original-content/cartridges/titris/titris.ctg",
        "original-content/cartridges/sokoban/sokoban.ctg",
        "original-content/cartridges/jaywalker99/jaywalker99.ctg",
    ] {
        let original = std::fs::read(repo(rel)).expect(rel);
        let d = decompile(&original, &Options { input_name: rel.into(), ..Default::default() })
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        // Recompile and compare payloads (the .ctg RLE framing is normalized,
        // so the payload — title, CRU, banks, pages — is the identity).
        let c = compile(&d.text).unwrap_or_else(|e| panic!("{rel}: recompile: {e:?}"));
        let got = payload_of(&c, OutFormat::Ctg).unwrap();
        assert_eq!(got, d.payload, "{rel}: payload must round-trip");
        // These are TMS9900 ROM cartridges: their whole content is one bank.
        assert_eq!(got.rom.len(), 0x2000, "{rel}");
    }
}

#[test]
fn synthetic_gpl_cartridge_survives_a_full_cycle() {
    // Author a small GPL cartridge in GSL, serialize a .ctg, decompile it,
    // recompile, and compare payloads — a self-contained end-to-end cycle
    // with no external files.
    let src = r#"
        format ctg;
        cartridge "CYCLE";
        origin 0x6000;
        data hdr {
            0xAA, 0x01, 0x01, 0x00,                  // valid, version, programs, reserved
            word 0, word 0x6010, word 0, word 0,     // power-up, programs, DSR, subprograms
            word 0,                                  // interrupt list
            0x00, 0x00,                              // pad to the list node at >6010
            word 0, word 0x6020, 0x05, "CYCLE",      // node: next, entry, name
        }
        var key: byte @ cpu[0x8375];
        fn main() @ 0x6020 {
            all(0x20);
            back(0x17);
        wait:
            scan();
            if (key == 0xFF) goto wait;
            if (key h> 0x60) goto lower;
            exit();
        lower:
            move(vdp[0x0000], grom[0x6050], 8);
            goto wait;
        }
        data font @ 0x6050 { 0x00, 0x3C, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x00, }
    "#;
    let c = compile(src).expect("compile");
    let ctg = write_output(&c, OutFormat::Ctg).expect("write");
    let d = decompile(&ctg, &Options { input_name: "cycle.ctg".into(), ..Default::default() })
        .expect("decompile");
    assert!(d.text.contains("fn prog_cycle()"), "program name recovered:\n{}", d.text);
    assert!(d.text.contains("scan();"));
    let c2 = compile(&d.text).expect("recompile");
    assert_eq!(
        payload_of(&c2, OutFormat::Ctg).unwrap(),
        payload_of(&c, OutFormat::Ctg).unwrap(),
        "payload identical after decompile→recompile"
    );
}
