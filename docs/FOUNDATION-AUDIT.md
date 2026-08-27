# Foundation behavior audit

## Purpose

Libre99 is a working emulator, but its implementation and tests were largely
developed in one project lineage. The previous TI experiment used multiple
agent seats, explicit source ranking, and independently reviewed chip corpora.
Neither history makes its code correct by itself.

Compare Libre99 against the prior accepted behavioral evidence in two passes.
This is a conformance audit, not a source merge and not a revival of the old
development process.

## Sources under comparison

- Candidate runtime: this repository, based on Libre99 `gsl` commit
  `0ff10f666af815be23a0c3045b9c324083faab29`.
- Independent legacy evidence: the separately preserved experiment at commit
  `052664a2511c66be2651cb8dc6e3b178e0ed8c75`.
- Primary TI/chip documentation cited by either project.
- Independent emulator implementations only as corroboration.
- Authentic software executions and held-out benchmark observations.

The old runtime is not the oracle. Libre99 is not the oracle. Primary evidence
and reproducible authentic behavior decide disagreements.

## Sequence

### A. Prequalification before the POC

This is deliberately small and must not become a full emulator recertification:

1. Keep the green workspace test/clippy baseline.
2. Confirm chip ownership boundaries and that observation can be added without
   coupling the frontend or analysis into the core.
3. Run representative legacy vectors for CPU flags/addressing, VDP ports and
   status, GROM address/data sequencing, TMS9901 interrupt/keyboard behavior,
   and console byte/word conversion.
4. Run one reset-to-title/menu comparison with authentic firmware and compare
   selected instruction/device/frame boundaries.
5. Prove the owner-local Parsec media boots and accepts deterministic input.

Passing this gate authorizes the Parsec POC. It does not certify the whole
machine.

### B. Rolling audit during the POC

When a Parsec trace depends on a chip behavior not covered by prequalification,
compare that behavior before relying on the resulting reconstruction. A
disagreement becomes one focused evidence question and regression, not a
general chip rewrite.

After the POC, expand the audit only if the observatory produced enough value
to continue or the next investigation reaches new behavior.

## Comparison scope

Audit the software-visible behavior needed to boot, control, and investigate
the MVP cartridges:

| Area | Compare |
|---|---|
| TMS9900 | instruction results and flags, addressing side effects, workspace accesses, interrupt entry/return, and timing where the running software can observe it |
| TMS9918A | data/control ports, address/prefetch behavior, status side effects, scan/frame/sprite results used by the games |
| GROM/GPL | counter/latch behavior, chip selection, address/data ports, GPL fetch/execute transitions, and relevant wait timing |
| TMS9901/CRU | keyboard matrix, timer/interrupt behavior, active levels, and CRU addressing used by the console/software |
| Console integration | ROM/RAM mapping, byte/word port conversion, interrupt routing, cartridge banking, and deterministic input/checkpoint behavior |

Do not exhaustively replay every legacy corpus before beginning useful work.
Start with representative prequalification vectors, then the paths reached by
Parsec and the selected Tunnels of Doom subsystem. Expand only when a real
trace reaches an unaudited behavior or a disagreement threatens a causal
conclusion.

## Method

1. Inventory the legacy rows/tests relevant to each area, retaining their
   source IDs and confidence labels.
2. Translate input and expected behavior into neutral test vectors or a thin
   test adapter. Do not import legacy framework types or implementation code.
3. Execute the same vector against Libre99. Record `match`, `difference`,
   `not-representable`, or `not-yet-reached`.
4. For each difference, inspect the primary source first. Then use authentic
   execution or multiple independent emulators as corroboration.
5. Fix Libre99 only when the evidence identifies Libre99 as wrong. Add a
   focused regression and label the authority.
6. If the prior result is wrong or overconfident, record that explicitly; do
   not distort Libre99 to preserve an old green result.
7. Run one shared integration comparison from reset through the first usable
   title/menu state, comparing instruction checkpoints, device accesses, and
   frame output at selected boundaries rather than requiring identical
   internal architecture.

## Review model

Implementation may be performed by one coding agent. Independence comes from
the evidence and the reviewing seat, not from multiplying prompts:

- one agent implements the adapter or focused correction;
- a second model reviews mismatches, source rank, and whether the regression
  discriminates the defect; and
- real executions provide the final integration evidence.

Routine changes do not need the old controller/relay ceremony. A mismatch
that changes a software-visible hardware fact or benchmark conclusion does
receive independent review before it is accepted.

## Completion record

Produce one concise matrix containing:

- behavior/vector identifier;
- component and authentic consumer;
- primary-source citation or evidence label;
- legacy result;
- Libre99 result;
- disposition and regression path; and
- remaining uncertainty.

The audit is sufficient to proceed when all behavior reached by the selected
Parsec benchmark is either matched, corrected, or explicitly labeled as an
unresolved compatibility choice that does not invalidate the investigation.
Non-reached edge cases remain a visible backlog; they do not block the MVP.

## What this audit does not prove

- That either codebase is cycle-perfect for every program.
- That several agreeing models or emulators manufacture a hardware fact.
- That a passing unit suite replaces authentic execution.
- That implementation lineage is clean-room evidence by itself.
- That the complete prior 4,000-file corpus must be transplanted.
