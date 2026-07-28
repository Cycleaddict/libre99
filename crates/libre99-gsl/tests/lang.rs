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

//! GSL language semantics: statement encodings (against the execution-pinned
//! byte patterns from `libre99-gpl`'s own tests), the canonical-spelling
//! rules of docs/GSL.md §6.1, diagnostics, and container output.

use libre99_gsl::compile;

fn image(src: &str) -> Vec<u8> {
    compile(src).unwrap_or_else(|e| panic!("compile failed: {e:?}", e = e)).image
}

fn at(img: &[u8], addr: usize, n: usize) -> &[u8] {
    &img[addr..addr + n]
}

#[test]
fn store_immediates_match_the_assembler() {
    // ST @>8400,>9F = BE 81 00 9F; DST @>8372,>FF7E = BF 72 FF 7E
    // (same golden bytes as libre99-gpl's asm.rs tests).
    let img = image(
        "origin 0x0000;
         var w: word @ cpu[0x8372];
         fn f() { cpu[0x8400] = 0x9F; w = 0xFF7E; }",
    );
    assert_eq!(at(&img, 0, 4), [0xBE, 0x81, 0x00, 0x9F]);
    assert_eq!(at(&img, 4, 4), [0xBF, 0x72, 0xFF, 0x7E]);
}

#[test]
fn bare_zero_is_clr_and_cz_spelled_zero_is_st_imm() {
    let img = image(
        "origin 0x0000;
         var b: byte @ cpu[0x8350];
         var w: word @ cpu[0x8340];
         fn f() {
             b = 0;      // CLR  @>8350      = 86 50
             w = 0;      // DCLR @>8340      = 87 40
             b = 0x00;   // ST   @>8350,>00  = BE 50 00
         l:
             if (b == 0) goto l;   // CZ @>8350 / BS = 8E 50 / 60 xx
         }",
    );
    assert_eq!(at(&img, 0, 2), [0x86, 0x50]);
    assert_eq!(at(&img, 2, 2), [0x87, 0x40]);
    assert_eq!(at(&img, 4, 3), [0xBE, 0x50, 0x00]);
    assert_eq!(at(&img, 7, 2), [0x8E, 0x50]);
    assert_eq!(img[9], 0x60); // BS, slot bits 0
    assert_eq!(img[10], 0x07); // back to the CZ
}

#[test]
fn inc_and_add_one_are_different_instructions() {
    let img = image(
        "origin 0x0000;
         var b: byte @ cpu[0x8350];
         fn f() { b++; b += 1; inct(cpu[0x8360]); }",
    );
    assert_eq!(at(&img, 0, 2), [0x90, 0x50]); // INC
    assert_eq!(at(&img, 2, 3), [0xA2, 0x50, 0x01]); // ADD imm 1
    assert_eq!(at(&img, 5, 2), [0x94, 0x60]); // INCT
}

#[test]
fn fused_compare_polarity() {
    let img = image(
        "origin 0x0000;
         var k: byte @ cpu[0x8375];
         fn f() {
         top:
             if (k == 0xFF) goto top;   // CEQ >FF then BS
             if (k != 0xFF) goto top;   // CEQ >FF then BR
             if (k h> 0x20) goto top;   // CH then BS
             if (k < 0x20) goto top;    // CGE then BR
             if ((k & 0x80) != 0) goto top; // CLOG then BR
         }",
    );
    assert_eq!(at(&img, 0, 3), [0xD6, 0x75, 0xFF]); // CEQ imm
    assert_eq!(img[3], 0x60); // BS
    assert_eq!(at(&img, 5, 3), [0xD6, 0x75, 0xFF]);
    assert_eq!(img[8], 0x40); // BR
    assert_eq!(at(&img, 10, 3), [0xC6, 0x75, 0x20]); // CH imm
    assert_eq!(img[13], 0x60);
    assert_eq!(at(&img, 15, 3), [0xD2, 0x75, 0x20]); // CGE imm
    assert_eq!(img[18], 0x40);
    assert_eq!(at(&img, 20, 3), [0xDA, 0x75, 0x80]); // CLOG imm
    assert_eq!(img[23], 0x40);
}

