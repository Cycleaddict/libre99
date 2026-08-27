# Tunnels of Doom per-floor kernel retry

## Frozen static prediction

This section was recorded before the one authorized authentic case.  It uses
the accepted `>A5C3` kernel-entry checkpoint (save-state SHA-256
`d0060d0d80f4b0e0b8d080c0448d358c84b1f94f00da98a221f691376ee12ab3`)
with only VRAM `>1CEC` changed from `>14` to `>00` and `>1CE6` changed from
`>02` to `>00`.  Changing either count alone did not satisfy the direct retry
predicate in the offline model.  No seed was searched.

The complete model input is seed `>A5C3`, payload SHA-256
`c32f4c8335180942983cacea8d783c50b62cf95181080040a4bcd5ec97de3313`,
`>3498..>36F1` context SHA-256
`4e76897ae4f204ec6ee9598655f458eab7f5d8c69cde2606a8beed1c4c64b26b`,
counts `>00/>00/>02`, position index/limit `>01/>01`, and control `>833F=>09`.
The separate post-pass checker is unresolved and is not assumed.  The model is
bounded after one completed direct retry.

The frozen owner-local prediction has SHA-256
`259fe32ca2e9d01a6b30f8b836502b2a94947460338928b2c046ccc5f6f5b568`.
It predicts one initial `>68` placement at VRAM `>3670`, then the direct retry
path `>84EB -> CALL >8605` with inline mode byte `>01 -> >8283`.  Mode `>01`
retains that `>68`; the zero placement counts consume no further RAND state,
so the same predicate recurs and bounded modeling stops at the second `>84EB`.
The expected seed is `>A181` after two RAND calls, with 1,896 candidate writes:
948 at `>8611`, 947 at `>863D`, and one at `>8553`.  Expected output payload
SHA-256 is
`d759204b718aa1510ffae5384d162efd2ca7b0b7c73542c981ff0921b1c23494`.

## Source-confirmed contract

The direct branch at `>84EB` is taken during the `>8403..>84DF` scan exactly
when scratchpad `>833F` is `>09`, the current payload byte is `>68`, `>69`, or
`>6A`, and all four neighbors at offsets `->20`, `+>20`, `-1`, and `+1` are
`>6B`.  A similarly surrounded `>67` is instead changed to `>6B` at `>842C`
and scanning continues.

`CALL >8605` reads caller-inline mode byte `>01`.  The reset scans offsets
`>0000..>021A`: bytes below `>60` are retained; other bytes first have bit
`>10` cleared at `>8611`; normalized `>68` is retained in mode `>01`, while
normalized `>69` and all other values at least `>60` become `>6B` at `>863D`.
The reset performs no RAND call.  Control returns toward `>8283`, which reloads
the `>67/>6A/>69` counts and restarts those placements without repeating the
initial `>68` placement.  The seed, position state, count sources, base
pointers, and retained `>68` placement survive; placement destinations and
working coordinates are rewritten.

The checker at `>857B..>85B4` is a separate predicate whose true return joins
the same `>84EB` reset route.  Its complete payload-level meaning remains
unresolved: `>85B5` delegates through recovered helpers at `>A62B` and
`>A5AF`, so byte-exact local control flow does not establish a neutral graph
or path predicate.  Model version 2 therefore requires an accepted no-retry
result explicitly when normal cleanup reaches `>857B`, and refuses a guessed
true result.

## Authentic comparison

**PASS for the frozen bounded route.** One authentic case loaded the accepted
checkpoint, applied the two recorded `vpoke` mutations, and enabled only
existing trace/coverage recorders.  It ran in one probe process; no seed search,
correction case, or emulator change occurred.  The saved owner-local GROM-fetch
trace has SHA-256
`5537a0122795e2d0514c2c628666d878f834aa0226eaeedc6c43742d8bb455ab`.

The trace prefix ending at the second fetch of `>84EB` contains the predicted
retry-specific sequence `>84EB -> >8605 -> >8283 -> >84EB`.  Including the
initial mode-`>02` reset, that prefix has two fetches of `>8605`, two of
`>8283`, one placement entry at `>852F`, one candidate store at `>8553`, 948
stores at `>8611`, and 947 stores at `>863D`: exactly the model's 1,896
candidate writes.  The only placement invocation executes the two decoded
RAND operations, producing seed `>A181`; the mode-`>01` reset and restarted
zero-count placements do not advance it.  The retained isolated `>68` is at
`>3670`, surrounded by `>6B` at `>3650`, `>3690`, `>366F`, and `>3671`.

The probe was advanced in frame increments and crossed the second predicate
before recording was disabled.  The comparison is therefore explicitly the
frozen trace prefix, not the later repeated cycle.  Read-only inspection of
the saved final state gives seed `>A181` and payload SHA-256
`d759204b718aa1510ffae5384d162efd2ca7b0b7c73542c981ff0921b1c23494`,
also exact.  There is no mismatch within the declared one-retry boundary.  The
authentic continuation confirms that the unchanged state retries again; the
offline model stops at that recurrence rather than pretending it completes.
