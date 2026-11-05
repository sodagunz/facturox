# Build the Iced application shell

Establish explicit screen state and Elm-style updates while keeping business operations outside the UI.

Status: planned.
Dependencies: 01, 03; extend 02 as required by GUI dependencies.

## Execution contract

Read `AGENTS.md` and `project-brief.md` in full before implementation. The user chose Iced and a future remotely hosted application-service boundary on 2026-11-04; these supersede the earlier UI-undecided wording. Initial execution remains one local process and SQLite. Fiscal integration, remote hosting, authentication, multi-machine operation and paid backup services are later work.

Execute only this ticket once implementation is requested. Record validation results and remaining limitations here. Move this file, keeping its name, to `.agents/plans/done/` only when all acceptance criteria pass; leave incomplete work in backlog.

## Steps

1. Recheck latest stable Iced against official releases; planning baseline is 0.14.0, not development 0.15 documentation. Pin the selected dependency and lock resolution.
2. Implement model/state, Message, update and view using the selected version’s API. Use a small explicit screen enum and screen-local state/messages for catalog and one minimal navigation destination; avoid a routing framework.
3. Represent loading, ready, submission and failure states explicitly. Run application work through Iced tasks with owned inputs/results; keep SQLite and domain decisions out of update/view.
4. Define navigation during in-flight work and stale completion handling. Prevent stale responses from replacing newer state and disable repeat submissions while pending.
5. Add only meaningful pure update/navigation tests. Document required platform libraries/rendering setup and extend CI to build the actual desktop executable.

## Acceptance criteria

- The application launches on Linux and navigation preserves the intended state.
- Update behavior handles success/failure and stale completions deterministically.
- The desktop executable builds for the three supported targets; record native GUI smoke tests separately.
- No remote backend, fiscal UI or mock full POS is introduced.

## Completion evidence

Pending implementation.
