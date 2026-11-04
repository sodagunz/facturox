# Argentina Grocery POS — Codex Project Handoff

**Research baseline:** 2026-11-04 (Argentina regulatory requirements must be rechecked before production rollout and after relevant changes).  
**Project stage:** architecture / pre-implementation; no implementation or taxpayer authorization has yet been confirmed.
**Architecture decision date:** 2026-11-04 (local standalone application; no server/deployment).  
**Primary language:** English; preserve Spanish names of Argentine legal/fiscal concepts and original regulation numbers.

## 1. Executive summary

Build a **small, maintainable, production-safe point-of-sale (POS) and stock-management solution** for a new fine-grocery store (*almacén / tienda de delicatessen*) in **Buenos Aires Province, Argentina**. This is a **free pilot for a family business**, potentially a precursor to a more reusable product. It is not a disposable demo: fiscal compliance, data integrity, business continuity, recovery and eventual migration to an off-the-shelf POS are first-class requirements.

The shop's registered operator is the mother's **business associate, a natural person registered as *monotributista***. Treat this business and its taxpayer identity as **completely independent** of other family businesses (pasta shop, medical consultancy, etc.). The operator's ARCA tax identifier should be checked as a **CUIT**; a user's prior description of a different CUIL must not be treated as verified tax/account information. **Do not put real tax IDs, passwords or production credentials in the repo.**

Developer is an Argentine citizen living outside Argentina, building/supporting the system remotely. In-person ARCA steps and physical hardware setup should be minimized, with the operator/accountant/local technician handling those that cannot be avoided.

**Preferred fiscal strategy (subject to validation):** issue electronic **Factura C** using ARCA's **WSFEv1** web service and print a compliant representation on an **ordinary thermal receipt printer**, avoiding a homologated fiscal controller. Stock management and operational printing remain local. **Never represent an unapproved fiscal document as an authorized invoice.**

## 2. Non-negotiable goals

1. **No deliberate fiscal noncompliance**: every completed sale requiring documentation must enter the appropriate legal issuance flow, regardless of cash/card payment method. A separate non-fiscal printer may print orders, internal tickets or copies, not serve as a hidden unreported sales ledger.
2. **No silent loss or duplication of fiscal transactions**: persist intent before network calls; handle uncertain ARCA outcomes via authoritative lookups and deterministic reconciliation.
3. **Local operation and explicit failure limits**: application business functions use an embedded local database; if the app fails but Internet/ARCA work, the operator can use Comprobantes en Línea. During full Internet/ARCA outages, locally recorded transactions await later invoicing: this is an **explicitly accepted compliance gap**, not lawful offline fiscal issuance. No claim of uninterrupted legally compliant sales during those outages.
4. **Minimal maintenance**: avoid bespoke fiscal rules where ARCA-supported mechanisms or a simpler design suffice; isolate the fiscal adapter, track changes, favor dependable operational simplicity over ambitious features.
5. **Portable data and planned exit**: the operator must be able to migrate products, stock, sales, invoices and customer/vendor data to commercial software if the pilot stops.
6. **Production-quality engineering**: auditable state transitions, off-machine backups, tested restores, least-privilege credentials, modest monitoring and runbooks.
7. **Local-first, single-machine footprint**: standalone Rust desktop executable and SQLite, no required server or cloud backend; keep future adapters possible but do not prebuild them.

## 3. What official ARCA sources establish (checked 2026-11-04)

### 3.1 Electronic invoicing versus fiscal controllers

- **RG 4290/2018, articles 6–7 as amended by RG 5893/2026**: taxpayers including persons in the simplified regime (*monotributistas*) may use electronic invoices, fiscal controllers, or both, without advance notification of that choice. RG 5893 took effect **2026-11-01**.
- ARCA's Monotributo guidance: normally issue **type C** electronic invoices (*Factura C*), except transactions requiring another class such as exports. The shop's actual registered activity and status must be verified with its accountant.
- **Conclusion, not a blanket ruling:** for this shop, **no fiscal controller appears generally mandatory merely because it is a retail grocery or because the operator is monotributista**. Use of an ordinary printer for the rendering of an authorized electronic invoice is viable. Verify specific circumstances before go-live.