#[test]
fn moves_match_execution_pinned_encodings() {
    // MOVE 4,G@>0100,V@>0000 = 31 00 04 A0 00 01 00 and the boot-trace
    // register form 39 00 08 00 04 51 (libre99-gpl asm.rs golden tests).
    let img = image(
        "origin 0x0000;
         fn f() {
             move(vdp[0x0000], grom[0x0100], 4);
             move(vreg(0), grom[0x0451], 8);
         }",
    );
    assert_eq!(at(&img, 0, 7), [0x31, 0x00, 0x04, 0xA0, 0x00, 0x01, 0x00]);
    assert_eq!(at(&img, 7, 6), [0x39, 0x00, 0x08, 0x00, 0x04, 0x51]);
}

#[test]
fn indexed_places_take_the_byte_path() {
    // ST with an indexed destination: opcode BE, GAS = C1 00 + index byte E0,
    // then the immediate. The assembler grammar rejects indexed operands; the
    // GSL compiler must encode them itself, identically to decode_gas.
    let img = image(
        "origin 0x0000;
         var t: byte @ cpu[0x8400];
         var ix: byte @ cpu[0x83E0];
         fn f() { t(ix) = 0x01; }",
    );
    assert_eq!(at(&img, 0, 5), [0xBE, 0xC1, 0x00, 0xE0, 0x01]);
    let (op, len) = libre99_gpl::operand::decode_gas(&img, 1).unwrap();
    assert_eq!(len, 3);
    assert_eq!(
        op,
        libre99_gpl::operand::Operand::Cpu { addr: 0x8400, indirect: false, index: Some(0xE0) }
    );
}

#[test]
fn vdp_indirect_and_word_casts() {
    // *V@>8356 encodes B0 56 (m4_probe candidate A); a word op through an
    // uncast pointer needs word(…).
    let img = image(
        "origin 0x0000;
         var p: word @ cpu[0x8356];
         fn f() {
             vdp[*p] = 0x17;          // ST *V@>8356,>17 = BE B0 56 17
             word(*p) = 0x0080;       // DST *@>8356,>0080 = BF 90 56 00 80
         }",
    );
    assert_eq!(at(&img, 0, 4), [0xBE, 0xB0, 0x56, 0x17]);
    assert_eq!(at(&img, 4, 5), [0xBF, 0x90, 0x56, 0x00, 0x80]);
}

#[test]
fn asm_blocks_share_symbols_with_gsl() {
    let img = image(
        "origin 0x0020;
         var snd: word @ cpu[0x83CC];
         fn f() {
             snd = tone;      // symbol from the data block below
             goto done;
         }
         asm {
done    RTN
         }
         data tone { 0x01, 0x9F, 0x00, }",
    );
    // DST @>83CC,tone — cell offset >CC needs the 12-bit GAS form (80 CC),
    // then the 16-bit immediate holding tone's address.
    assert_eq!(at(&img, 0x20, 3), [0xBF, 0x80, 0xCC]);
    let tone = ((img[0x23] as u16) << 8) | img[0x24] as u16;
    assert_eq!(tone, 0x0029, "DST(5) + B(3) + RTN(1) from >0020");
    assert_eq!(img[tone as usize], 0x01, "tone points at the data block");
    assert_eq!(img[0x25], 0x05, "B done");
    assert_eq!(img[tone as usize - 1], 0x00, "RTN before the data");
}

#[test]
fn while_and_if_sugar_compile() {
    let img = image(
        "origin 0x0000;
         var k: byte @ cpu[0x8375];
         fn f() {
             while (k == 0xFF) { scan(); }
             if (k == 0x0D) { k = 0; } else { k--; }
             return;
         }",
    );
    // while: CEQ; BR end; SCAN; B top.
    assert_eq!(at(&img, 0, 3), [0xD6, 0x75, 0xFF]);
    assert_eq!(img[3], 0x40, "exit branch is BR (branch when compare fails)");
    assert_eq!(img[5], 0x03, "SCAN body");
    assert_eq!(img[6], 0x05, "B back to top");
}

