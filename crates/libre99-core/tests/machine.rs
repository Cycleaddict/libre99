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

//! Bus multiplexer conformance: the VDP, GROM, and sound chips sit on the **high**
//! byte of the 8-bit multiplexed bus and answer only at even addresses, so a
//! *word* access reaches each chip exactly **once** — the odd half is discarded.
//! This is why TI software drives these ports with byte instructions; getting it
//! wrong double-strobes the chip (sound) or double-advances the GROM address
//! counter (GROM), corrupting whatever the port drives. Mirrors Classic99, which
//! `return`s on `x & 1` for every device port.

use std::sync::LazyLock;

use libre99_core::bus::Bus;
use libre99_core::machine::{
    Machine, StateAccess, StateAccessFilter, StateFilter, StateSpace, Tms9900Bus,
};

static CONSOLE_ROM: LazyLock<Option<Vec<u8>>> =
    LazyLock::new(|| libre99_core::third_party::load("roms/994aROM.Bin"));
static CONSOLE_GROM: LazyLock<Option<Vec<u8>>> =
    LazyLock::new(|| libre99_core::third_party::load("roms/994AGROM.Bin"));

/// A *word* read of the GROM data port advances the address counter by **one**,
/// not two: the odd half of the access is open bus and performs no GROM read.
#[test]
fn word_read_of_grom_data_port_advances_counter_once() {
    let mut bus = Tms9900Bus::new(&[], &[]);
    bus.grom.load(0x0100, &[0x11, 0x22, 0x33, 0x44]);
    // Point the counter at >0100 the way software does: two byte writes.
    bus.write_byte(0x9C02, 0x01);
    bus.write_byte(0x9C02, 0x00);
    let before = bus.grom_address();
    let _ = bus.read_word(0x9800);
    assert_eq!(
        bus.grom_address().wrapping_sub(before),
        1,
        "a word read must auto-increment the GROM counter once, not twice"
    );
}

/// A single *word* write to the GROM address port latches only the high byte, so
/// it cannot stand in for the two byte writes that load a full 16-bit address.
#[test]
fn word_write_to_grom_address_is_not_two_byte_writes() {
    let mut by_bytes = Tms9900Bus::new(&[], &[]);
    by_bytes.write_byte(0x9C02, 0x12);
    by_bytes.write_byte(0x9C02, 0x34);

    let mut by_word = Tms9900Bus::new(&[], &[]);
    by_word.write_word(0x9C02, 0x1234);

    // The byte path completed a 16-bit address load (and prefetched); the word
    // path latched only the high byte. The counters must therefore differ — before
    // the odd-half guard they were identical (the bug).
    assert_ne!(by_bytes.grom_address(), by_word.grom_address());
}

/// A *word* write to the sound port strobes the SN76489 only once (the high
/// byte); the odd half is ignored, so it cannot smuggle a second (data) byte in.
#[test]
fn word_write_to_sound_port_reaches_psg_once() {
    let mut bus = Tms9900Bus::new(&[], &[]);
    // Latch tone channel 0 with period low-nibble 0 (>80 = `1 00 0 0000`).
    bus.write_byte(0x8400, 0x80);
    assert_eq!(bus.psg.period(0), 0);
    // Word write: the high byte >80 re-latches ch0 tone; the odd-half low byte >3F
    // is a data byte that, if it reached the chip, would set period bits 4..9.
    bus.write_word(0x8400, 0x803F);
    assert_eq!(
        bus.psg.period(0),
        0,
        "the odd half of a word write must not reach the sound chip"
    );
}

/// The hardware decodes 12 CRU address lines, so software bit addresses
/// above >0FFF alias back into the 4096-bit space — a write to bit >1012
/// lands on bit >012 (a keyboard column-select pin), and a read of
/// bit >1008 samples bit >008 (a keyboard row).
#[test]
fn cru_bit_addresses_alias_into_the_12_bit_space() {
    use libre99_core::keyboard::TiKey;
    let mut bus = Tms9900Bus::new(&[], &[]);
    bus.keyboard.set_key(TiKey::A, true); // matrix cell (column 5, row 5)

    // Select column 5 (0b101 on P2..P4 = bits 18..20) via ALIASED addresses.
    bus.write_cru_bit(0x1012, true);
    bus.write_cru_bit(0x1013, false);
    bus.write_cru_bit(0x1014, true);

    // Row 5 reads on bit 3+5 = 8, active low — through the alias and directly.
    assert!(!bus.read_cru_bit(0x1008), "aliased row read sees the key");
    assert!(!bus.read_cru_bit(0x0008), "canonical row read agrees");
}

