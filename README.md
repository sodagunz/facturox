# Argentina Grocery POS

Early development: not ready for real checkout or invoicing.

Install [devenv](https://devenv.sh/) and enter `devenv shell`. The shell supplies the pinned Rust nightly, rustfmt and Clippy.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p facturox-native
cargo deny --locked check advisories licenses sources
```

The native application opens one local backend and stores SQLite in the platform data directory under `facturox/db.sqlite`. Keep it on a local filesystem. A verified encrypted off-machine backup and restore procedure is required before production.

One Inventory aggregate owns products, quantities and closed local orders. Create products with unique trimmed names, optional unique barcodes, integer-centavo prices and optional opening quantities. Adjust quantities with signed whole-unit deltas and optional reasons. Draft adds and discard are purely local; **unfinished drafts are lost on exit**. Checkout immediately records a closed local order and deducts quantities atomically, or rejects the whole command. Captured first-add prices are validated, not silently repriced.

An uncertain command result is retried once internally with the identical request. If it remains uncertain, the app shows an error and locks changes rather than risking a duplicate operation. There are no manual refresh or retry controls. After restart, inspect history before re-entering an uncertain operation: pending frontend requests are not recovered across process exit. Commands are validated against current Inventory; callers do not supply revisions. Successful writes update the affected frontend state from committed events, without reloading everything.

Closed orders are **pending fiscal handling**, not payment records or authorized invoices. No ARCA integration is included. The regulatory release gates and the acknowledged total-outage compliance gap in `project-brief.md` still apply.

The rewritten initial migration is incompatible with the preceding prototype schema; the app never deletes an incompatible database. See [database development](docs/database.md) for reset precautions, recovery limitations and SQLx metadata preparation. See [Inventory design](docs/catalog-design.md) for boundaries and price/retry policies.

CI builds unsigned artifacts on Linux x86-64, Windows x86-64 and macOS ARM64. Compilation does not verify graphics or hardware; exercise the application on each platform before release. CI builds and checks only; it does not deploy a service.
