# Observatory gate 2 — Tunnels of Doom

**Result: PASS** (bounded stairs-descend, not the map generator).

Commercial media, checkpoints, traces, and the full blind report stay
outside Git under `~/.local/share/libre99-observatory/tod-mvp/`.

## Question

From a stable playable dungeon checkpoint, which executing GPL/native
operations, mutable state, and VDP writes cause one directional input to
change the dungeon view in one bounded window?

## Method

1. Copy owner-local PHM 3042 (`tunnels_of_doom.rpk`) and the QUEST/PENNIES
   disk (`Tunnels_Of_Doom_Disk_1.dsk`) out of Git; wrap the RPK GROM as a
   `.ctg` for `libre99probe`.
2. Preregister identities, boot path, checkpoints, input, and hidden
   sources *before* reading community ToD write-ups.
3. Probe-only boot: clean-room firmware, QUEST, one-floor party, leave the
   store into the starting dungeon room.
4. Nine campaign runs (3× baseline, 3× FCTN+X from the spawn room, 3× the
   same chord from a later room). Fresh probe process per experiment.
5. Blind analysis from vtrace, GROM fetches, peeks, and in-game AID.
6. One observability addition when vtrace showed only interpreter PCs:
   optional GROM stream address + R9 on each existing VDP-write record
   (off with vtrace; not in save states).
