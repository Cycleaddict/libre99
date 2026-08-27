# Tunnels of Doom payload consumer semantics

## Question and boundary

This package asks one bounded question: how do the existing GPL consumers at
`>A3B5` and `>A5E1` interpret the neutral per-floor candidate payload at
VRAM `>34B8..>36D1`?

The answer is a structural decoder, not a renderer or a complete dungeon
generator. It does not assign room, corridor, wall, door, stocking, or visual
tile names. Those meanings remain unresolved. No emulator execution was added
for this package: the comparison uses the already accepted authoring capture
and the already frozen held-out region and trace.

## Source-confirmed consumer behavior

The byte-verified owner-local GSL recovery establishes the following behavior:

- The active coordinates are 17 rows by 26 columns with a 32-byte row stride.
  Coordinate `(row, column)` addresses
  `>34B8 + row * >0020 + column`.
- GPL `>A3B5..>A3E1` reads a payload byte, ignores bit `>10`, and maps the
  normalized value to a neutral consumer class.
- GPL `>A5AF..>A5D6` maps direction indices to north (`-32`), east (`+1`),
  south (`+32`), and west (`-1`).
- GPL `>A5D7..>A617` rotates the direction bit and tests it against the
  current cell's connection mask. The neutral mask bits are north `>01`, east
  `>02`, south `>04`, and west `>08`.
- Direct consumers of this recovered behavior include GPL `>A62B..>A662` and
  `>A766`.

The exact decoded table is:

| Normalized byte | Consumer class | Connection mask | Directions |
|---|---:|---:|---|
| `>60` | 1 | `>0A` | east, west |
| `>61` | 1 | `>05` | north, south |
| `>62` | 1 | `>0B` | north, east, west |
| `>63` | 1 | `>0E` | east, south, west |
| `>64` | 1 | `>0D` | north, south, west |
| `>65` | 1 | `>07` | north, east, south |
| `>66` | 1 | `>0F` | north, east, south, west |
| `>67` | 2 | `>0F` | north, east, south, west |
| `>68` | 5 | `>0F` | north, east, south, west |
| `>69` | 4 | `>0F` | north, east, south, west |
| `>6A` | 3 | `>0F` | north, east, south, west |
| `>6B` | 0 | `>00` | none |

The labels “consumer class” and “connection mask” describe proven operations,
not game-level semantics. Raw `>70..>7B` decode identically to `>60..>6B`
because the consumer explicitly clears bit `>10`. Other values are rejected
instead of silently assigned a meaning.

## Decoder and comparison

`tools/tod_payload_decoder.py` is a standard-library command with no emulator
dependency:

```bash
python3 tools/tod_payload_decoder.py decode --payload INPUT
python3 tools/tod_payload_decoder.py cell --payload INPUT --row 1 --column 1
python3 tools/tod_payload_decoder.py region --payload INPUT \
  --row 0 --column 0 --height 4 --width 4 --json
python3 tools/tod_payload_decoder.py compare \
  --prediction FROZEN-PREDICTION.json --evidence EXISTING.mtrace --json
```

`INPUT` may be exactly 538 raw bytes, a JSON object with `payload_hex`, or an
existing probe text dump containing the full range. Machine-readable output
includes row, column, byte offset, VRAM address, raw and normalized bytes,
ignored-bit status, neutral class, mask, cardinal booleans, evidence label,
and retained uncertainty.

The comparison recognizes only the two proven consumer read points in the
filtered existing trace. Each coordinate receives one of four statuses:

- `observed`: both consumer operations read the predicted byte;
- `partialObservation`: only one required operation read the predicted byte;
- `contradiction`: an operation read a different byte; or
- `notObserved`: neither operation read the coordinate.

Acceptance is deliberately by **distinct decoded raw/class/mask tuple**, not by
coordinate. Every tuple in the frozen region must have at least one `observed`
coordinate and no observed coordinate may contradict the prediction. An
unvisited duplicate remains `notObserved`; absence is never promoted into a
positive coordinate claim.

## Existing-evidence result

The authoring floor provides 124 unique addresses at the `>A3B5` classifier
read and 125 at the `>A5E1` mask read, with 125 unique addresses in their
union. Every consumed byte agrees with the corresponding already recorded
payload byte. This is observed agreement with the source-confirmed decoder
boundary; it is not evidence for a game-level tile name.

The deterministic first-row-major held-out selection was frozen before its
consumer evidence was compared. Its prediction JSON has SHA-256
`27873a12a816bb933e9bf28f32f5a0903cb3b753c3d39b7bd569aa19d511693d`.
The selected region contains four distinct decoded tuples, for raw `>60`,
`>61`, `>67`, and `>6B`.

The corrected comparison is **PASS**:

- 4/4 distinct raw/class/mask tuples have an authentic `observed` coordinate;
- 11/16 coordinates were read by both required consumer operations;
- 0 partial observations and 0 contradictions occurred;
- 5/16 coordinates were not visited; all five are duplicate `>6B`, class 0,
  mask `>00` instances; and
- the corrected comparison JSON has SHA-256
  `971a9b065d95cfa3782c5c7ce6a0d7b5056bd59efe790c521219fb89fd9203db`.

The five unvisited coordinates are `(0,0)` `>34B8`, `(2,2)` `>34FA`, `(2,3)`
`>34FB`, `(3,2)` `>351A`, and `(3,3)` `>351B`. Their only accepted statement is
`notObserved`. Coverage of another coordinate with the same tuple validates
the decoder tuple, not the contents or role of any unvisited coordinate.

## Evidence labels and retained uncertainty

- **source-confirmed:** grid geometry, bit-`>10` normalization, neutral class
  mapping, direction offsets, mask construction, and mask tests, from the
  byte-verified recovered GPL instructions.
- **observed:** exact authoring reads and held-out tuple coverage from the
  accepted owner-local filtered captures.
- **unresolved:** game-level names for the raw values and neutral classes;
  coordinate-specific meaning at every unvisited cell; how later stocking,
  rendering, or inter-floor logic uses these classes.

No new floor, seed, region, or direct-cell execution was performed. Commercial
media, payload dumps, traces, GSL recovery, and comparison artifacts remain
owner-local and outside Git.
