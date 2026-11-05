# Add quality gates and native platform builds

Catch Rust regressions on Linux, Windows and Apple Silicon without deployment infrastructure.

Status: planned.
Dependencies: 01.

## Execution contract

Read `AGENTS.md` and `project-brief.md` in full before implementation. The user chose Iced and a future remotely hosted application-service boundary on 2026-11-04; these supersede the earlier UI-undecided wording. Initial execution remains one local process and SQLite. Fiscal integration, remote hosting, authentication, multi-machine operation and paid backup services are later work.

Execute only this ticket once implementation is requested. Record validation results and remaining limitations here. Move this file, keeping its name, to `.agents/plans/done/` only when all acceptance criteria pass; leave incomplete work in backlog.

## Steps

1. Configure strict formatting and Clippy including pedantic, reviewing narrow exceptions with reasons. Add suitable dependency vulnerability/license checks and a documented exception policy.
2. Add GitHub Actions native builds/tests for x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc and aarch64-apple-darwin using ticket 01’s pinned toolchain and locked dependencies.
3. Verify runner architecture explicitly; choose current supported runner labels from official documentation. Pin third-party actions to reviewed revisions.
4. Upload executable build artifacts with target-identifying names. Keep packaging, signing and deployment outside this ticket.
5. Document Linux development and native Windows/macOS smoke testing. Treat cross-compilation as optional follow-up, with SDK/linker requirements investigated only if needed.

## Acceptance criteria

- CI checks formatting, strict linting, tests and dependency security with actionable failures.
- All three native target jobs are configured; record actual run evidence or explicitly pending remote execution.
- Build artifacts are clearly identified; GUI launch/hardware behavior is not claimed from compilation alone.

## Completion evidence

Pending implementation.
