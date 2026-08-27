# Observatory MVP

## Mission

Create a practical research instrument that lets a human and an AI observe a
running vintage program, connect effects back to the responsible code and
state, and produce an evidence-labeled reconstruction that can be tested.

The long-term value is enabling more faithful explanations, restorations, and
enhanced ports of software whose mechanics are difficult to recover with
static disassembly alone. Parsec and Tunnels of Doom are demonstrations, not
hard-coded product boundaries.

## Foundation we are keeping

Libre99's `gsl` branch already provides:

- a playable TI-99/4A implementation with CPU, VDP, GROM, TMS9901, sound,
  cartridge, disk, keyboard, save-state, and clean-room firmware support;
- a TMS9900 assembler/disassembler and GPL assembler/disassembler;
- GSL decompile, edit, compile, and byte-identical roundtrip verification;
- a deterministic headless probe with scripted input, screen/state/memory
  inspection, screenshots, GROM fetch traces, CPU/GROM coverage, and
  checkpoints; and
- an annotation workflow that labels observations and preserves byte identity.

Those are production foundations. We extend them; we do not rebuild them in a
generic composition framework.

They are not automatically authoritative. Before the Parsec POC, Libre99 must
pass the small prequalification gate in `FOUNDATION-AUDIT.md`. During the POC,
every software-visible CPU, VDP, GROM, TMS9901, console-routing, timing, or
keyboard behavior used by the investigation is compared as it is reached.
This preserves the useful independent work from the prior repository without
requiring a complete chip audit before the product hypothesis is tested.

## Chip ownership

Libre99 already has suitable ownership boundaries inside `libre99-core`:
`cpu.rs`, `vdp.rs`, `grom.rs`, `cru.rs` (TMS9901), `psg.rs`, `disk.rs`, and
`keyboard.rs`; `machine.rs` owns console wiring and address/CRU routing. The CPU
reaches the machine through the small `Bus` trait and can be tested against
flat memory.

Keep those boundaries. A chip defect is repaired and regression-tested in its
own module; a console mapping or route defect belongs in `machine.rs`. Do not
split every module into a crate, import the old chip implementations, or
perform a general cleanup before the POC. Revisit physical crate boundaries
only if a real change cannot be isolated or a second platform needs to reuse a
pure semantic component.

## Architecture

```text
authentic software
       |
Libre99 execution core ---------------- display / keyboard / sound
       |
optional, filtered event tap
       |
compact trace + checkpoints + recorded inputs
       |
offline index and queries
       |
AI/human reconstruction tools
```

The emulator executes. The recorder observes. Analysis stays outside the hot
loop. Observation must not change machine behavior, and play mode must not pay
for forensic detail it did not request.

## MVP deliverables

| Deliverable | Existing base | Smallest missing result | Pass evidence |
|---|---|---|---|
| Reproducible runtime | Emulator and probe | Verify authentic Parsec can be booted, controlled, checkpointed, and replayed from owner-local media | One deterministic session script and matching checkpoints/screens outside Git |
| Reliable static recovery | GSL verified decompiler | Fix or bound `F-001`: inline-argument calls and trace-discovered entries must not silently misdecode executed GPL | Focused synthetic regressions plus byte-identical roundtrip of the investigated media |
| Causal observation | GROM fetch trace, coverage, memory peeks | For a selected interval, associate executed CPU/GPL operations with relevant memory and VDP/GROM effects; support address/event filters and triggers | A replayable capture answers which code produced a chosen visible or state change |
| Searchable evidence | Text traces and save states | Export compact trace data and enough indexing/query support for the benchmark questions; SQLite is acceptable but not mandatory if a simpler format answers them | Queries reproduce the same causal slice without rereading an uncontrolled live stream |
| Reconstruction report | `/annotate` workflow | Produce evidence-labeled assembly/GSL/pseudocode and state layout for one bounded subsystem | Claims link to source, observation, corroboration, or inference and survive held-out executions |

## Product demonstrations

1. **Baseline:** Libre99 runs interactively and through the probe with the
   owner's authentic media.
2. **Parsec:** identify and explain one bounded scrolling or rendering update,
   including the responsible code, state, and VDP effects. Validate the result
   against held-out evidence as defined in `BENCHMARK.md`.
3. **Tunnels of Doom:** generalize the method to one bounded mechanical
   subsystem, such as a map representation/transformation step. Do not claim
   recovery of the complete game engine.

Demo 2 proves the observatory concept. Demo 3 tests whether it transfers to a
large hybrid GPL/native, data-heavy program.

## Explicit non-goals for the MVP

- Reconstructing perfect original source or original comments.
- Automatically decompiling arbitrary games on arbitrary platforms.
- Bit- or cycle-perfect behavior that no authentic software, trace, visual,
  audio, input, or saved state can observe.
- Media-sector provenance, archival disk reconstruction, or redistribution of
  commercial media.
- Proactive modeling of every undocumented hardware quirk.
- A generalized distributed event service or a cross-platform hardware core.
- Hostile-user security, signed journals, per-event hashes/schema validation,
  compliance controls, or years-long uptime.
- A live AI inside the emulator's execution loop.

## Evidence and implementation policy

Every substantive conclusion carries one label:

- `source-confirmed`: primary documentation or original source establishes it.
- `observed`: a replayable run establishes it.
- `corroborated`: independent implementations agree, without primary proof.
- `inferred`: it best explains the evidence but remains unproven.
- `unresolved`: competing explanations remain.

A working compatibility choice may be used when primary evidence is absent,
provided the label and regression remain visible. Real software discoveries
create bounded research items; they do not trigger proactive quirk hunts.

## Long-term direction

If the TI MVP produces a material evidence gain:

- deepen data-flow, call-graph, GPL/native transition, VDP, CRU, GROM, disk,
  and input queries as real investigations demand;
- reconstruct additional subsystems into testable, maintainable descriptions;
- support enhanced recreations and ports that preserve mechanics while using
  the target machine's strengths, rather than demanding bit-identical ports;
  and
- add Atari 8-bit and C64 observatory adapters around mature emulators.

Future platforms should reuse trace concepts, evidence labels, query formats,
and evaluation methods. They should not be forced through a genericized TI
hardware model.

## Decision rule

Before adding a feature, answer:

> Does this help run authentic software, capture a reproducible behavior, or
> explain/reconstruct that behavior?

If the answer is no, it is outside the MVP.
