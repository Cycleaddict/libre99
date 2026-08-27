# Observatory working agreement

This branch turns Libre99's existing emulator, GSL toolchain, and probe into
an evidence-producing reverse-engineering observatory. It is not another
emulator rewrite.

## Start here

Before changing code, read:

1. `START-HERE.md`
2. `docs/OBSERVATORY-MVP.md`
3. `docs/FOUNDATION-AUDIT.md`
4. `docs/BENCHMARK.md`
5. `docs/RECONSTRUCTION-NEXT.md` for the current technical direction
6. `docs/AGENT-COMMANDS.md` before invoking another model
7. The relevant upstream guide: `docs/GSL.md`, `docs/PROBE.md`, or
   `docs/FINDINGS.md`

`CLAUDE.md` remains useful upstream project guidance. For observatory work,
the evidence rules below take precedence over its claim that one reference
emulator is authoritative.

## Operating rules

- Build on the current Libre99 implementation. Do not re-create the old
  breadboard framework, relay, stage system, generic bus, or security
  infrastructure.
- Add something only when it helps run authentic software, capture a
  reproducible behavior, or explain/reconstruct that behavior.
- Prefer a real execution and a focused regression over speculative
  completeness work. Fix reproduced defects; record non-blocking ideas.
- Keep observation optional and cheap when disabled. The emulator executes;
  recording observes; analysis happens outside the hot loop.
- Use the smallest TI-specific implementation that proves the MVP. Do not
  design Atari/C64 abstractions before the TI benchmark works.
- Do not change working code merely to impose a preferred style.
- Keep commercial firmware, cartridges, disks, traces derived from them, and
  decompilations outside Git.
- Keep personal usernames, email addresses, absolute home-directory paths, and
  private coordination context outside tracked files. Use repository-relative
  paths, shell-discovered roots, and neutral evidence labels in public docs.
- Preserve Libre99's license headers and documentation requirements.
- The active Codex task is the controller. It derives scope from the repository,
  asks Grok read-only design/evidence questions when useful, delegates bounded
  implementation to Ox Alpha when available, reviews every resulting diff, and
  runs the real acceptance commands. The owner does not shuttle prompts.
- A coding model does not commit, push, expand scope, or rewrite working code for
  style. Model failure is a tooling result, not an engineering finding.

## Evidence discipline

Evidence ranks in this order:

1. Primary hardware/software documentation and original source when available.
2. Reproducible observation from authentic software on this runtime.
3. Corroboration from independent emulators or implementations.
4. Inference.

Label conclusions `source-confirmed`, `observed`, `corroborated`, `inferred`,
or `unresolved`. Several agreeing emulators are useful corroboration, never a
substitute for primary evidence. A compatibility choice may be implemented
when needed to make progress, but must retain its label and a focused test.

## Proportionality

This is a private hobby/research tool on a trusted local machine. Hostile-user
security, per-event hashing, signing, compliance provenance, 24x7 operation,
and proof whose only consumer is another verifier are outside the MVP. Media
identity at load boundaries and GSL byte-roundtrip verification remain useful
because they answer real research questions.

Work in ordinary reviewable commits. A second model may review risky behavior
or evidence claims, but no stage machinery or agent relay is required.
