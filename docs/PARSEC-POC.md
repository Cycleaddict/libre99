# Observatory gate 1 — Parsec causal POC

**Result: PASS** (one frozen gameplay frame, not the whole scrolling engine).

Commercial media, checkpoints, traces, screenshots, and decompilations stay
outside Git under `~/.local/share/libre99-observatory/parsec-poc/`.

## Question

From a frozen in-game checkpoint, which CPU instructions produced the VDP
data-port writes of **one** subsequent frame, and which of those writes
changed VRAM (`old != new`)?

## Media and checkpoint (identities only)

| Artifact | Owner-local path | SHA-256 |
|---|---|---|
| Cartridge | `Parsec.ctg` | `82540f122eecb92be2b4a1bb2693fbcd74e45aaba937871bcff500763a88c056` |
| Extracted ROM | `parsec-rom.bin` | `c0a03e2376d7928a914fffd14f3d9a3dbf52bb4455ed1202cb596aa91cee202b` |
| Frozen gameplay checkpoint | `parsec-scroll-before.state` | `4a91a716a4f9ffeb259305f0cd6f4395fa7d7f3831d100305a8a846e862e65d6` |

The checkpoint is the load used by the one-frame causal capture (`vtrace on
0000 3FFF`, then `frames 1`). Restore plus that script is deterministic on
this runtime.

Replay:

```bash
python3 tools/observatory_campaign.py \
  --manifest ~/.local/share/libre99-observatory/parsec-poc/parsec-causal-100.json \
  --probe target/release/libre99probe \
  --output OUTPUT_DIR \
  --only baseline-01
```

Campaign-scale repeats of that identical variation share causal digest
`4b3d7c20ce1eb6aa36402276307ff4d61621613c0384b380127cbe4420a58dc5`.

## Observed one-frame result

| Measure | Value |
|---|---|
| Capture | 1 frame, full VRAM vtrace `>0000–>3FFF` |
| Total VDP writes | 52 |
| State-changing writes (`old != new`) | 34 |
| Writer PCs | `>734C`, `>7E76`, `>7EF2`, `>7EF8`, `>7F04`, `>7F0A` |

Of the 34 changing writes:

- **32** at PC `>734C`, opcode `>D801` — Graphics-II pattern-table bytes in
  `>0000–>0FFF` (`observed`).
- **2** at PC `>7E76`, opcode `>D836` — sprite-attribute table `>1B06`
  (`>98→>88`) and `>1B07` (`>0F→>03`) (`observed`).

The remaining writes in the 52 are non-changing (`old == new`), including
further calls through `>7E76` and sprite helpers `>7EF2`/`>7EF8`/`>7F04`/`>7F0A`.

## Blind inference, then listing

Blind (vtrace + static cart ROM, listing hidden): `>734C` is a VRAM
byte-write helper used by a coordinate/phase walk that shifts sparse
pattern bits; `>7E76` is a counted CPU-to-VRAM copy. The 32 bit-shifts
were inferred as starfield/background motion; the sprite-1 copy was
inferred as a prominent animated object (`inferred`).

Held-out Parsec source listing, after freeze: `>734C` belongs to the
star-field writer path; `>7E76` is the generic CPU-to-VRAM copy used
there for the ship-fire sprite (`source-confirmed` for those two
identities relative to the listing). Whole-engine recovery is not
claimed.

## Neutrality

`vtrace` allocates nothing while off. Recording does not change save
states or rendered frames (`observed`, focused tests). Play mode does
not pay for the log unless armed.

## Labels

| Claim | Label |
|---|---|
| 52 writes / 34 changing on this checkpoint+1 frame | `observed` |
| 32 changing writes at `>734C` / `>D801` | `observed` |
| 2 changing sprite-table writes at `>7E76` / `>D836` | `observed` |
| `>734C` is the star-field writer path | `source-confirmed` (listing after freeze) |
| `>7E76` is the CPU-to-VRAM copy for the ship-fire sprite in this window | `source-confirmed` (listing after freeze) |
| Complete scrolling engine recovered | not claimed |

## Limitations

- One frozen frame, not the full scroll/render pipeline.
- Joystick holds on that same frame did not change the causal digest
  (input is sampled on a longer timescale than this window).
- Commercial media and traces remain owner-local.
