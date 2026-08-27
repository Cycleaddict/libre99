# Executable model — Tunnels of Doom stairs-descend (R4)

The R4 work package reconstructs one bounded ToD subsystem into control flow,
data layout, pseudocode, and an **executable behavioral model**. This document
is the model's specification; `tools/tod_stairs_model.py` is the model, and
`tools/test_tod_stairs_model.py` is its focused regression suite. Python 3
(standard library only), like the atlas and campaign tools — it is observatory
tooling, not part of the Cargo workspace.

The model is a **reconstruction**, not an emulator and not a GPL runtime. It
executes nothing, reads no media, and holds no cartridge bytes. It predicts a
decision and a small set of byte mutations, and it says plainly where the
evidence stops.

## Boundary

Modeled:

- the GPL stairs-descend decision beginning at `>669B`,
- its immediate state mutations at `>66C7` (`>1CF8` increment) and `>66CB`
  (`>1D00` store),
- the delayed copy at `>A798` (`>10FA = >1CF8`), applied as a **separate**
  operation,
- the message/acknowledgement fallback at `>8018`/`>96EA`, including the
  condition, SCAN, `>83A1`, timer, `>1D01`, and backdrop contract,
- the known native interpreter access boundary, as metadata,
- the DESCENDING visible-effect classification.

Not modeled, and not claimed: the screen renderer, the name-table contents, the map, the dungeon
generator, the party, combat, timing, frame counts, and the rest of the GPL
machine. The eight-frame gap observed between `>66CB` and `>A798` is why the
delayed copy is a separate phase; the model does not pretend to schedule it.

## Neutral data layout

Six explicit unsigned bytes, range-checked `0..255`, named after their
addresses because their game-level meanings are not recovered:

| Field | Address | Space | Role inside the boundary |
|---|---|---|---|
| `key_code` | `>8375` | scratchpad | key byte compared with `0A` at `>669B` |
| `vram_1d00` | `>1D00` | vram | eligibility predicate; `04`/`06` continue, `05` is stored back on acceptance |
| `vram_1ce8` | `>1CE8` | vram | secondary flag compared with `01` at `>66AE` |
| `vram_10fe` | `>10FE` | vram | value compared against `>1CF8` at `>66B5` |
| `vram_1cf8` | `>1CF8` | vram | counter incremented at `>66C7` |
| `vram_10fa` | `>10FA` | vram | destination of the delayed copy at `>A798` |

The fallback additionally accepts four address-based fields: scratchpad
`>83A1`, `vdp_timer` at `>8379`, VRAM `>1D01`, and an optional newly detected
`scan_key_code`. They are consulted only after the `>8018` path is reached. If
`>83A1=00` and no new key is supplied, the model stops at the proven SCAN wait.

## Exact pseudocode

The frozen control flow, reproduced exactly as implemented:

```text
immediate(s):
    if s.key_code != 0x0A:
        reject(branch=0x66E5, reason=input)

    if s.vram_1d00 not in {0x04, 0x06}:
        reject(branch=0x663F, reason=predicate)

    if s.vram_1ce8 != 0x01:
        goto accept_66c7

    if unsigned(s.vram_10fe) >= unsigned(s.vram_1cf8):
        goto accept_66c7

    call message 0x2D; ordinary RTN clears condition
    call 0x8018; reset condition tail-transfers to 0x96EA
    if scratch_83a1 != 0:
        scratch_83a1 = 0
        ordinary RTN clears condition
        reject(branch=0x663F, reason=acknowledgement_complete)
    BACK = 0x0C if (vdp_timer & 0x40) else 0x02
    SCAN until a newly detected key sets condition
    BACK = 0x06 if vram_1d01 == 0x02 else 0x03
    ordinary RTN clears condition
    reject(branch=0x663F, reason=acknowledgement_complete)

accept_66c7:
    s.vram_1cf8 = (s.vram_1cf8 + 1) & 0xFF
    s.vram_1d00 = 0x05
    return accepted(branch=0x66C7,
                    delayed_copy_pending=true,
                    visible_effect=DESCENDING)

delayed(s, accepted_result):
    require accepted_result.delayed_copy_pending
    s.vram_10fa = s.vram_1cf8
    return copy_at_0xA798_complete
```

Notes that the pseudocode compresses:

- The eligibility test is two comparisons, not one. `>66A0` accepts `04`;
  every other value reaches `>66A7`, which requires `06`; anything else takes
  the branch bytes at `>66AC` to `>663F`. The reported operation list
  reflects that structure, so an accepted `04` never lists `>66A7`.
- The `vram_1ce8 != 0x01` bypass reaches the accepted target through `>66B3`,
  and therefore never reads `>10FE` and never enters the acknowledgement path.
- The comparison at `>66B5` is **unsigned**; equality takes the transition
  path. `>10FE = 0x80` against `>1CF8 = 0x7F` accepts, where a signed reading
  would fall through to `>8018`.
- Ordinary fallback completion changes no stairs VRAM byte. SCAN may update
  `key_code`; the nonzero-`>83A1` shortcut clears that separate scratch byte.
