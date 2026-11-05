# Define the first service and persistence contract

Specify a small product-catalog operation boundary that can later be implemented by a remote service.

Status: planned.
Dependencies: 01.

## Execution contract

Read `AGENTS.md` and `project-brief.md` in full before implementation. The user chose Iced and a future remotely hosted application-service boundary on 2026-11-04; these supersede the earlier UI-undecided wording. Initial execution remains one local process and SQLite. Fiscal integration, remote hosting, authentication, multi-machine operation and paid backup services are later work.

Execute only this ticket once implementation is requested. Record validation results and remaining limitations here. Move this file, keeping its name, to `.agents/plans/done/` only when all acceptance criteria pass; leave incomplete work in backlog.

## Steps

1. Define add-product and list-products use cases, product identity, name, optional barcode and price semantics. Use exact monetary representation and explicit validation; settle duplicate barcode and ordering behavior.
2. Separate Iced state/messages/views from application operations and persistent storage. Use narrow capability traits only at actual seams. Application operations own validation and atomicity; callers receive owned UI-neutral results and meaningful errors rather than SQLite connections or transactions.
3. Record that a future remote backend may host application operations and persistence; auth, transport, remote implementations and distributed consistency remain deferred. Do not promise that changing a trait solves multi-machine operation.
4. Specify the initial SQLite schema, migration/version policy, per-user database path, single-writer ownership, connection lifecycle and deliberate durability settings. Explain how blocking database work stays off the GUI event loop.
5. Describe validation failures, duplicate submissions, unavailable/locked storage, full disk, migration failure and interrupted writes before implementation. Define what the UI may report as successfully saved.
6. Record decisions in a concise design note. Keep this ticket focused on contracts and persistence design; implement the vertical slice in ticket 05.

## Acceptance criteria

- Catalog operations, error behavior and transaction boundaries are explicit enough to implement and test.
- A future remote service can preserve operation semantics without exposing database internals to Iced.
- Storage initialization/migration failures preserve existing data and prevent false success.
- Scope is catalog only: no sale/fiscal journal or speculative repository framework.

## Completion evidence

Pending implementation.
