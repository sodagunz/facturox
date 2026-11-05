# Argentina Grocery POS

The project is in early development. The application is not ready for checkout or invoicing.

Install [devenv](https://devenv.sh/) and run `devenv shell`. The shell reads `rust-toolchain.toml` and supplies the pinned Rust nightly, rustfmt and Clippy. Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`, and `cargo test --locked` inside the shell.

CI builds on Linux x86-64, Windows x86-64 and macOS ARM64 using native runners. Build artifacts are unsigned executables. Compilation does not verify graphics or hardware; launch and exercise the app on each operating system before release. Development and initial smoke testing happen on Linux; test Windows and macOS natively when hardware is available. Cross-compilation is not required for CI.

CI checks dependency advisories, licenses and sources with `cargo-deny`. Review any proposed advisory or license exception individually, record its reason and remove it when no longer needed; never suppress an entire class of findings.

The catalog stores its SQLite database in the current user's platform data directory under `facturox/catalog.sqlite3` (on Linux, normally `$XDG_DATA_HOME/facturox` or `~/.local/share/facturox`). The catalog is an early development slice, not a checkout or invoicing system. Keep the database on a local filesystem. A live SQLite database can have WAL files: do not copy just the database file while the application is running. A verified encrypted off-machine backup and restore procedure is required before production use.

The catalog rejects duplicate nonempty barcodes and stores prices as integer centavos. If the application stops after saving but before confirming the result, check the catalog before retrying. Disk-full and permission failures must be tested on isolated disposable storage; a failed save must be shown as an error. The automated recovery test kills a separate process around a SQLite commit, but does not simulate hardware failure or prove that a backup can be restored.