/// GROM accesses stall the CPU far beyond the multiplexer's 4 cycles —
/// Classic99's hardware-measured values, stacked on the mux wait exactly as
/// Classic99 stacks them. The second address byte costs more than the first
/// (it completes the address and triggers the prefetch), so the cost model
/// tracks the GROM's write-latch phase.
#[test]
fn grom_port_accesses_stall_beyond_the_multiplexer() {
    let mut bus = Tms9900Bus::new(&[], &[]);
    assert_eq!(bus.wait_states_rw(0x9800, false), 23, "data read: 4 + 19");
    assert_eq!(
        bus.wait_states_rw(0x9802, false),
        17,
        "address read: 4 + 13"
    );
    assert_eq!(bus.wait_states_rw(0x9C00, true), 26, "data write: 4 + 22");
    assert_eq!(
        bus.wait_states_rw(0x9C02, true),
        19,
        "first address byte: 4 + 15"
    );
    bus.write_byte(0x9C02, 0x12);
    assert_eq!(
        bus.wait_states_rw(0x9C02, true),
        25,
        "second address byte: 4 + 21"
    );
    bus.write_byte(0x9C02, 0x34);
    assert_eq!(
        bus.wait_states_rw(0x9C02, true),
        19,
        "phase resets after a full address"
    );
    // The odd half of a word access is open bus; everything else is unchanged.
    assert_eq!(bus.wait_states_rw(0x9801, false), 4);
    assert_eq!(
        bus.wait_states_rw(0x8C00, true),
        4,
        "VDP keeps the plain mux wait"
    );
    assert_eq!(bus.wait_states_rw(0x0000, false), 0, "console ROM is fast");
    assert_eq!(bus.wait_states_rw(0x8300, false), 0, "scratchpad is fast");
}

/// The stall reaches instruction timing through the CPU: a GROM data-port
/// read inside MOVB costs 14 (base) + 8 (symbolic operand) + 23 (port) = 45.
#[test]
fn a_grom_data_read_charges_the_stall_through_the_cpu() {
    use libre99_core::cpu::Cpu;
    let mut bus = Tms9900Bus::new(&[], &[]);
    bus.poke_word(0x8320, 0xD060); // MOVB @>9800,R1 — program in scratchpad
    bus.poke_word(0x8322, 0x9800);
    let mut cpu = Cpu::new();
    cpu.set_wp(0x8300);
    cpu.set_pc(0x8320);
    assert_eq!(cpu.step(&mut bus), 45);
}

// A *word* write to the VDP data port must land a single byte and advance the
// VRAM address only once. The 9918A hangs off the high byte of the console's data
// bus, so the odd half of a word access never reaches the chip — the bus drops
// it. (When the bus instead wrote both halves, every word write double-advanced
// the address; the disk DSR's power-up `CLR @>8C00` VRAM-clear loop then ran off
// the end of VRAM and wrapped its zeros back over the master title screen.) This
// behavior lives in the console bus, not the bare `Vdp`, so the test drives a
// `Machine` (which is why it lives here and not in the VDP unit tests).
#[test]
fn word_write_to_vdp_data_port_lands_one_byte() {
    let (Some(rom), Some(grom)) = (CONSOLE_ROM.as_deref(), CONSOLE_GROM.as_deref()) else {
        eprintln!("SKIPPED: third-party media not present");
        return;
    };
    let mut m = Machine::new(rom, grom);
    // Point the VRAM write address at >0100 (low byte, then high byte | >40).
    m.bus_mut().write_byte(0x8C02, 0x00);
    m.bus_mut().write_byte(0x8C02, 0x41);

    // One word write of >ABCD: only the high byte (>AB) is latched.
    m.bus_mut().write_word(0x8C00, 0xABCD);
    assert_eq!(
        m.vdp().vram(0x0100),
        0xAB,
        "word write latches the high byte"
    );
    assert_eq!(
        m.vdp().vram(0x0101),
        0x00,
        "the low half of the word must not be written to VRAM"
    );

    // The address advanced exactly once: the next byte write goes to >0101.
    m.bus_mut().write_byte(0x8C00, 0xEE);
    assert_eq!(
        m.vdp().vram(0x0101),
        0xEE,
        "a word access advances the VRAM address by one, not two"
    );
}

