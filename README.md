# Argentina Grocery POS

The project is in early development. The application is not ready for checkout or invoicing.

Install [devenv](https://devenv.sh/) and run `devenv shell`. The shell reads `rust-toolchain.toml` and supplies the pinned Rust nightly, rustfmt and Clippy. Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`, and `cargo test --locked` inside the shell.

CI builds on Linux x86-64, Windows x86-64 and macOS ARM64 using native runners. Build artifacts are unsigned executables. Compilation does not verify graphics or hardware; launch and exercise the app on each operating system before release. Development and initial smoke testing happen on Linux; test Windows and macOS natively when hardware is available. Cross-compilation is not required for CI.

Run `cargo deny --locked check advisories licenses sources` inside the devenv shell to check dependency advisories, licenses and sources. CI runs the same checks. Review any proposed advisory or license exception individually, record its reason and remove it when no longer needed; never suppress an entire class of findings.

The catalog stores its SQLite database in the current user's platform data directory under `facturox/catalog.sqlite3` (on Linux, normally `$XDG_DATA_HOME/facturox` or `~/.local/share/facturox`). The catalog is an early development slice, not a checkout or invoicing system. Keep the database on a local filesystem. A live SQLite database can have WAL files: do not copy just the database file while the application is running. A verified encrypted off-machine backup and restore procedure is required before production use.

The catalog rejects duplicate nonempty barcodes and stores prices as integer centavos. If the application stops after saving but before confirming the result, check the catalog before retrying. Disk-full and permission failures must be tested on isolated disposable storage; a failed save must be shown as an error. The automated recovery test kills a separate process around a SQLite commit, but does not simulate hardware failure or prove that a backup can be restored.

Stock and orders are an experimental local workflow. Existing products start with zero available units; select a product and record a signed adjustment with a reason to set stock. Adding an item to an open order immediately reserves one unit. Removing it or cancelling the order restores the reserved units; closing retains the reservation and records the order in the log as pending fiscal handling. The order log is not an invoice or payment record. After a restart, inspect the open order and log before repeating an action whose result was uncertain.

During development, SQLite schema is initialized from `src/schema.sql` without versioned migrations. Schema changes may require recreating the local development database; back up anything you want to keep first. Design and test a migration path before production use.