#[test]
fn pin_overlap_is_an_error_and_gaps_are_zero_filled() {
    let err = compile(
        "origin 0x0000;
         fn a() { scan(); scan(); }
         fn b() @ 0x0001 { return; }",
    )
    .unwrap_err();
    assert!(err[0].message.contains("overlaps"), "got {err:?}");

    let img = image("origin 0x0000;\nfn a() @ 0x0010 { return; }");
    assert!(img[..0x10].iter().all(|&b| b == 0), "gap zero-filled");
    assert_eq!(img[0x10], 0x00); // RTN
}

#[test]
fn diagnostics_carry_gsl_lines() {
    let src = "origin 0x0000;\nvar b: byte @ cpu[0x8350];\nvar w: word @ cpu[0x8340];\nfn f() {\n    b = w;\n}\n";
    let err = compile(src).unwrap_err();
    assert_eq!(err[0].line, 5, "mixed-width error points at the statement: {err:?}");
    assert!(err[0].message.contains("mixed byte/word"));
}

#[test]
fn duplicate_and_reserved_names_are_rejected() {
    assert!(compile("const x = 1; const x = 2;").is_err());
    assert!(compile("var move: byte @ cpu[0x8300];").is_err());
}

#[test]
fn ctg_output_round_trips_through_the_container() {
    let c = compile(
        "format ctg;
         cartridge \"DEMO\";
         cru 0x0000;
         origin 0x6000;
         data hdr { 0xAA, 0x01, 0x01, 0x00, word 0, word 0x6010, word 0, word 0,
                    word 0, 0x00, 0x00, word 0, word 0x601B, 0x04, \"DEMO\", }
         fn main() @ 0x601B { exit(); }
         rom 0 { 0xAA, 0x01, }",
    )
    .unwrap();
    let bytes = libre99_gsl::write_output(&c, libre99_gsl::OutFormat::Ctg).unwrap();
    let parsed = libre99_core::cartridge::Cartridge::parse(&bytes).unwrap();
    assert_eq!(parsed.title, "DEMO");
    assert_eq!(parsed.rom_banks, 1);
    assert_eq!(parsed.rom[0], 0xAA);
    assert_eq!(parsed.grom.len(), 1);
    assert_eq!(parsed.grom[0].0, 0x6000);
    assert_eq!(parsed.grom[0].1[0], 0xAA);
    assert_eq!(parsed.grom[0].1[0x1B], 0x0B, "EXIT at main");
}

#[test]
fn grom24_output_matches_the_system_image_shape() {
    let c = compile("format grom24;\norigin 0x0000;\nfn f() @ 0x0020 { exit(); }").unwrap();
    let bytes = libre99_gsl::write_output(&c, libre99_gsl::OutFormat::Grom24).unwrap();
    assert_eq!(bytes.len(), libre99_gpl::GROM_IMAGE_LEN);
    assert_eq!(bytes[0x20], 0x0B);
}

#[test]
fn fmt_blocks_encode_the_screen_sublanguage() {
    // fmt { col(13); row(3); htext("AB"); hchar(3, ' '); repeat (2) { hmove(4); } }
    // 08 | FF 0D | FE 03 | 01 41 42 | 42 20 | C1 | 83 | FB 00 3B | FB
    // (the repeat loop-back word points at the body, >003B).
    let img = image(
        "origin 0x0000;
         fn f() @ 0x0030 {
             fmt {
                 col(13); row(3);
                 htext(\"AB\");
                 hchar(3, ' ');
                 repeat (2) { hmove(4); }
             }
         }",
    );
    assert_eq!(
        at(&img, 0x30, 16),
        [
            0x08, 0xFF, 0x0D, 0xFE, 0x03, 0x01, 0x41, 0x42, 0x42, 0x20, 0xC1, 0x83, 0xFB, 0x00,
            0x3B, 0xFB
        ]
    );
}

#[test]
fn fmt_gas_forms_encode_via_operands() {
    // bias(imm) is FC; bias(var) / hstr(n, var) carry GAS operands
    // (short direct CPU form: >8350 -> >50).
    let img = image(
        "origin 0x0000;
         var bv: byte @ cpu[0x8350];
         fn f() @ 0x0030 {
             fmt { bias(0x60); bias(bv); hstr(2, bv); }
         }",
    );
    assert_eq!(at(&img, 0x30, 8), [0x08, 0xFC, 0x60, 0xFD, 0x50, 0xE1, 0x50, 0xFB]);
}
