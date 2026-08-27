# Other-model command cookbook

These are known command shapes for asking another installed model a bounded
question from this repository. They are conveniences, not required gates or a
review process. The calling agent decides when another perspective is useful
and invokes it directly; the owner does not shuttle prompts.

Never put an API key in a prompt, command, repository file, or captured output.
Use the existing local CLI authentication.

## Normal engineering allocation

- The active Codex task is the controller: derive state, bound the change,
  invoke other models, inspect every diff, run acceptance, and decide whether
  the result is ready to commit.
- Grok is read-only. Use it for a material design/evidence question and, when
  warranted, one completed-change audit.
- Ox Alpha is the primary implementation seat while the free model remains
  available. It receives an exact bounded task, may edit and test, but never
  commits or pushes.
- A CLI failure, timeout, empty answer, or unavailable model is not a product
  defect. The controller retains the worktree and decides the smallest next
  action without asking the owner to relay prompts.

Routine mechanical fixes do not require Grok. Public API, trace semantics,
hardware behavior, evidence classification, or reconstruction claims do.

## Reusable question format

Write the question to `/tmp/libre99-model-question.md` using the available
file-editing tool. A useful compact shape is:

```text
Work read-only in the repository supplied as the current working directory.

QUESTION
[One bounded engineering or evidence question.]

CONTEXT
- Git HEAD: [derive from Git]
- affected behavior/consumer: [software, trace, frame, or reconstruction]
- proposed answer, if any: [short]

EVIDENCE
- primary source or documentation: [paths/pages, or none]
- reproduced execution/tests: [commands/results]
- implementation: [paths/symbols]
- contrary or uncertain evidence: [short]

Answer the question directly. Separate confirmed facts from inference. Point
to exact contrary evidence if you disagree. Do not edit files or invent a
workflow.
```

## Grok Build

Installed version when recorded: `grok 1.0.5`.

Repository-aware, read-only question:

```sh
grok --cwd "$(git rev-parse --show-toplevel)" \
  --prompt-file /tmp/libre99-model-question.md \
  --sandbox read-only \
  --tools Read,Glob,Grep,Bash \
  --disable-web-search \
  --no-subagents \
  --max-turns 16 \
  --reasoning-effort high \
  --output-format plain
```

For a simple question that needs no repository tools:

```sh
grok --cwd "$(git rev-parse --show-toplevel)" \
  --prompt-file /tmp/libre99-model-question.md \
  --sandbox read-only \
  --tools Read,Glob,Grep \
  --disable-web-search \
  --no-subagents \
  --max-turns 8 \
  --reasoning-effort high \
  --output-format plain
```

## Ox Alpha through OpenCode

Installed OpenCode version when recorded: `1.18.21`.

Ox Alpha is locally configured as `opencode-go/ox-alpha-free`. The provider's
advertised model list may not show this temporary model even while the local
configuration can invoke it. Use it opportunistically; a nonzero exit is a
tool/model-availability failure, not an engineering conclusion.

Operational note (2026-08-26): the documented identifier still routes, but
both the exact `build` boundary and a no-tools probe returned an immediate
provider-side `Unexpected server error`; `opencode models opencode-go` did not
advertise the temporary model. Preflight it before depending on it. Do not let
a fresh controller mistake this known availability condition for a repository
or task failure. The command below remains the intended boundary when the
provider restores the model.

Read-mostly advisory question using OpenCode's plan agent:

```sh
opencode run \
  --model opencode-go/ox-alpha-free \
  --variant high \
  --agent plan \
  --file /tmp/libre99-model-question.md \
  --title "Libre99 bounded engineering question" \
  "Read the attached question, inspect the repository only as needed, and answer it directly. Do not edit project files."
```

For machine-readable event output, add `--format json`. The default formatted
output is easier for a human or calling agent to read.

Do not add `--auto` for an advisory question. If OpenCode requests permission
to write, execute external commands, or leave the repository, decline it; the
question should be answered from read access and supplied evidence.

### Bounded implementation prompt

Write `/tmp/libre99-build-task.md` with the controller's file-editing tool:

```text
Work in the repository supplied as the current working directory.

Read AGENTS.md and START-HERE.md first. Derive Git state from the repository.

OBJECTIVE
[One observable software/reconstruction result.]

EVIDENCE AND CURRENT BEHAVIOR
[Primary paths, reproduced commands/results, and uncertainties.]

AUTHORIZED CHANGE
- Files or owning modules: [exact paths/areas]
- Required behavior: [bounded]
- Required focused tests: [bounded]

DO NOT
- Do not commit or push.
- Do not broaden scope, redesign unrelated code, or rewrite working code for style.
- Do not copy commercial media, derived traces, decompilations, or reference-emulator code.
- Do not provision tools or use another agent.

ACCEPTANCE
[Focused tests, workspace checks when proportionate, and the authentic replay the
controller will perform or verify.]

Implement the task and run the focused checks. Return changed paths, behavior, tests,
and any remaining uncertainty. Stop without speculative work if the objective cannot
be met inside the authorized change.
```

Invoke the installed build agent directly from the repository:

```sh
opencode run \
  --dir "$(git rev-parse --show-toplevel)" \
  --model opencode-go/ox-alpha-free \
  --variant high \
  --agent build \
  --file /tmp/libre99-build-task.md \
  --title "Libre99 bounded implementation" \
  "Implement the attached bounded task. Do not commit or push."
```

Before assigning a real task in a fresh controller session, run a no-write
availability probe:

```sh
opencode run \
  --dir "$(git rev-parse --show-toplevel)" \
  --model opencode-go/ox-alpha-free \
  --variant high \
  --agent build \
  --title "Libre99 Ox availability probe" \
  "Do not use tools or edit files. Reply with exactly OX_BUILD_READY."
```

Proceed only on exit `0` with a substantive response. If the provider is still
unavailable, Codex may complete design and Grok review, but must not begin a
large implementation on the assumption that Ox will return. Preserve the
bounded build prompt for retry or ask the owner to select another coding seat.

Do not use `--auto` by default. The controller must inspect `git status`, the
complete diff, and test output after Ox returns. Reject unrelated edits rather
than rationalizing them. If Ox is unavailable, preserve the task and report a
tooling failure; do not silently expand Codex work or create a relay.

### Completed-change audit

Use the reusable Grok question format, but make the question explicit:

```text
Audit this completed change read-only. Look only for software-visible defects,
unsupported evidence claims, regressions, or unnecessary architecture. Inspect the
full base-to-worktree diff and the named test results. Do not demand process artifacts,
style rewrites, speculative edge cases, or proof whose only consumer is another test.
Return PASS or concrete findings with path/symbol, reachable consumer, and reproduced
or source-backed evidence.
```

The controller evaluates the audit against primary evidence and real execution.
Model agreement does not authorize a behavior unsupported by either.

## Capturing and using an answer

The calling agent should retain the subprocess exit status and response text
in its task output or working notes. A CLI error, empty response, timeout, or
missing model is not disagreement. A substantive answer is advice:

- primary evidence still outranks every model;
- agreement does not raise an evidence label;
- disagreement should name a fact or assumption worth checking; and
- no additional agent round is needed unless the first answer exposes a real
  ambiguity.
