# Database development and recovery

SQLite persistence is private to `facturox-core`. The application opens one backend at startup. WAL, synchronous FULL, foreign keys and a five-second busy timeout are configured. Use a local filesystem, not network sharing.

## Inventory events and transactions

The authoritative `inventory_events` table is one contiguous Inventory event sequence. Each row records its `"order"`, command identifier, recording timestamp and JSONB payload. The `order` field is an event position, not a schema version; SQL must quote it because it is a keyword. Gaps, unknown event types and invalid payloads fail closed. No schema-version fields are stored.

`BEGIN IMMEDIATE` covers the event-head check, authoritative domain decision, event append, required projections and operation receipt. The backend stages changes in memory and publishes them only after commit succeeds. It replays when opening the database or detecting another writer's event position; normal commands use the committed in-memory aggregate. An order's timestamp is captured once by the backend and replayed as data, not recomputed.

Product creation and its optional opening ProductQuantity adjustment form one atomic event batch. `OrderCreated` records its line snapshots and directly deducts those quantities; there is no separate deduction event. `Backend::rebuild_projections` rebuilds catalog, quantities and orders atomically without changing operation deduplication records. Company details and appearance are stored separately from Inventory events.

Exact retries compare the serialized typed request (operation ID and command) and return the original stored receipt. Reusing an operation ID for changed contents is rejected. Storage/commit errors are conservatively uncertain. The frontend retries once with the identical request; if uncertainty persists, it retains the request and locks changes. Recovery is internal, not a manual retry protocol. A definite business rejection means the command was not committed; the UI preserves the draft for editing. Commands do not require a caller revision or a conflict-refresh handshake. A confirmed draft is cleared before automatic resynchronization after missing committed events. Failed synchronization leaves changes locked and shows a data error.

The event log is append-only through the application interface. Head checks assume old rows are not edited in place. Reopen or explicitly rebuild to validate the entire log after maintenance. The `products` and `orders` tables remain transactional, rebuildable read models, not additional sources of truth. `inventory_commands` stores submitted requests and receipts for exact-retry deduplication.

Both `inventory_events.payload` and `orders.payload` are SQLite JSONB BLOBs. Rust serializes JSON text, writes it through `jsonb(?)`, and reads event payloads through `json(payload)` before Serde deserialization. SQLite's binary encoding is opaque to Rust; it is not UTF-8 JSON bytes. Command requests and receipts remain JSON text.

For debugging in a SQLite shell:

```sql
SELECT "order", operation_id, json(payload)
FROM inventory_events
ORDER BY "order";

SELECT id, json_extract(payload, '$.lines') FROM orders;
```

SQL JSON functions accept the JSONB payload directly; `json(payload)` is useful when you want readable text.

## Application database

`Backend::open_default()` uses the platform application-data directory:

- Linux: `$XDG_DATA_HOME/facturox/db.sqlite`, normally `~/.local/share/facturox/db.sqlite`.
- macOS: `~/Library/Application Support/facturox/db.sqlite`.
- Windows: `%APPDATA%\facturox\data\db.sqlite`.

The initial migration was rewritten for the single-aggregate development reset. Previous schemas are intentionally incompatible; there is no automatic deletion or conversion. Before resetting any development database, stop every process that has it open and handle its `-wal` and `-shm` sidecars together. Preserve a verified backup if any data matters. **Never use this reset workflow on real transactions**; design and verify a migration or export instead.

Draft orders and pending frontend requests are memory-only. After a crash or restart, inspect order history before re-entering an operation whose result was uncertain.

## SQLx query validation

Local builds default to `SQLX_OFFLINE=false` in `.cargo/config.toml` and validate queries against the schema database. The devenv shell provides `sqlx` and `cargo sqlx`, and sets `DATABASE_URL` to the absolute path of `target/sqlx-schema.sqlite3`. It does not point at the application's database. CI platform builds override `SQLX_OFFLINE=true` to use checked-in `.sqlx/` metadata; the separate metadata-check job stays online. To regenerate metadata, enter the shell and recreate the disposable schema database from the workspace root:

```sh
devenv shell
mkdir -p target
rm -f target/sqlx-schema.sqlite3 target/sqlx-schema.sqlite3-wal target/sqlx-schema.sqlite3-shm
cargo sqlx database setup --source facturox-core/migrations
cargo sqlx prepare --workspace -- --all-targets
cargo sqlx prepare --check --workspace -- --all-targets
```

Commit the regenerated `.sqlx/` metadata with query changes. CI checks it against a fresh migrated database. Never prepare queries against shop data. SQLx 0.9 migrations use `run_direct` to avoid its generic `Acquire` lifetime limitation and keep backend opening Send for native tasks.

## Recovery validation

Tests cover pure decisions/replay, invalid quantities and prices, duplicate lines, atomic opening batches, projection rebuilds, projection and commit failure rollback, invalid-JSONB/event and sequence-gap rejection, exact retries after reopening, current-state validation, cache synchronization, settings and independent writers competing for the final available unit. A subprocess is killed before/after commit to verify that incomplete event writes disappear and committed batches remain retryable.

Tests do not simulate disk hardware failure, power loss on every filesystem, disk-full conditions or verified off-machine restore. These remain production rehearsal gates. Back up through SQLite's backup facility or a quiesced consistent copy including required WAL state; do not copy only a live database file. Protect backups and rehearse restores before production.
