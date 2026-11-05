# Deliver the persistent catalog slice

Add a product through Iced, list it and recover it after restarting the application.

Status: implementation in review; validation pending.
Dependencies: 02, 03, 04.

## Execution contract

Read `AGENTS.md` and `project-brief.md` in full before implementation. The user chose Iced and a future remotely hosted application-service boundary on 2026-11-04; these supersede the earlier UI-undecided wording. Initial execution remains one local process and SQLite. Fiscal integration, remote hosting, authentication, multi-machine operation and paid backup services are later work.

Execute only this ticket once implementation is requested. Record validation results and remaining limitations here. Move this file, keeping its name, to `.agents/plans/done/` only when all acceptance criteria pass; leave incomplete work in backlog.

## Steps

1. Implement the agreed catalog operations and SQLite adapter, migrations and per-user data path. Start with SQLite as the sole production backend.
2. Connect the Iced form/list to the service through the agreed operation boundary. Show validation and persistence errors clearly; report success only after commit.
3. Verify duplicate-barcode policy and repeated user actions. Keep entered form data available after failed saves and make retries follow the contract.
4. Test real SQLite files in temporary directories: create/migrate, add/list, close/reopen, rejected input and failed writes with no partial records. Use synthetic data only.
5. Exercise interrupted writes with a child process killed before and after the commit boundary; reopened data must match committed outcomes. Document disk-full/permission failure checks and their limits without risking user data.
6. Run the quality gates and native build matrix, then record Linux GUI smoke results and pending Windows/macOS manual checks. Document how to find and preserve local data.

## Acceptance criteria

- Adding a product, listing it and restarting preserves the committed product.
- Invalid/duplicate input follows ticket 03’s rules; failed writes never appear successful.
- Migration/open failures are visible and do not overwrite the database.
- Recovery checks demonstrate no partial product records after termination.
- This slice is catalog-only and does not claim readiness for real checkout or fiscal issuance. Invoicing remains through the operator’s existing Comprobantes en Línea workflow.

## Completion evidence

Linux: `cargo fmt --all`, strict pedantic Clippy and `cargo test --locked` passed. Five tests include real SQLite reopen, duplicate/invalid input, unsupported schema and child-process termination before/after commit. The app stayed running in an isolated Linux launch smoke check; interactive add/restart was not verified. Windows/macOS builds and GUI smoke tests are pending. Disk-full behavior and backup/restore have not been verified. Keep this ticket in backlog until the remaining acceptance checks are complete.