7. Held-out read of the published manual transcription at
   [4apedia Tunnels of Doom](https://4apedia.com/index.php/Tunnels_of_Doom).

## Nine-run determinism

All nine succeeded. Each group has one causal digest.

| Group | Checkpoint | Setup | Writes / changing | Digest prefix |
|---|---|---|---|---|
| baseline | spawn room | none | 0 / 0 | `a8193de0` |
| descend | spawn room | `hold fctn x` | 1186 / 282 | `3e8a5899` |
| held-out C | later room | `hold fctn x` | 312 / 40 | `e001e3f6` |

`--only descend-01` reproduced `3e8a5899…`.

Replay of that run:

```bash
python3 tools/observatory_campaign.py \
  --manifest ~/.local/share/libre99-observatory/tod-mvp/tod-descend-ab.json \
  --probe target/release/libre99probe \
  --output OUTPUT_DIR \
  --only descend-01
```

## Blind causal answer

**Input** `FCTN+X` (down arrow) from the spawn room.

**GPL/native:** cartridge GROM stream at `>66xx` / `>86xx` / `>C1xx`,
executed by the console GPL interpreter. Changing VDP writes come from
native PCs `>15C6`, `>1D2A`, `>1F7A` (console ROM). R9 `>BE74` on the
`>1D2A` writes is GPL `ST` (`source-confirmed` opcode). Other R9 values
during inner loops are `unresolved` as opcodes.

**State / VDP:** name-table VRAM `>0000–>02FF` is rewritten to a solid
pattern plus the word `DESCENDING` at `>0264`. Scratchpad cells in
`>8300–>8361` also change. Extra VRAM (`>1CF8`, `>0FAxx`, `>34xx`) is
`unresolved` (sprites/patterns/FMT).

**View:** spawn room (top-down **room** camera, not hallway 3-D) →
DESCENDING interstitial.

No cartridge CPU ROM is mounted. The GPL/native boundary here is the
interpreter fetching cartridge GROM, not a BL into cart ROM.

## Case C

**Prediction (from B only):** the same chord on a later dungeon view
would replay the `>66xx`/`>86xx` descend path and the DESCENDING screen.

**Result:** not confirmed. C changed 40 name-table bytes at `>02E5–>02FA`
from GROM `>C1xx`/`>E6xx`/`>E7xx` and PCs `>15C6`/`>1F7A`. No DESCENDING
cluster.

## R2 mutable-state resolution

The bounded R2 capture arms `mtrace` only for scratchpad key byte `>8375`,
VRAM `>1D00`, `>1CF8`, `>1CE8`, `>10FE–10FF`, and `>10FA`, plus the GPL
stream `>6654–66D2`. The positive spawn checkpoint and held-out later-room
checkpoint each ran 60 frames with `hold fctn x`, twice from fresh probe
processes. Owner-local records were byte-identical within each pair: 107
accesses positive and 67 negative. Positive save-state and PNG bytes also
matched a recorder-disabled control run.

### State map

| Cell | Positive start | Negative start | Runtime role | Label |
|---|---:|---:|---|---|
| scratchpad `>8375` | `0A` | `0A` | GPL key code read by the interpreter; `0A` selects the down-arrow branch at GPL `>669B` | `observed`; branch decode `source-confirmed` |
| VRAM `>1D00` | `06` | `05` | The distinguishing predicate cell. GPL `>66A0` first accepts `04`; `>66A7` then requires `06`. The negative `05` branches away at `>66AC`; only the positive continues. `06` is therefore the down-stairs-eligible class in this routine, but the cell's complete game-wide enum is not recovered. | `observed`; broader name `inferred` |
| VRAM `>1CE8` | `01` | not reached | A secondary positive-path flag compared with `01` at `>66AE`; exact game meaning remains unknown. | `observed`, `unresolved` semantics |
| VRAM `>10FE` | `00` | not reached | Compared against `>1CF8` at `>66B5`; equality takes the transition path here. Exact bound/limit meaning remains unknown. | `observed`, `unresolved` semantics |
| VRAM `>1CF8` | `00→01` | `01`, not read on rejected path | GPL `INC` at `>66C7` changes the transition counter. Its later use and copy are consistent with a zero-based floor/depth index. | change `observed`; name `inferred` |
| VRAM `>10FA` | `00→01` later | unchanged in this window | GPL `ST` at `>A798` copies the new `>1CF8` value here during the subsequent redraw/setup path. | copy `observed`; cached-floor meaning `inferred` |

### Causal sequence and native boundary

1. Native `>08B0` / opcode `>D013` (`MOVB *R3,R0` in the GPL value
   loader) reads `>8375=0A` in both runs while GPL `>669B` compares the
   down-arrow key.
2. Native `>08CE` / opcode `>D020` (`MOVB @>8800,R0`) reads VRAM for the
   GPL comparisons. At GPL `>66A0`/`>66A7`, it reads `>1D00` as `06` in
   the positive run and `05` in the negative. The negative fetch stream
   ends this path at branch bytes `>66AC–66AD`; it performs no filtered
   state write.
3. The positive run continues through reads of `>1CE8=01`, `>10FE=00`,
   and `>1CF8=00`. GPL `INC` at `>66C7` reads `>1CF8`, then native
   `>1D2A` / opcode `>D802` (`MOVB R2,@>8C00`) writes `>1CF8 00→01`.
4. GPL `ST` immediate-byte at `>66CB` reads the destination's old value,
   then the same native writer stores `>1D00 06→05`.
5. Eight modeled frames later, GPL `ST` at `>A798` reads `>10FA=00` and
   `>1CF8=01`, then native `>1D2A` writes `>10FA 00→01`.

Thus the evidence proves the predicate input, distinguishing cell, accepted
GPL branch, primary transition mutation, and later copy. It does not yet prove
the complete enum represented by `>1D00`, nor whether `>1CE8` and `>10FE`
encode party mode, generated depth, or another bound. Those alternatives are
left for a later experiment rather than promoted from inference.

## Held-out (4apedia / TI manual)

`corroborated` for this PHM 3042 product, not a byte-identical listing.

- Confirms FCTN+X = down stairs, multi-second descent, spawn-room stairs,
  QUEST load, `SELECT MOVEMENT OPTION`.
- Bounds the C prediction: FCTN+X descends only on stairs. After leaving
  the stair cell, C should not repeat DESCENDING.
- Clarifies the spawn camera is the *room* view; hallway 3-D was not this
  window.
- Does not name GPL addresses or interpreter PCs.

## Labels and leftovers

| Claim | Label |
|---|---|
| FCTN+X from spawn → DESCENDING via VDP name-table writes | `observed` |
| Writers are console-ROM GPL interpreter PCs | `observed` |
| Cartridge GROM `>66xx`/`>86xx` is the GPL stream for that redraw | `observed` |
| `>BE` is GPL `ST` | `source-confirmed` |
| Manual key binding and stairs-only descend | `corroborated` |
| Routine names / map / facing bytes | `unresolved` |

Out of scope and not claimed: dungeon generator, combat, QUEST database.

## Code added

`vtrace` records prefetch-corrected GROM address, next GROM byte, and R9
on each VDP write while recording is on. Default off. Save states and
rendered frames unchanged (existing neutrality test extended).