Sources:
- RG 5893: https://biblioteca.arca.gob.ar/search/query/norma.aspx?p=t:RAG%7Cn:5893%7Co:9%7Ca:2026%7Cf:27/08/2026
- Consolidated RG 4290: https://biblioteca.arca.gob.ar/search/query/norma.aspx?p=t:RAG%7Cn:4290%7Co:3%7Ca:2018%7Cf:02/08/2018
- Monotributo invoicing: https://arca.gob.ar/monotributo/ayuda/facturacion.asp
- ARCA FAQ: https://www.arca.gob.ar/facturacion/monotributo/comprobantes.asp

### 3.2 Electronic duplicate/archive obligations — important release gate

- **RG 4290 article 10** explicitly obliges web-service electronic invoice issuers making consumer sales in activities listed in its annex to issue and archive **electronic duplicates** according to relevant chapters of **RG 3685**. Article 11 provides possible consequences for noncompliance, including being required to use a new-technology fiscal controller.
- The annex includes grocery-related retail categories, so **the shop is likely affected**, subject to its actual activity/classification and operations.
- **Do not assume that simply storing POS JSON or PDF copies meets RG 3685.** Research required file structure, generation, storage, retention, backups, and any submission or informational obligations; verify the official technical annexes and obtain targeted clarification on any unresolved legal applicability before launch.
- This is a **must-resolve legal and technical gate**, not a later nice-to-have.

Source: consolidated RG 4290 above, especially arts. 10–11 and annex; inspect RG 3685 and its amendments directly.

### 3.3 ARCA API and authentication

- Official SOAP invoice web service: **WSFEv1**; document type **C** is supported.
- Authentication/authorization: **WSAA** with X.509 certificates; testing and production have separate configuration, certificates and delegated access. Production certificate and service authorization are administered through ARCA systems by the taxpayer or properly authorized representative.
- Test in homologación before using taxpayer production credentials.
- For ambiguous API timeouts, ARCA documents **FECompConsultar** (query a document) and **FECompUltimoAutorizado** (last authorized document). Never blindly retry as though no invoice was issued.
- Document output must comply with applicable electronic invoice content requirements, including **QR** (RG 4892/2020, applicable to monotributistas). Verify specific fields, types, numbering and customer identification thresholds against current official documentation.

Sources:
- WSFEv1 manuals/downloads: https://www.arca.gob.ar/ws/documentacion/ws-factura-electronica.asp
- WSAA: https://www.arca.gob.ar/ws/documentacion/wsaa.asp
- Certificates: https://www.arca.gob.ar/ws/documentacion/certificados.asp
- Official WSFEv1 developer manual: https://www.arca.gob.ar/fe/ayuda/documentos/wsfev1-RG-4291.pdf
- QR applicability: https://www.arca.gob.ar/fe/qr/vigencia-y-aplicacion.asp

### 3.4 Contingency and outages — consciously limited scope

- ARCA documents **CAEA** as a contingency mechanism for WSFE users, including certain Factura C workflows, subject to registration, issuance/reporting and exceptional-use rules. **The project has explicitly decided NOT to implement CAEA.** This is a scope decision, not a claim that the mechanism is unavailable.
- **Comprobantes en Línea** (with appropriate separate registered point of sale) is the human fallback when the **application** is down but Internet and ARCA are functioning.
- If the shop loses Internet connectivity or ARCA is unavailable, the chosen business workflow is to **record transactions locally and submit invoices after access returns**. This **does not fulfill the ordinary requirement to issue/make available a fiscal document at the retail sale**. Customer indifference to immediate invoices does not remove the obligation. Label pending records accurately; do not print them as valid invoices.
- The owner has knowingly accepted this **narrow outage compliance gap** to avoid CAEA complexity. Keep this risk documented in a launch register and runbook; do not silently reinterpret it as permitted or recommend concealing the delay. Reassess if outages become frequent or the business requirements change.
- Internet is necessary for *online fiscal authorization*, not for using the local inventory database or preparing/recording an order.

Sources:
- ARCA contingency guidance: https://arca.gob.ar/fe/emision-autorizacion/contingencias.asp
- RG 5782: https://biblioteca.arca.gob.ar/search/query/norma.aspx?p=t:RAG%7Cn:5782%7Co:9%7Ca:2025%7Cf:28/10/2025
- Current WSFEv1 documentation: https://www.arca.gob.ar/ws/documentacion/ws-factura-electronica.asp

### 3.5 Taxpayer versus developer duties

