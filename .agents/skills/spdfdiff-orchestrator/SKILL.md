---
name: spdfdiff-orchestrator
description: Coordinate semantic-pdf-diff work across multiple agents or workstreams. Use when planning parallel implementation, assigning crate ownership, reviewing shared spdfdiff_types API changes, sequencing integration, resolving cross-crate conflicts, deciding vertical-slice versus compatibility-gate scope, or validating that specialist agent outputs can be merged safely.
---

# SPDFDiff Orchestrator

## Workflow

1. Read `AGENTS.md`, then read `references/orchestrator-plan.md`.
2. Identify the target milestone, workstream, and skill for each planned slice.
3. Assign one clear owner per crate or folder. Do not let two agents write the same files unless the task is explicitly a handoff.
4. Treat `crates/spdfdiff_types`, `AGENTS.md`, and plan files as shared-boundary surfaces that require orchestration.
5. Keep implementation slices small enough that `cargo fmt`, `cargo clippy`, and `cargo test` can run after each integration.
6. Merge or stack work in dependency order, starting with shared types and tests before downstream behavior.

## Parallel Assignment Rules

- Give each specialist a bounded write set and a matching repo-local skill.
- Tell each specialist they are not alone in the codebase and must not revert unrelated edits.
- Prefer mocked inputs for downstream crates while upstream parser/text work is still moving.
- Avoid parallel edits to `spdfdiff_types` unless one agent owns the API and others only review.
- Keep public IR changes paired with tests, snapshots when available, and plan/reference updates.

## Integration Rules

- Run the full gate after each integrated slice:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- Check that the slice truthfully labels its scope as `vertical-slice`, `compatibility-gate`, or `public-alpha`.
- Check that unsupported features produce stable diagnostics instead of broad compatibility claims.
- Check that reports remain deterministic and that default severity does not emit legal/business `Critical`.
- Update the relevant repo-local skill when changing workflow, crate boundaries, diagnostics, or verification requirements.

## Escalation

Pause and clarify before merging when:

- two agents need the same file ownership;
- a change broadens public compatibility claims;
- a shared IR field changes without tests or downstream updates;
- a specialist starts GUI, editor, OCR, renderer, or visual-diff work outside the current plan;
- Cargo gates fail and the fix requires cross-crate design decisions.
