# Agent Instructions — Argentina Grocery POS

Read `project-brief.md` in full before proposing architecture, tools, or implementation. This project is currently in **architectural planning**, not authorized for broad code generation.

## Mission and agreed architecture

Build a **production-quality, maintainable single-computer grocery POS and inventory application** for an independent **monotributista** shop in Buenos Aires Province, Argentina. This is a free family pilot but processes real transactions. The developer works remotely.

**Decisions already made — do not reopen without a concrete reason:**
- **Rust throughout**, pinned **nightly** toolchain; manage development tools with **devenv**.
- **Standalone local desktop application**, **one shop computer / one writer initially**. **No mandatory backend server, hosted API, cloud database, SaaS, or deployment infrastructure.**
- **Embedded SQLite** is the default transactional store. Business logic runs locally and must not require a remote service.
- **UI framework undecided:** evaluate **Iced** versus a web frontend embedded through **Tauri** later; build and test backend/domain code first.
- Keep business/domain logic separate from UI, storage, fiscal network adapter and printing. Favor *small capability traits* and generics when there is an actual seam, e.g. `InvoiceService<B: InvoiceBackend>`; **do not build hypothetical LocalBackend/RemoteBackend implementations or a broad generic framework now**.
- ARCA **WSAA / WSFEv1** for electronic **Factura C**, ordinary thermal receipt printer; no fiscal controller unless later shown necessary.
- **No CAEA implementation**. During complete connectivity/ARCA outages, the operator plans to record pending transactions locally and invoice after recovery, accepting that this **does not meet contemporaneous fiscal issuance requirements**. Do not call this compliant contingency. Comprobantes en Línea is a fallback only while Internet/ARCA are usable.
- Remote component, if ever added, is **optional off-machine encrypted backup + modest telemetry**, strictly out of checkout's critical path. Backups are required for production; a hosted service is not required for the initial architecture.
- **GitHub Actions builds/tests/artifacts only, no deploy:** `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. Packaging/signing can be decided after UI selection.

## Mandatory engineering behavior
- **Do not rush into code or architecture:** answer the current design question at its level; introduce implementation detail only when requested.
- Use strict formatting, Clippy pedantic (review intentional lint exceptions), tests, and appropriate security checks. Pin toolchains and dependencies and keep CI and devenv consistent. Nightly features should be intentional, not gratuitous.
- Do not couple domain/application code to Axum, Tokio server runtime, Cloudflare Workers, remote SQL, or a particular UI. No HTTP API is required for an embedded application.
- Preserve durability: sale intent before ARCA calls, explicit fiscal state, authoritative reconciliation on ambiguous timeouts, immutable authorized documents, idempotent reprints, recoverable inventory movements.
- SQLite: design transactions and backups deliberately. Test recovery after sudden process termination, disk failure and restores. Avoid direct database sharing over network filesystems.
- **No production credentials, real tax identifiers, private keys, invoice data, or personal addresses** in source control or tests. Store secrets securely on the host; protect backups.
- **Never add functionality to hide sales or bypass fiscal records.** Separate non-fiscal printouts from authorized fiscal documents. Offline pending transaction != authorized invoice.
- Keep a clear export/migration route to a commercial POS.

## Regulatory discipline
- Cite and date official ARCA references when stating regulatory requirements, distinguishing verified text, interpretation and pending validation.
- Electronic duplicate/archive requirements under **RG 4290 art. 10 / RG 3685** are a major unresolved implementation/release gate; review the full technical annexes rather than assuming invoices stored as JSON suffice.
- The chosen no-CAEA policy is a **documented compliance gap during full outages**, not something to conceal or design around as if compliant. Flag it in launch decisions, runbooks, and operator documentation. Do not invent an alternative authorization mechanism.
- The actual taxpayer registration, invoice type, registered point of sale, certificates, legal retention, and printed document format must be verified before production use. Prefer official technical documentation to relying on unsupported accountant assumptions.

## Work style and first task
Work incrementally and ask only genuinely blocking questions. Prefer one small, testable end-to-end slice and describe failure modes before broad scaffolding. First refine the domain boundaries and SQLite persistence model **without choosing the desktop UI or introducing a server**. Avoid re-proposing cloud deployment, CAEA, microservices, generic repository frameworks or cross-platform installers unless asked.
