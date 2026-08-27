# Tunnels of Doom stairs fallback contract (`>8018` / `>96EA`)

G-007 resolves the last guessed input in the bounded stairs model. The route is
a shared message acknowledgement trampoline, not a game-state predicate. On
the stairs fallback, it always returns with GPL condition reset, so the branch
at `>66C5` continues at `>663F`; it never authorizes the mutation path at
`>66C7`.

## Source-confirmed contract

The byte-verified GPL sequence is:

```text
>66AE  require VRAM >1CE8 = >01 for the fallback checks
>66B5  unsigned >10FE < >1CF8 falls through
>66BE  CALL >E00E, inline message byte >2D at >66C1
>66C2  CALL >8018
>66C5  BR (condition reset) >663F
>66C7  increment >1CF8; this path is not taken after acknowledgement

>8018  BR (condition reset) >96EA

>96EA  if scratchpad >83A1 != 0: clear it and RTN
>96F3  otherwise BACK >0C when (vdp_timer & >40) != 0, else BACK >02
>96FF  SCAN; while condition is reset, branch back to >96EA
>9702  compare VRAM >1D01 with >02
        equal: BACK >06; unequal: BACK >03 at >970E
>9710  RTN
```

The accepted console source defines ordinary GPL `RTN` as clearing the
condition bit and `RTNC` as preserving it
(`original-content/system-roms/rom/console.asm`, `RTN`/`RTNC`). The message
routine at `>E00E` and both exits from `>96EA..>9710` use ordinary `RTN`.
Accepted SCAN documentation defines condition set only for a newly detected
key; no new key writes `>FF` and leaves condition reset. Therefore:

1. message `>2D` returns with condition reset;
2. `>8018` tail-transfers to `>96EA` (it has no `RTN` or independent
   immediate-return arm);
3. the nonzero-`>83A1` shortcut clears that byte and returns reset;
4. the zero-`>83A1` route waits for a newly detected key, applies the final
   backdrop, then returns reset; and
5. the stairs caller takes `>66C5` to `>663F` without changing `>1CF8` or
   `>1D00`.

The fourteen static callers use the same reset-condition convention:

| CALL site | Reset-condition provenance before `>8018` |
|---|---|
| `>66C2` | message `>2D` ordinary return |
| `>69BD` | message `>38` ordinary return |
| `>6A70` | message `>76` return or an already reset branch to the join |
| `>6B24` | message `>39` ordinary return |
| `>6B35` | message `>3A`; intervening stores do not preserve a set return |
| `>6C3A` | message `>79`; intervening clear does not provide an accept status |
| `>6C8A` | message `>7A` ordinary return |
| `>6F4D` | each entry to the join follows an ordinary-return message path |
| `>7707` | message `>54` return or a reset branch to the join |
| `>77A0` | message `>57` ordinary return |
| `>C3D3` | message `>80`; `>C8E4` immediately takes its reset branch to ordinary return on this entry |
| `>C48F` | message `>4E` ordinary return |
| `>C4CA` | message `>59`; intervening stores do not provide an accept status |
| `>C659` | `>C667` ordinary return |

Several callers branch on the reset condition immediately after the shared
routine; others use it only as a wait/acknowledgement boundary. No caller
supplies a separate game-state Boolean to `>8018`.

## Frozen alternatives and authentic result

The frozen owner-local prediction has SHA-256
`c46c7972e2e5d3596fd2a3660a13e9ad88b86da35a6ef88dd77d4598f9411200`.
It used the accepted stairs checkpoint and changed only VRAM `>1CF8` from
`>00` to `>01`, making `>10FE=>00` unsigned-below `>1CF8=>01`. The fixed input
schedule was: hold FCTN+X for 60 frames, release for five frames, then press
space.

- Alternative A predicted entry into `>96EA`, a SCAN wait, ordinary-`RTN`
  condition reset, branch `>66C5` to `>663F`, no `>1CF8`/`>1D00` mutation, and
  timer/`>1D01`-selected backdrop writes.
- Alternative B retained only the former model premise: if the new-key
  condition survived the return, `>66C5` would fall through to `>66C7`,
  changing `>1CF8 01→02` and `>1D00 06→05`.

Exactly one authentic process ran. Its 10,409-fetch GPL trace executed
`>66AE`, `>66B5`, message byte `>2D`, `>66C2`, `>8018`, and 382 entries at
`>96EA` while waiting. With the original key held, SCAN repeatedly wrote
`>0A`; release produced `>58` then `>FF`; space produced the new key `>20`.
The last acknowledgement sequence was:

```text
>96EA >96F3 >96F8(BACK >0C) >96FF SCAN
>96EA >96F3 >96F8(BACK >0C) >96FF SCAN(new >20)
>9702(read >1D01=>01) >970E(BACK >03) >9710(RTN)
>66C5 >663F
```

The timer advanced while the routine waited, so the repeated `>96F3` test
used both waiting backdrops (`>02` while bit `>40` was clear, then `>0C` while
set). The frozen prediction specified the entry selection and the same loop;
the frame-60 snapshot therefore showed register 7 at `>0C`. After
acknowledgement it was `>03`, as predicted from `>1D01=>01`.

At both the waiting and final snapshots, VRAM `>1CF8` remained `>01` and
`>1D00` remained `>06`. The trace fetched `>66C5` and `>663F`, never `>66C7`.
Alternative A was selected with no control, input, condition, or state mismatch.
The former `continuation_allowed` Boolean is removed from model version 2.

## Model boundary and retained uncertainty

`tools/tod_stairs_model.py` now accepts concrete fallback state:
`scratch_83a1`, `vdp_timer`, `vram_1d01`, and an optional newly detected
`scan_key_code`. It reports entry/backdrop/return details. When `>83A1=00` and
no new key is supplied, it stops at the proven `>96FF` wait; this is the only
remaining input refusal. Previously accepted positive, negative, and bypass
paths do not reach the fallback and are unchanged.

The neutral contract is complete for this route. Still unresolved are the
game-level meanings of the address-named state cells, how long a user waits,
and the wider UI meanings of message ids and backdrop colors. Those questions
do not change the recovered stairs control contract.