/// The observatory's VDP-write channel records semantic data-port writes with
/// their causing instruction, while remaining absent from machine state. This
/// drives real TMS9900 instructions rather than poking the bus directly.
#[test]
fn vdp_write_provenance_is_ordered_filtered_and_observational() {
    let mut prepared = Machine::new(&[], &[]);
    prepared.cpu_mut().set_wp(0x8300);
    prepared.cpu_mut().set_pc(0x8320);
    prepared.bus_mut().poke_word(0x8302, 0xAB00); // R1 byte value >AB
    prepared.bus_mut().poke_word(0x8304, 0xCD00); // R2 byte value >CD
    prepared.bus_mut().poke_word(0x8306, 0xEF12); // R3 word value >EF12
    prepared.bus_mut().poke_word(0x8312, 0xA500); // R9 GPL-opcode breadcrumb
    prepared.bus_mut().poke_word(0x8316, 0x9000); // R11 link breadcrumb
    prepared.bus_mut().poke_word(0x8320, 0xD801); // MOVB R1,@>8C00
    prepared.bus_mut().poke_word(0x8322, 0x8C00);
    prepared.bus_mut().poke_word(0x8324, 0xD802); // MOVB R2,@>8C00
    prepared.bus_mut().poke_word(0x8326, 0x8C00);
    prepared.bus_mut().poke_word(0x8328, 0xC803); // MOV  R3,@>8C00
    prepared.bus_mut().poke_word(0x832A, 0x8C00);
    prepared.vdp_mut().set_vram(0x0100, 0x11);
    prepared.vdp_mut().set_vram(0x0101, 0x22);
    prepared.vdp_mut().set_vram(0x0102, 0x33);
    prepared.vdp_mut().set_vram(0x0103, 0x44);
    prepared.bus_mut().write_byte(0x8C02, 0x00);
    prepared.bus_mut().write_byte(0x8C02, 0x41); // write address >0100
    let checkpoint = prepared.save_state();

    let mut control = Machine::new(&[], &[]);
    control.load_state(&checkpoint).unwrap();
    let mut observed = Machine::new(&[], &[]);
    observed.load_state(&checkpoint).unwrap();
    assert!(observed.bus().vdp_write_filter().is_none());
    assert!(observed.bus().vdp_write_log().is_empty());
    observed.bus_mut().record_vdp_writes(0x0100, 0x0102);

    for _ in 0..3 {
        assert_eq!(control.step(), observed.step());
        assert_eq!(
            control.save_state(),
            observed.save_state(),
            "recording must not change any intermediate machine state"
        );
    }

    let log = observed.bus().vdp_write_log();
    assert_eq!(log.len(), 3, "two MOVB writes plus one high byte from MOV");
    assert_eq!((log[0].pc, log[0].opcode), (0x8320, 0xD801));
    assert_eq!((log[1].pc, log[1].opcode), (0x8324, 0xD802));
    assert_eq!((log[2].pc, log[2].opcode), (0x8328, 0xC803));
    assert!(log.iter().all(|event| event.r11 == 0x9000));
    assert!(log.iter().all(|event| event.r9 == 0xA500));
    // Empty GROM: the prefetch-corrected next-data address is stable and
    // observational (it must not differ across the recorded writes).
    let grom = log[0].grom;
    assert!(log.iter().all(|event| event.grom == grom));
    assert_eq!(
        log.iter()
            .map(|e| (e.port, e.address, e.old_value, e.value))
            .collect::<Vec<_>>(),
        vec![
            (0x8C00, 0x0100, 0x11, 0xAB),
            (0x8C00, 0x0101, 0x22, 0xCD),
            (0x8C00, 0x0102, 0x33, 0xEF),
        ]
    );
    assert!(log.windows(2).all(|pair| pair[0].cycle < pair[1].cycle));
    assert_eq!(
        observed.vdp().vram(0x0103),
        0x44,
        "odd word byte is not latched"
    );

    // Diagnostic recording is absent from the serialized machine and cannot
    // perturb execution or presentation.
    assert_eq!(control.save_state(), observed.save_state());
    let mut control_frame = vec![0; 256 * 192];
    let mut observed_frame = vec![0; 256 * 192];
    control.render(&mut control_frame);
    observed.render(&mut observed_frame);
    assert_eq!(control_frame, observed_frame);

    // A fresh filtered run retains only the matching semantic destination.
    let mut filtered = Machine::new(&[], &[]);
    filtered.load_state(&checkpoint).unwrap();
    filtered.bus_mut().record_vdp_writes(0x0101, 0x0101);
    for _ in 0..3 {
        filtered.step();
    }
    assert_eq!(filtered.bus().vdp_write_log().len(), 1);
    assert_eq!(filtered.bus().vdp_write_log()[0].address, 0x0101);
}

