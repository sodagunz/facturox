# Configure the Rust development baseline

Make the existing Cargo application reproducible with a pinned nightly shared by development and CI.

Status: planned.
Dependencies: None.

## Execution contract

Read `AGENTS.md` and `project-brief.md` in full before implementation. The user chose Iced and a future remotely hosted application-service boundary on 2026-11-04; these supersede the earlier UI-undecided wording. Initial execution remains one local process and SQLite. Fiscal integration, remote hosting, authentication, multi-machine operation and paid backup services are later work.

Execute only this ticket once implementation is requested. Record validation results and remaining limitations here. Move this file, keeping its name, to `.agents/plans/done/` only when all acceptance criteria pass; leave incomplete work in backlog.

## Steps

1. Inspect the existing Cargo package and devenv lock; preserve existing work rather than rerunning cargo init.
2. Select and verify a dated nightly available with rustfmt and Clippy on all three target platforms. Add rust-toolchain.toml with a minimal profile and required components. Nightly-only language features need an actual use case.
3. Configure devenv 2.4.0 to consume that file, checking its supported options rather than assuming newer devenv behavior. Remove sample greetings and unused template configuration.
4. Keep the smallest useful UI-neutral library and desktop entry point; justify any workspace split. Commit Cargo.lock for the application and configure ignored local data/build outputs.
5. Verify a fresh environment reports the selected nightly and builds/tests the existing program with locked dependencies.

## Acceptance criteria

- One dated toolchain is authoritative; devenv and rustup consumers agree.
- Formatting, compilation and tests work from the documented development entry point.
- Record actual versions and commands, including unavailable-platform checks as pending rather than passed.

## Completion evidence

Pending implementation.