- No general requirement identified to homologate **ordinary POS/inventory application software** as fiscal hardware. Do not extrapolate this to manufacturing/integrating physical fiscal controllers.
- The **taxpayer** owns the fiscal registration, permissions, filing obligations and statutory compliance, even if software automates processes.
- Nonresident development appears technically possible. Separate cross-border tax/commercial questions are outside the project scope; it is a free family pilot, but avoid treating that as a legal determination.
- ARBA/Ingresos Brutos and local regulations may create separate obligations; do not assume federal ARCA invoicing handles provincial requirements.

## 4. Legal / regulatory open questions (block production launch)

Verify these against published ARCA technical and legal documentation, with operator confirmation or professional assistance only where needed; document source/date:

- [ ] Confirm **actual CUIT**, active Monotributo status, precise registered **activity codes**, fiscal and shop addresses; confirm the shop may invoice under that taxpayer and correct tax identity.
- [ ] Confirm electronic Factura C via **WSFEv1** is sufficient for the shop's sales and that **no business-specific legal requirement** forces controller usage.
- [ ] Determine whether **RG 4290 art. 10 / RG 3685** electronic duplicate regime applies, and specify exactly what software must generate, retain, secure and/or submit.
- [ ] Register and verify **electronic invoicing point of sale(s)**; define the separate Comprobantes en Línea fallback point if required. No CAEA point is planned.
- [ ] Document the **operational and legal limitations** for (a) custom app down, (b) ARCA WS down, (c) all connectivity down and (d) printer down. **No CAEA implementation**; delayed invoicing in a total outage remains an acknowledged compliance gap.
- [ ] Confirm invoice presentation/delivery/QR rules and consumer identification thresholds, especially cash sales and returns.
- [ ] Confirm retention/archiving periods, secure backup obligations, any privacy/personally-identifiable information obligations, and accountant export needs.
- [ ] Verify Monotributo **revenue/category/product/pricing limits** and possible future transition to Responsable Inscripto. Keep invoice classes flexible, but do not prebuild a full RI suite.
- [ ] Confirm provincial ARBA/Ingresos Brutos treatment and any required point-of-sale data exports.

**Avoid storing CUIT, CUIL, certificates, private keys, personal addresses or real sales in documentation/examples.**

## 5. Proposed domain boundaries (design proposal; not yet a decision)

Concepts:
- `Product`, `SKU/Barcode`, `Price`, `TaxClassification` (configurable), `StockLocation`, `StockMovement`, `StockAdjustment`.
- `Basket` / `Order` (can be draft or abandoned); `Sale` (business transaction); `Payment` and `Refund`.
- `FiscalDocument` (separate immutable record, document class/type, point of sale, number, request, response, CAE, issue/expiry dates, fiscal state, correlation/idempotency key).
- `PrintJob` (ordinary rendering or internal order print; retries cannot create new fiscal documents).
- `AuditEvent` and `Sync/ExportEvent`.

Do not collapse order, payment, stock mutation, invoice authorization and physical printing into one irreversible operation. Capture explicit compensations and operator workflows for errors, cancelled orders, returns and refunds. Accounting corrections should follow the appropriate credit-note process, not edits to previously authorized invoices.

### Confirmed deployment shape (2026-11-04)

```text
One shop computer (single writer initially)
┌────────────────────────────────────────────────────┐
│ Standalone Rust desktop app                         │
│  ├─ UI (Iced or Tauri: undecided)                   │
│  ├─ domain + application services (UI-neutral)      │
│  ├─ embedded SQLite + transaction/fiscal journal    │
│  ├─ ARCA WSAA/WSFEv1 adapter → Internet → ARCA      │
│  └─ receipt printer adapter → ordinary thermal      │
└────────────────────────────────────────────────────┘
              │ optional, asynchronous later
              └── off-machine encrypted backup / minimal telemetry
```

**A local Rust application is not an HTTP server.** Do not introduce Axum, a hosted backend, D1, a VPS, Cloudflare Workers, or an API layer without a demonstrated need. The operating system process hosts business logic and SQLite directly. ARCA remains an external HTTP/SOAP dependency.

