# Tunnels of Doom dungeon-generator reconnaissance

## Disposition

**PASS for bounded reconnaissance; NO-GO for full generator reconstruction as
one immediate package.** Authentic execution located the generation boundary,
the candidate byte payload, its mutating GPL/native operations, the controlled
random source, and one repeatable multi-phase property. That makes a future
neutral reconstruction of the per-floor candidate-buffer kernel plausible. It does
not yet bound the later room contents, monster/item stocking, inter-floor data,
or the meanings of every tile value tightly enough to make a complete dungeon
generator economical in one step.

This work did not change the emulator, GSL, probe, campaign runner, or stairs
model. It used the existing filtered recorders, coverage, a short GPL fetch
breadcrumb, and R1 trace-entry recovery. Commercial media, states, traces, and
the byte-verified decompilation remain owner-local.

## Declared experiment

The experiment was frozen before triggering generation. Every accepted run:

1. restored the same owner-local POINT OF NO RETURN checkpoint;
2. armed coverage, name-table `vtrace`, and `mtrace` filters for CPU
   `>83C0..>83C1`, CPU `>8378`, and VRAM `>34B8..>36D1`;
3. tapped `FCTN+6` (PROC'D), then ran a fixed total of 12,000 frames;
4. saved the recorders before disabling them; and
5. captured the final screen, seed word, candidate bytes, and save state.

Group A left the restored random seed at `>83C0=>0000`. Group B changed only
that word to `>1234` with a diagnostic poke before arming observation. A1/A2
and B1/B2 ran in fresh probe processes. The fixed window and seed values were
not adjusted after seeing a completed outcome.

The initial 1,200-frame pilot was excluded from acceptance: it stopped on the
visible `DIGGING FLOOR 1` screen, and recorder logs had been saved after the
probe's `off` commands discarded them. Before rerunning, one procedural
addendum extended the otherwise unchanged window to 12,000 frames and moved
each save before `off`. The pilot established neither a completion boundary nor
an atlas claim.

A separate, declared entry breadcrumb restored the same checkpoint, enabled the
existing GPL fetch trace for only the PROC'D tap plus 50 more frames (80 frames
total), saved 10,725 fetches, and stopped. It was needed because static recovery
showed two callers of the per-floor routine; no new tracing capability was
added.

## Observed execution

### Determinism and controlled variation

Both repeats within each group were byte-identical across their complete
`mtrace`, `vtrace`, coverage, and save-state outputs:

| Group | Seed before trigger | Filtered accesses | Candidate writes | GROM addresses / native PCs covered | Save-state SHA-256 |
|---|---:|---:|---:|---:|---|
| A1 = A2 | `>0000` | 9,400 | 1,184 | 6,144 / 997 | `cd2f72913d3b540c1776d99988c6c39815f32c3c5e5ceda6ec4806d3ff908040` |
| B1 = B2 | `>1234` | 9,488 | 1,142 | 6,169 / 997 | `eaba49cf57724dc8a1889401fddeff6c2aa1cc3565a74f35e5ebeac2104fe0d1` |

Changing only the seed changed 220 of the 538 final payload bytes. Group A's
final payload contained 113 `>60`, 19 `>61`, 2 `>62`, 2 `>64`, 6 `>65`, 9
`>66`, 20 `>67`, 1 `>68`, 2 `>6A`, and 300 `>6B` bytes, plus 64 `>20`
padding bytes. Group B contained 64 `>60`, 28 `>61`, 1 `>63`, 4 `>64`, 3
`>65`, 6 `>66`, 20 `>67`, 1 `>68`, 2 `>6A`, and 345 `>6B` bytes, plus the
same 64 `>20` bytes. These are observed byte populations, not semantic tile
names.

The console GPL `RAND` path read the seed at native PC `>0182` / opcode
`>3860`, wrote the next seed at `>018A` / `>C802`, and wrote the result byte at
`>019C` / `>D802`. The first accepted random result was `>0A` from seed
`>0000` and `>0F` from seed `>1234`.

### Boundary

The 80-frame breadcrumb observed this exact entry chain:

```text
GPL >63E3: CALL >8002
GPL >8002: branch to >8246
GPL >8246: per-floor generator entry
```

The byte-identical GSL recovery places `>63E3` inside the trace-discovered
outer routine beginning at `>62F4`. That routine initializes dungeon state,
calls `>8002` once per floor, calls the following per-floor continuation at
`>8022`, increments the floor counter at `>63F2`, compares it with the selected
floor count at `>63F6`, and leaves the loop at `>63FF` when complete. The
accepted run covers that fall-through and later displays the GENERAL STORE.

For group A, candidate mutation began at modeled frame `2409+39188`, the last
candidate write occurred at `3316+3923`, and the final filtered candidate read
occurred at `3780+48013`. The last non-cursor name-table change completing the
GENERAL STORE display occurred at `4274+39849`. Group B's corresponding last
write, last read, and visible completion were `3397+9979`, `3782+40739`, and
`4281+41103`. Modeled frame numbers include the checkpoint's pre-existing
frame position; every run still executed exactly 12,000 session frames.