/// The VDP recorder keeps the GROM stream position (prefetch-corrected next
/// byte) with each write so GPL activity can be attributed without a CPU trace.
#[test]
fn vdp_write_provenance_retains_grom_stream_position() {
    let mut m = Machine::new(&[], &[]);
    m.bus_mut().grom.load(0x6010, &[0xBE]);
    m.cpu_mut().set_wp(0x8300);
    m.cpu_mut().set_pc(0x8320);
    m.bus_mut().poke_word(0x8302, 0xAB00); // R1
    m.bus_mut().poke_word(0x8320, 0xD801); // MOVB R1,@>8C00
    m.bus_mut().poke_word(0x8322, 0x8C00);
    m.bus_mut().write_byte(0x9C02, 0x60);
    m.bus_mut().write_byte(0x9C02, 0x10); // GROM >6010 + prefetch
    m.bus_mut().write_byte(0x8C02, 0x00);
    m.bus_mut().write_byte(0x8C02, 0x40); // VRAM write address >0000
    assert_eq!(m.bus().grom.next_data_address(), 0x6010);
    assert_eq!(m.bus().grom.next_data_byte(), 0xBE);
    m.bus_mut().record_vdp_writes(0x0000, 0x0000);
    m.step();
    let log = m.bus().vdp_write_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].grom, 0x6010);
    assert_eq!(log[0].grom_byte, 0xBE);
}