- The unresolved result is only the bounded no-new-key wait at `>96FF`.
- The predicate rejection is reported through `>66AC` to `>663F`. The fallback
  return is reported through the distinct branch at `>66C5` to `>663F`.
- The increment wraps within a byte: `0xFF + 1 = 0x00`.

## Interpreter metadata

The native accesses are reported, never executed:

| Native PC | Opcode | Role |
|---|---|---|
| `>08B0` | `>D013` | GPL value-loader byte read of the key code |
| `>08CE` | `>D020` | VDP data-port read of a GPL comparison operand |
| `>1D2A` | `>D802` | VDP data-port write performing a GPL store |

A prediction lists only the boundary accesses its path implies: an
input rejection lists `>08B0` alone; a path that reads VRAM adds `>08CE`; a
path that writes adds `>1D2A`.

## Commands

```bash
python3 tools/tod_stairs_model.py predict \
    --key-code '>0A' --vram-1d00 '>06' --vram-1ce8 '>01' \
    --vram-10fe '>00' --vram-1cf8 '>00' --vram-10fa '>00' --delayed
python3 tools/tod_stairs_model.py predict \
    --key-code '>0A' --vram-1d00 '>06' --vram-1ce8 '>01' \
    --vram-10fe '>00' --vram-1cf8 '>01' --vram-10fa '>00' \
    --scratch-83a1 '>00' --vdp-timer '>8D' --vram-1d01 '>01' \
    --scan-key-code '>20' --json
python3 tools/tod_stairs_model.py predict ... --json
python3 tools/tod_stairs_model.py compare CASES.json
```

Byte arguments accept `10`, `'>0A'`, or `'0x0A'`; each defaults to `>00`.
`--scratch-83a1`, `--vdp-timer`, `--vram-1d01`, and `--scan-key-code` supply
the recovered fallback context. Omitting a new key while `>83A1=00` makes the
`>8018` path stop at its input wait. `--delayed`
applies the copy at `>A798` when one is pending. `--json` emits the whole
prediction — status, reason, branch, visible effect, before/immediate/delayed
state, GPL operations, and native metadata — as one compact sorted line.

Exit codes: `0` success (and, for `compare`, every field matching), `1` a
comparison mismatch, `2` invalid input or an unreadable/malformed case file.

## Comparison file format

`compare` reads a **versioned owner-local** case list. The file holds model
inputs and independently observed in-boundary outputs; it is not tracked, and
it must contain no media path, trace, or cartridge byte.

```json
{
  "format": "libre99-observatory/tod-stairs-comparison",
  "version": 1,
  "cases": [
    {
      "name": "accepted-positive",
      "input": {
        "key_code": ">0A",
        "vram_1d00": ">06",
        "vram_1ce8": ">01",
        "vram_10fe": ">00",
        "vram_1cf8": ">00",
        "vram_10fa": ">00"
      },
      "expected": {
        "status": "accepted",
        "branch": ">66C7",
        "visible_effect": "DESCENDING",
        "immediate": {"vram_1cf8": ">01", "vram_1d00": ">05"},
        "delayed": {"vram_10fa": ">01"}
      }
    }
  ]
}
```

- `input` requires all six stairs byte fields and accepts an optional
  `fallback` object with the four address-based fields above.
- `expected` may declare any non-empty subset of `status`, `reason`, `branch`,
  `visible_effect`, `immediate`, and `delayed`; `immediate`/`delayed` may
  declare any subset of the byte fields. Only what is observed needs to be
  asserted. An unknown key is refused by name rather than ignored.
- Output is deterministic: one `PASS`/`FAIL` line per compared field, grouped
  per case, then a summary of cases, fields, passes, failures, and failing
  cases. A `FAIL` line prints the observed value first and the model's second.

This is deliberately not a general comparison framework. It compares this
subsystem's declared fields and nothing else.

## Declared matrix

| Case | Setup | Frozen prediction | Authentic result |
|---|---|---|---|
| `accepted-positive` | existing spawn checkpoint, `hold fctn x`, `key=0A, 1D00=06, 1CE8=01, 10FE=00, 1CF8=00, 10FA=00` | accept at `>66C7`; `1CF8 00→01`; `1D00 06→05`; delayed `10FA 00→01`; DESCENDING | matched from the accepted R2 capture |
| `rejected-negative` | existing later-room checkpoint, `hold fctn x`, `key=0A, 1D00=05, 1CF8=01, 10FA=01` | reject at `>663F`; no in-boundary mutation; no DESCENDING | matched from the accepted R2 capture |
| `heldout-secondary-bypass` | fresh spawn checkpoint, `>1CE8` set `01`→`00` before input, then `hold fctn x` | accept through the `>66B3` bypass to `>66C7` without reading `>10FE`; `1CF8 00→01`; `1D00 06→05`; delayed `10FA 00→01`; DESCENDING; run twice from fresh probe processes and require identical filtered evidence and in-boundary results | matched; two fresh runs produced byte-identical filtered state and VDP evidence |
| `g007-fallback-ack` | accepted spawn checkpoint, only `>1CF8 00→01`, established stairs input, release, then new key `>20` | message `>2D`; `>8018→>96EA`; SCAN wait; ordinary return; `>66C5→>663F`; no `>1CF8`/`>1D00` mutation | matched in exactly one frozen authentic process; see `docs/TOD-STAIRS-FALLBACK.md` |