### Candidate representation and mutators

The neutral candidate is the 538-byte VRAM payload `>34B8..>36D1`. Static
decode shows row and column loops over `02..12` and `03..1C`, a stride of
`>0020`, and an index-base subtraction of `>0043`. This is consistent with a
17-by-26 active grid embedded in a 32-byte row stride. The span also contains
padding/border cells, so that geometry is a decoded layout fact, not a claim
that all 538 bytes are playable cells.

Every observed candidate write was performed by native PC `>1D2A` / opcode
`>D802` (`MOVB R2,@>8C00`) on behalf of these GPL operations:

| GPL operation | Decoded operation | A writes | B writes | Evidence role |
|---|---|---:|---:|---|
| `>8611` (stream next `>8617`) | `AND V@>34B8(index),>EF` | 474 | 474 | reset/normalization pass |
| `>863D` (stream next `>8643`) | `ST V@>34B8(index),>6B` | 474 | 474 | reset remaining candidate cells to `>6B` |
| `>8553` (stream next `>8559`) | `ST V@>34B8(index),@>8308` | 23 | 23 | seed-dependent value placement |
| `>8339` (stream next `>833F`) | `ST V@>34B8(index),@>8306`; index then `+>0020` | 53 | 65 | stride-32 stores; 51 / 58 unique addresses |
| `>83D8` (stream next `>83DE`) | same store; index then `+>0001` | 160 | 104 | stride-1 stores; 151 / 98 unique addresses |
| `>845F`, `>847E` | additional indexed candidate stores | 0 | 1 each | seed-dependent stores in B |

R1's existing `--entries` mechanism recovered two previously data-classified
executed regions without changing GSL: entry `>62F4` recovered the outer
routine, and entry `>852F` recovered the random-placement code after the
unsupported one-byte XGPL operation at `>852E`. The resulting five-page GSL
roundtrip was byte-identical: 263 functions, 5,185 statements, 19 raw-byte
instructions, and zero demotions.

## Reproducible property

The evidence proves this bounded property:

1. `>8605..>864B` performs an invariant reset over the candidate payload. Both
   seed groups execute the same 948 writes at the same two GPL/native sites.
2. `>852F..>857A` consumes `>8378` random results and performs 23
   seed-dependent value stores at `>8553`.
3. The `>82C0..>83FF` loops then perform stride-32 and stride-1 stores; write
   counts and the final byte layout differ by seed.
4. Later code reads the completed candidate at `>A3B5` (stream `>A3BD`) after
   candidate mutation has ceased.

Thus the seed changes the 23 stored addresses and the subsequent stride-store
results, while reset size and write attribution remain invariant. Calling this
a dungeon topology/carving phase is a strong inference supported by the visible
`DUNGEON UNDER CONSTRUCTION / DIGGING FLOOR 1` screen and the decoded
row/column/adjacent-cell operations. Exact meanings of `>60..>6B` remain
unresolved.

## Evidence classification and limitations

- **Observed:** trigger, modeled boundaries, seed transitions, ordered reads
  and writes, native PC/opcodes, GPL stream breadcrumbs, counts, final payloads,
  visible progress/store screens, and exact repeat hashes.
- **Source-confirmed:** static GPL instruction decoding, routine/call
  boundaries, loop bounds, strides, and operand addresses from a byte-identical
  roundtrip of the matching owner-local cartridge bytes.
- **Inferred:** “floor grid,” “marker placement,” “topology propagation,” and
  “carving” are neutral proposed purposes, not recovered original names.
- **Unresolved:** exact tile-value meanings; why the reset loop tests offset
  `>021A` while the copied payload count is `>021A` bytes; meanings of special
  markers; later consumers beyond the `>A3B5` reader; room contents,
  stocking, monsters/items, and multi-floor persistence.

No secondary emulator or published game-source implementation was consulted.
The result was frozen from the matching runtime and byte-verified decode first,
so there is no secondary provenance to promote or confuse with observation.
Libre99 is authoritative here only for what this reproduced execution did; it
does not make the inferred game semantics primary evidence.

## Economic decision

A future **per-floor candidate-buffer kernel** model bounded to `>8246..>84F1`
plus helpers `>84F2..>86B8` is economically plausible: inputs, random source, payload,
mutators, phase order, and deterministic oracle are now known. A **full dungeon
generator** reconstruction is not yet economical as one package because the
later content/stocking and inter-floor data contracts are not bounded, and the
tile alphabet lacks discriminating semantics.

The next task, only if the owner authorizes it, should choose between (a) one
neutral executable model of that per-floor candidate-buffer kernel with held-out
seeds or (b) a different subsystem. It should not silently expand this reconnaissance
into complete generator, atlas, or game reconstruction.
