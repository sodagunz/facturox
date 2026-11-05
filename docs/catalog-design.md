# Inventory and local orders

Agreed architecture, 2026-11-05. This supersedes the earlier catalog/checkout design, not the regulatory release gates in `project-brief.md`.

One shop-wide `Inventory` aggregate owns products, whole-unit `ProductQuantity` balances and closed order entities. Orders are read history, not independent aggregate roots. This puts product identity, quantity checks and order deductions behind one consistency boundary. There are no persisted drafts, allocations or generic event framework.

`domain.rs` is pure Rust. `Command::validate` owns acceptance rules against Inventory. `Inventory::decide(command, occurred_at)` validates and produces events without mutation; `apply` checks and applies historical facts separately. The only commands are `CreateProduct`, `AdjustProductQuantity` and `CreateOrder`. The only events are `ProductCreated`, `ProductQuantityAdjusted` and `OrderCreated`. Creating a product with an opening quantity produces a two-event atomic batch. An order immediately closes and deducts exactly its positive line quantities; insufficient quantity rejects the entire command.

Names must be trimmed, unique (case-sensitive), contain 1–200 characters and no control characters. Optional barcodes must be trimmed, unique, contain 1–100 characters and no control characters. Prices and totals use checked signed 64-bit integer centavos. Balances cannot be negative. Adjustments have a nonzero signed delta and optional reason. Orders require nonempty, unique product lines and checked totals. Names/prices in order lines are immutable snapshots.

## Captured-price policy

The frontend's `Draft::add` captures name and unit price on the first add; later adds only increase quantity. Draft editing state belongs to `facturox-native`, not the backend. `Order::total()` derives its value from the lines with checked arithmetic; totals are not stored in order events. Checkout validates both values against the authoritative catalog inside the write transaction. Arbitrary client prices are rejected, never trusted or silently replaced. Catalog prices are currently immutable: no price-change command exists. If price editing is added, introduce a durable quote/version policy explicitly; do not silently substitute the new price in an existing draft.

## Application boundary

`Backend` is a concrete UI-neutral application service. Composition opens it once, then shares its handle with async tasks. Native code sees commands, requests, receipts and snapshots, not SQLite connections, paths or transaction mechanics. The startup snapshot feeds the frontend; successful commands return committed events that update affected local state without a full reload. If the frontend missed intervening events, it synchronizes automatically. Refresh and retry mechanics are not operator controls.

`Request` includes a stable operation ID and a command. The frontend retries an uncertain response once internally with the unchanged request; persistent uncertainty locks editing and shows an error. An exact retry returns the original receipt, even after reopening. Reusing its ID with any changed command is rejected. Commands are evaluated against current authoritative Inventory, not a caller-supplied revision. Validation failures publish nothing. In-memory pending commands and drafts are lost on process exit: inspect history before re-entering an operation with an uncertain outcome after restart. Durable recovery of frontend pending requests is not implemented.

The backend retains committed aggregate state in memory. Each command acquires SQLite's immediate write transaction and checks the `inventory_events` head. It replays only when another writer has advanced that head, then evaluates the command against current state. Event `order` values support replay ordering and cache synchronization; they are not schema versions or caller preconditions. Proposed changes are staged and replace committed memory only after commit succeeds. Startup replays the event log; durable snapshots are not implemented yet.

The `products` and `orders` read models are projected in the same transaction as event persistence. Projection handlers are currently internal; no configurable projector/listener framework is introduced yet. `inventory_commands` retains requests and receipts for deduplication.

Company details and appearance are separate transactional data, not Inventory events. No payment, ARCA, invoice authorization or printing implementation is included. A closed local order is pending fiscal handling, not an authorized invoice.