- **Rust** is fixed, with a pinned **nightly** toolchain. **devenv** supplies the development environment and tools. CI enforces formatting, strict Clippy (including pedantic with reviewed exceptions), tests and appropriate security checks.
- **SQLite** is the initial single-machine transactional database; choose durable settings and safe backup/restore procedures. No multiple concurrent checkout hosts or network-filesystem DB sharing in the initial release.
- **UI later**: Iced (native Rust GUI) versus Tauri (Rust core with embedded web UI). Keep workflows behind UI-independent application services and do not commit to either yet. A small CLI/test harness may exercise backend functionality during early development.
- **Modularity later**: favor narrow capability traits and generic service dependencies only when warranted, e.g. `InvoiceService<B: InvoiceBackend>`; avoid a catch-all `Backend` trait and do not implement hypothetical local/remote alternatives up front. Local persistence and ARCA network integration are different concerns; the latter is inherently remote even in a local app.
- **Remote backup and telemetry**: optional future service, never required for checkout. **Off-machine backups themselves are required before production**, whether a managed service or simpler encrypted destination. Restoration must be tested.
- **Scalability boundary**: a second till, synchronized devices or multiple shops would require a new architectural decision; don't prematurely solve distributed consistency.

### GitHub Actions: build, do not deploy

Use CI for tests/lint and platform builds with downloadable artifacts only:

| Operating system | Rust target | GitHub runner (initial proposal) |
|---|---|---|
| Linux x86-64 | `x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-15` / ARM runner as available |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `windows-latest` |

Native runner builds are preferable initially to complex cross compilation. Choose actual packaging once the UI is selected. macOS notarization and Windows signing are separate release concerns; unsigned CI artifacts are sufficient for early testing. **No deployments, container registry, image publishing, or staging server.**

## 6. Fiscal state machine (illustrative; validate semantics)

```
DRAFT_ORDER
  -> SALE_RECORDED (durably persisted; invoice intent allocated)
  -> AUTHORIZATION_REQUESTED
       -> AUTHORIZED (CAE obtained and stored)
       -> UNCERTAIN (timeout/connection loss; query ARCA before retry)
       -> REJECTED (operator resolution needed)
  -> DOCUMENT_RENDERED / PRINTED / DELIVERED (independent operational state)
```

**CAEA is out of scope.** Never label locally stored, pending-authorization documents as authorized invoices. When checkout is recorded during an outage, display it as **unbilled / pending fiscal issuance**, with a recovery/reconciliation queue and an explicit warning that issuance was delayed and may be noncompliant.

**Safety properties / invariants**:
- Once ARCA has authorized a document, never issue a second document for the same intended fiscal transaction just because a response or print was lost.
- Never delete/overwrite an issued fiscal document.
- Never silently advance numbering or reuse a number without authoritative reconciliation.
- Every stock adjustment is attributable and recoverable; do not assume the fiscal event and inventory event commit atomically across systems.
- Every pending local transaction and delayed invoice must appear in a detectable reconciliation backlog; do not misidentify it as an authorized contingency invoice.

## 7. Operational continuity plan to design and rehearse

| Failure scenario | Response / explicit limitation |
|---|---|
| Thermal printer fails | Authorized invoice remains recorded; reprint/re-deliver without reissuing. |
| UI/app fails but Internet/ARCA function | Use independently configured Comprobantes en Línea; reconcile manually issued invoices after recovery. |
| WSFE times out / uncertain result | Query ARCA by point of sale, type and number; never blindly issue another invoice. |
| ARCA is unavailable or shop Internet is down | Record pending local operations, make fiscal status visibly pending, reconcile and invoice later. **This is not compliant timely invoicing and no lawful offline issuance is implemented.** |
| Shop computer or disk fails | Restore from off-machine encrypted backup; test recovery and identify possible recent data loss. |
| Developer unavailable | Document daily close, backup restore, fallback and CSV/export migration to commercial software. |

Rehearse process crash, printer unplugging, duplicate clicks, interrupted ARCA response and database restoration. Do not represent unbilled sales as valid Factura C documents.

## 8. Security and maintenance constraints

- Keep private keys/certificates out of source control; encrypt backups; secrets management appropriate to the actual deployment.
- Prefer narrow, revocable ARCA service permissions over shared Clave Fiscal passwords; protect personally identifying customer and sales data.
- Automated backups with routinely tested restore and offline/off-site copy; retention strategy should conform to legal archival requirements.
- Pin dependencies, log structured events (without secrets), monitor pending authorization/reconciliation, alert on certificate expiry and API schema/regulation changes.
- Avoid mandatory hosted dependencies; define recoverable manual processes.
- Create a `regulatory-register.md` later: law / effective date / operational consequence / test / owner / next review.
- Avoid implementing speculative tax/business logic, hidden cash-sale modes, automatic invoice-number guesswork or automatic legal rule changes without review.

## 9. Discovery and phased delivery

### Phase 0: architectural and regulatory prerequisites