#[test]
fn filtered_state_provenance_attributes_cpu_vram_and_grom_without_perturbation() {
    let mut prepared = Machine::new(&[], &[]);
    prepared.cpu_mut().set_wp(0x8300);
    prepared.cpu_mut().set_pc(0x8320);
    prepared.bus_mut().grom.load(0x6010, &[0xBE]);
    prepared.bus_mut().poke(0x2000, 0xA5);
    prepared.bus_mut().poke(0x2001, 0xB6);
    prepared.vdp_mut().set_vram(0x1234, 0x5A);
    prepared.bus_mut().write_byte(0x9C02, 0x60);
    prepared.bus_mut().write_byte(0x9C02, 0x10);
    prepared.bus_mut().write_byte(0x8C02, 0x34);
    prepared.bus_mut().write_byte(0x8C02, 0x12); // read setup >1234
    prepared.bus_mut().poke_word(0x8320, 0xD060); // MOVB @>9800,R1
    prepared.bus_mut().poke_word(0x8322, 0x9800);
    prepared.bus_mut().poke_word(0x8324, 0xD0A0); // MOVB @>8800,R2
    prepared.bus_mut().poke_word(0x8326, 0x8800);
    prepared.bus_mut().poke_word(0x8328, 0xD0E0); // MOVB @>2000,R3
    prepared.bus_mut().poke_word(0x832A, 0x2000);
    prepared.bus_mut().poke_word(0x832C, 0xD803); // MOVB R3,@>2001
    prepared.bus_mut().poke_word(0x832E, 0x2001);
    prepared.bus_mut().poke_word(0x8330, 0xD803); // MOVB R3,@>8C00
    prepared.bus_mut().poke_word(0x8332, 0x8C00);
    let checkpoint = prepared.save_state();

    let mut control = Machine::new(&[], &[]);
    control.load_state(&checkpoint).unwrap();
    let mut observed = Machine::new(&[], &[]);
    observed.load_state(&checkpoint).unwrap();
    observed.bus_mut().record_state_accesses(vec![
        StateFilter {
            space: StateSpace::Cpu,
            access: StateAccessFilter::ReadWrite,
            start: 0x2000,
            end: 0x2001,
        },
        StateFilter {
            space: StateSpace::Cpu,
            access: StateAccessFilter::ReadWrite,
            start: 0x8306,
            end: 0x8307,
        },
        StateFilter {
            space: StateSpace::Vram,
            access: StateAccessFilter::ReadWrite,
            start: 0x1234,
            end: 0x1236,
        },
        StateFilter {
            space: StateSpace::Grom,
            access: StateAccessFilter::Read,
            start: 0x6010,
            end: 0x6010,
        },
    ]);

    for _ in 0..5 {
        assert_eq!(control.step(), observed.step());
        assert_eq!(control.save_state(), observed.save_state());
    }

    let log = observed.bus().state_access_log();
    assert!(log.iter().any(|event| {
        (
            event.space,
            event.access,
            event.address,
            event.port,
            event.value,
        ) == (
            StateSpace::Grom,
            StateAccess::Read,
            0x6010,
            Some(0x9800),
            0xBE,
        ) && (event.pc, event.opcode) == (0x8320, 0xD060)
    }));
    assert!(log.iter().any(|event| {
        (
            event.space,
            event.access,
            event.address,
            event.port,
            event.value,
        ) == (
            StateSpace::Vram,
            StateAccess::Read,
            0x1234,
            Some(0x8800),
            0x5A,
        ) && (event.pc, event.opcode) == (0x8324, 0xD0A0)
    }));
    assert!(log.iter().any(|event| {
        (
            event.space,
            event.access,
            event.address,
            event.old_value,
            event.value,
        ) == (
            StateSpace::Cpu,
            StateAccess::Write,
            0x2001,
            Some(0xB6),
            0xA5,
        ) && (event.pc, event.opcode) == (0x832C, 0xD803)
    }));
    assert!(log.iter().any(|event| {
        (
            event.space,
            event.access,
            event.address,
            event.old_value,
            event.value,
        ) == (
            StateSpace::Vram,
            StateAccess::Write,
            0x1236,
            Some(0x00),
            0xA5,
        ) && (event.pc, event.opcode) == (0x8330, 0xD803)
    }));
    assert!(log.iter().any(|event| event.address == 0x8306));
    assert!(log.windows(2).all(|pair| pair[0].cycle <= pair[1].cycle));

    let state = observed.save_state();
    observed.load_state(&state).unwrap();
    assert!(observed.bus().state_access_filters().is_none());
    assert!(observed.bus().state_access_log().is_empty());
}

#[test]
fn interrupt_entry_is_not_misattributed_to_the_interrupted_instruction() {
    let mut rom = vec![0; 0x2000];
    rom[4..8].copy_from_slice(&[0x83, 0x80, 0x10, 0x00]); // level-1 vector
    let mut m = Machine::new(&rom, &[]);
    m.cpu_mut().set_wp(0x8300);
    m.cpu_mut().set_pc(0x8320);
    m.cpu_mut().set_st(1); // accept level 1
    m.bus_mut().write_byte(0x8C02, 0x20);
    m.bus_mut().write_byte(0x8C02, 0x81); // VDP interrupts enabled
    m.bus_mut().write_cru_bit(2, true); // 9901 /INT2 mask enabled
    m.vdp_mut().vblank();
    m.bus_mut().record_state_accesses(vec![StateFilter {
        space: StateSpace::Cpu,
        access: StateAccessFilter::Write,
        start: 0x839A,
        end: 0x839F,
    }]);

    m.step();
    assert_eq!(m.cpu().pc(), 0x1000);
    assert!(
        m.bus().state_access_log().is_empty(),
        "interrupt context writes have no causing instruction PC/opcode"
    );
}
