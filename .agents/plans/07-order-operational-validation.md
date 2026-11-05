# Validate order operations across supported platforms

Before treating the stock and order slice as complete, validate it against the platform and recovery gates in `project-brief.md`.

1. Once GitHub Actions workflows are available on the default branch, run the Linux x86-64, Windows x86-64 and Apple Silicon macOS jobs and resolve failures. Record links and artifact limitations.
2. On native Windows and macOS desktops, exercise product creation, stock adjustment, add/remove, cancellation, closure, restart and order history. Record OS versions and results.
3. In disposable storage, test actual disk-full and permission-denied writes. Confirm that no partial reservation or false success occurs, and that refresh/retry is safe after an uncertain result.
4. Design and test an SQLite-aware encrypted off-machine backup and restore procedure, including database WAL handling. Record the demonstrated recovery point and unresolved legal archival requirements.

Move this plan to `.agents/plans/done` only when evidence for each criterion is recorded. Production checkout and fiscal issuance remain blocked on the release gates in the project brief.
