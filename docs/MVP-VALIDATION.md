# Observatory MVP validation

Four gates. No fifth gate will be added. Fresh-start usability is the
final MVP validation gate.

| # | Gate | Result |
|---|---|---|
| 1 | Parsec causal POC | PASS — [PARSEC-POC.md](PARSEC-POC.md) |
| 2 | Campaign scale | PASS — [CAMPAIGNS.md](CAMPAIGNS.md) |
| 3 | ToD generalization | PASS — [TOD-GENERALIZATION.md](TOD-GENERALIZATION.md) |
| 4 | Fresh-start usability | PASS — this document |

The observatory MVP validation is complete. Further observability work
is driven by actual reconstruction questions, not additional generic
validation.

## Gate 4 — fresh-start usability

Tested public commit: `f80bd41684a433d6fad374db0b12352ce400d717`
(`observatory-mvp` on `https://github.com/Cycleaddict/libre99.git`).

Fresh clone: `/private/tmp/libre99-fresh-zojIqI` (network clone,
`--single-branch observatory-mvp`; cloned HEAD matched the tested
commit; empty `git status --porcelain`).

### Build and test (inside the clone)

| Check | Result |
|---|---|
| `cargo build --release -p libre99-probe` | pass |
| `python3 -m unittest tools/test_observatory_campaign.py` | 8/8 pass |
| `cargo test --workspace` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `git diff --check` | pass |
| clone worktree after tests | clean |

### Parsec replay (`--only baseline-01`)

From the clone’s `target/release/libre99probe` and
`tools/observatory_campaign.py`, using
`~/.local/share/libre99-observatory/parsec-poc/parsec-causal-100.json`.
Output: `~/.local/share/libre99-observatory/fresh-start/parsec-baseline-01/`.

| Field | Result |
|---|---|
| status | ok |
| emulator Git commit | `f80bd41684a433d6fad374db0b12352ce400d717` |
| checkpoint SHA-256 | `4a91a716a4f9ffeb259305f0cd6f4395fa7d7f3831d100305a8a846e862e65d6` |
| causal digest | `4b3d7c20ce1eb6aa36402276307ff4d61621613c0384b380127cbe4420a58dc5` |
| total / changing VDP writes | 52 / 34 |
| writer PCs | `734C`, `7E76`, `7EF2`, `7EF8`, `7F04`, `7F0A` |

### ToD replay (`--only descend-01`)

Same clone binaries, using
`~/.local/share/libre99-observatory/tod-mvp/tod-descend-ab.json`.
Output: `~/.local/share/libre99-observatory/fresh-start/tod-descend-01/`.

| Field | Result |
|---|---|
| status | ok |
| emulator Git commit | `f80bd41684a433d6fad374db0b12352ce400d717` |
| checkpoint SHA-256 | `431aeb39f0470937ec3980fdf64730c18ad088e15e647171ae7a9118b1a40684` |
| causal digest | `3e8a58994afb4e9668e5ea421d2e87ddd122ed833767e3477365675304cdbf60` |
| total / changing VDP writes | 1186 / 282 |
| writer PCs | `15C6`, `1790`, `1D2A`, `1F7A` |
| DESCENDING in name-table observation | yes |

### Fresh-context comprehension

A read-only explore subagent, given only START-HERE, OBSERVATORY-MVP,
PARSEC-POC, TOD-GENERALIZATION, CAMPAIGNS, this document, and the two
replay rows (no chat history, no traces, no listings), answered the six
questions in agreement with those docs: bounded mission; one-frame
Parsec causal write; one ToD stairs-descend; inferred/unresolved items
kept labeled; campaign `--only` replay; MVP non-goals. It distinguished
observation from inference and did not claim complete source or map
recovery.

One documentation correction followed: TOD-GENERALIZATION now names the
exact `tod-descend-ab.json` replay command (the reader could not find
that filename in the tracked docs).

## Remaining known limitations

- `F-001` (GSL inline-argument tiler), `F-002` (raw memory export),
  `F-003` (RAND seed) are still open.
- Parsec POC is one frame, not the full scroller.
- ToD result is one stairs-descend from the spawn **room** camera, not
  the map generator or hallway 3-D view.
- Commercial media, checkpoints, and traces stay outside Git.