The held-out case was **not** used to author or revise the model. The
synthetic unit cases (input rejection at `>66E5`, predicate rejection at
`>663F`, `1D00=04`, the `>1CE8` bypass, unsigned comparison, both `>83A1`
routes, both final backdrops, the no-new-key wait, byte wrap, the delayed-phase
precondition, and input validation) exercise the code; they do not replace the
authentic matrix.

The matrix was frozen before the additional held-out run. The controller then
compared 22 independently observed in-boundary fields: all 22 matched. In the
new case, execution fetched GPL `>66AE`, read `>1CE8=00`, fetched the bypass at
`>66B3`, and reached `>66C7` without any `>10FE` read. It wrote `>1CF8 00→01`
and `>1D00 06→05`, copied `>1CF8` to `>10FA` eight frames later, and displayed
`DESCENDING`. Two fresh probe processes produced byte-identical 91-record
filtered state captures and byte-identical 684-record name-table VDP captures.
The comparison case file and captures remain owner-local.

G-007 was a later, separately frozen contract case, not an amendment to the
22-field R4 matrix. It resolved `>8018`/`>96EA` and produced model version 2;
the earlier positive, negative, and bypass predictions remain unchanged.

## Evidence labels

- **`source-confirmed`** — the static control flow at `>669B..>66E5` and
  `>A798`, from the byte-verified owner-local GSL decode of the matching
  cartridge bytes (referenced logically; no bytes, paths, or decompilation are
  tracked).
- **`observed`** — the R1–R3 runtime facts already accepted in the atlas: the
  `>8375=0A` key byte, `>1D00` `06`/`05`, the `>66AC`→`>663F` rejection, the
  `>1CF8 00→01` and `>1D00 06→05` mutations, the later `>10FA 00→01` copy, the
  native boundary, and the DESCENDING effect. R4 additionally observes the
  predeclared `>1CE8=00` bypass through `>66B3`, absence of a `>10FE` read on
  that path, identical mutations/effect, and deterministic repeat evidence.
  G-007 observes the fallback route through `>8018`, the SCAN wait at `>96FF`,
  return through `>9710`, branch `>66C5→>663F`, and absence of stairs mutation.
- **`inferred`** — model predictions for states no accepted experiment has
  exercised, including the synthetic branch and byte-boundary cases.
- **`unresolved`** — everything in the next section.

A model prediction is never promoted to an observation. `observatory/atlas/0003-tod-stairs-r4-model.json`
records the model, its declared matrix, and its predictions accordingly:
static facts as `source-confirmed`, predictions as `inferred` hypotheses, and
the completed matrix as `observed` facts with logical owner-local evidence
references.

## Retained uncertainty

- `>1D00`: `04` and `06` continue and `05` is stored back inside this routine;
  the cell's complete range and its game-level meaning are not recovered.
- `>1CE8`: compared with `01`; the meaning of either value is unknown, and the
  bypass is modeled purely as "not `01`".
- `>10FE`: compared against `>1CF8` unsigned; what bound or limit it expresses
  is unknown.
- `>1CF8`: a counter incremented on acceptance; its wider use is not proven.
- `>10FA`: the delayed copy's destination; its wider use is not proven.
- `>8018`/`>96EA` is recovered as a shared message acknowledgement contract;
  see `docs/TOD-STAIRS-FALLBACK.md`. Input timing and the wider UI meanings of
  the message/backdrop values remain outside the model.
- The delayed copy's schedule (beyond the observed eight-frame separation) and everything the
  redraw does are outside the boundary.

## Post-R4 viability assessment

This bounded result supports executable reconstruction as a viable next layer:
one neutral, source-free model predicted all 22 declared fields across an
accepted path, a rejected path, and a previously unexecuted bypass. Static
decode supplied exact control flow; filtered authentic execution supplied the
state values, timing separation, native boundary, and visible classification
that static analysis alone could not establish.

That result does not establish economical scaling to the dungeon generator or
the rest of ToD. The address-named state cells and wider UI semantics show
where the next evidence cost begins. R4 therefore ends at an explicit
assessment stop; it does not commission another subsystem automatically.

## Tests

```bash
python3 -m unittest tools/test_tod_stairs_model.py
```

The suite covers the accepted equality path from the positive atlas state, the
`>663F` and `>66E5` rejections, the `1D00=04` alternate predicate, the `>1CE8`
bypass proving `>10FE` and the fallback are not consulted, unsigned
comparison on both sides, the recovered acknowledgement routes and no-key
wait, byte wrap, the delayed phase and its precondition, input
validation, CLI JSON prediction, and `compare` PASS/FAIL/invalid-file
behavior. It also asserts that the field names stay address-based and that
`>8018` remains a neutral tail trampoline rather than a gameplay predicate.