1. Confirm registered taxpayer status, activity, electronic Factura C and appropriate registered point(s) of sale using published ARCA sources and actual account data.
2. Resolve RG 4290 art. 10 / RG 3685 electronic duplicate/archive **technical** obligations (formats, retention, storage) before production, rather than leaving implementation unspecified.
3. Confirm one computer/one operator, OS and receipt printer, and the operator's basic failure workflow.
4. Record the **no-CAEA** decision and its outage compliance gap explicitly; do not claim full compliance during total ARCA/Internet outages.
5. Define durable SQLite transaction semantics, backups/restores and initial exported data formats.

### Phase 1: local Rust core and testing harness

- Pinned Rust nightly and devenv; strict lint/test setup; GHA build-only matrix for Linux x86-64, Apple Silicon macOS and Windows x86-64.
- UI-neutral domain/application services, local SQLite persistence, inventory and sale/fiscal state model.
- A CLI or test harness can drive core use cases without choosing Iced or Tauri.

### Phase 2: fiscal integration, nonproduction

- WSAA/WSFEv1 homologación with Factura C authorization, lookup/read-back, retry/reconciliation, authorized invoice rendering/QR.
- Tests using mocked ARCA outcomes, incl. ambiguous timeouts and duplicate action prevention.
- Electronic-duplicate archive implementation once the applicable Annex V rules are checked; deterministic round-trip validation.

### Phase 3: operational desktop POS and go-live

- Choose Iced or Tauri based on UI and printing/device requirements; barcode/catalog/pricing/checkout/returns/thermal printer.
- Off-machine encrypted backups with tested restore, exports, operator runbooks, Comprobantes en Línea fallback.
- Test outage recovery with the **known non-compliant delayed-invoicing scenario documented**; do not silently call it a compliant contingency.
- Validate production authorization and regulated document output before using for real billing.

### Phase 4: pilot and scalability assessment

- Measure crashes, intervention time, unbilled backlog, fiscal reconciliation exceptions, backup restore reliability and operator friction.
- Decide continuation or migration; only then consider another workstation, a LAN service, remote backup/telemetry or a hosted backend.

## 10. Questions to ask the business before architecture decisions

1. When does the shop open? Is it operating and billing today? Which invoicing method is currently available?
2. Exact legally registered activity, and does the same monotributista operate any *other* businesses? (Do not import unrelated family businesses.)
3. Initially one checkout computer. What change would actually require a second register or multi-host inventory?
4. What hardware/OS is already purchased (printer, barcode scanner, cash drawer, PC/tablet)?
5. Average daily sales and baskets, typical peak throughput, frequency of internet outages, power reliability?
6. Will there be weighted goods, lot/batch tracking, expiration dates, promotions, mixed payments and returns?
7. What software currently handles tax filing, fiscal duplication/storage and accountant exports?
8. Who handles the shop physically when the developer is unavailable, and can they run a written fallback procedure?
9. Does the shop require invoices to be printed for every customer or can the document be delivered digitally when lawfully appropriate?
10. What are the maintenance budget and acceptable recovery objectives? Who maintains hardware and rotates certificates?

## 11. Explicit non-goals for initial iteration

- Fiscal controller hardware integration unless required after research.
- A second printer used for unreported cash sales.
- Integrating the family's other businesses or CUITs.
- Multi-tenant SaaS, multi-branch accounting, full ERP or payroll.
- Automated ARBA tax returns or general legal compliance advice.
- An unverified always-offline fiscal mode.
- Cloud hosting, HTTP server, microservices, event sourcing, message brokers or a specific frontend framework.
- CAEA issuance/reporting and offline fiscal authorization; total connectivity outages remain a known compliance gap.
- Shipping native installers, code signing or app-store distribution before UI selection.

## 12. First prompt for Codex

> Read `project-brief.md` and `AGENTS.md` in full. We are discussing architecture; **do not generate the entire application or choose the UI**. Respect the settled decisions: standalone local Rust desktop app, one computer, SQLite, pinned nightly + devenv, GitHub Actions **build-only** for Linux x86-64 / macOS ARM64 / Windows x86-64, no server or deployment, no CAEA. Propose only a **minimal domain and persistence boundary** for catalog/stock/sales/Factura C, keeping services modular through capability traits when useful without prebuilding remote backends. Highlight transaction invariants, ARCA unknown-outcome recovery, RG 3685 implementation verification, backup/restore, and the acknowledged outage compliance gap. Ask before committing to a new architectural direction.
