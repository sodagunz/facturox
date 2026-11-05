# Stock and order workflow

Implement whole-unit inventory and one durable open order for the single shop computer. Adding an item immediately reserves stock; cancellation restores it. Closed orders are operational records, not fiscal documents or payment records. Read `project-brief.md` in full before implementation and retain its production and fiscal release gates.

## Decisions and invariants

- Stock shown is available stock. Existing catalog products start at zero. Positive and negative manual adjustments are attributed with a required reason; reject adjustments that make availability negative or overflow.
- Store stock movements durably. The available balance is derived from movements or transactionally maintained with them. Preserve the version 1 catalog during a transactional, forward-only SQLite migration; never discard an unknown newer schema.
- One open order survives restart. Each line snapshots product identity, name, unit price in centavos and a positive whole-unit quantity. Use checked integer arithmetic for quantities and totals.
- Each add or increase commits the order line and negative stock movement in the same SQLite transaction; reject insufficient stock without partial changes. Each removal or decrease commits the corresponding positive stock movement with its line change.
- Cancelling an open order atomically restores every reserved item, records the cancellation and cannot restore stock twice. Closing requires a nonempty open order, records it and does not deduct again. Closed and cancelled orders remain visible for audit.
- Use a stable request identifier persisted with each mutating command, so replay after a lost response returns the original result without applying the operation twice. Different intentional clicks get different IDs. The UI must preserve or reconcile an uncertain request before offering a retry.
- Keep UI-independent operations and SQLite details separate. Database work runs off the Iced event loop. Disable duplicate actions while pending, discard stale UI completions, show errors, and reload authoritative state when the outcome may be uncertain.
- Closed orders are shown as unbilled/pending fiscal handling; Comprobantes en Línea remains the existing manual invoicing route. Do not present orders as authorized invoices or implement payment/fiscal authorization in this slice.

## Sequence

1. Stock: migrate existing databases, show available quantities and add an adjustment action with a reason.
2. Orders: implement and test persistent open orders, stock reservations, cancellation, closure and replay-safe commands.
3. UI: add products to the open order, change/remove quantities, cancel or close it, and view order history and stock. Preserve keyboard access and visible errors.
4. Verify migration, insufficient stock, duplicate requests, cancellation twice, close twice, overflow, failed writes, restart and process termination before/after commit. Exercise the actual GUI path and record native-platform CI and smoke-test status. Leave unresolved production gates explicit.

## Completion evidence

Pending. A plan moves to `.agents/plans/done` only after all criteria and required validation pass.
