//! Durable orders and their inventory reservations.
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::catalog::{CatalogError, SqliteCatalog, insert_stock_movement, storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLine {
    pub product_id: i64,
    pub name: String,
    pub unit_price_cents: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub id: i64,
    pub status: OrderStatus,
    pub lines: Vec<OrderLine>,
    pub total_cents: i64,
    pub created_at: String,
    pub ended_at: Option<String>,
}

/// Called inside the shared schema migration transaction, after schema version 2.
pub(crate) fn migrate_v3(tx: &Transaction<'_>) -> Result<(), CatalogError> {
    tx.execute_batch(
        "CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            status TEXT NOT NULL CHECK(status IN ('open', 'closed', 'cancelled')),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            ended_at TEXT,
            CHECK ((status = 'open' AND ended_at IS NULL) OR (status <> 'open' AND ended_at IS NOT NULL))
        );
        CREATE UNIQUE INDEX orders_one_open ON orders(status) WHERE status = 'open';
        CREATE TABLE order_lines (
            order_id INTEGER NOT NULL REFERENCES orders(id),
            product_id INTEGER NOT NULL REFERENCES products(id),
            name TEXT NOT NULL,
            unit_price_cents INTEGER NOT NULL CHECK(unit_price_cents >= 0),
            quantity INTEGER NOT NULL CHECK(quantity > 0),
            PRIMARY KEY (order_id, product_id)
        );
        CREATE TABLE order_commands (
            request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 128),
            kind TEXT NOT NULL CHECK(kind IN ('add', 'remove', 'close', 'cancel')),
            order_id INTEGER NOT NULL REFERENCES orders(id),
            product_id INTEGER,
            quantity INTEGER,
            CHECK ((kind IN ('add', 'remove') AND product_id IS NOT NULL AND quantity > 0) OR
                   (kind IN ('close', 'cancel') AND product_id IS NULL AND quantity IS NULL))
        );
        PRAGMA user_version = 3;",
    ).map_err(storage)
}

fn validation(message: &str) -> CatalogError {
    CatalogError::Validation(message.to_owned())
}

#[derive(Clone, Copy)]
struct Command<'a> {
    request_id: &'a str,
    kind: &'static str,
    order_id: Option<i64>,
    product_id: Option<i64>,
    quantity: Option<i64>,
}

fn replay(tx: &Transaction<'_>, cmd: Command<'_>) -> Result<Option<i64>, CatalogError> {
    if cmd.request_id.trim() != cmd.request_id
        || cmd.request_id.is_empty()
        || cmd.request_id.len() > 128
    {
        return Err(validation(
            "Request ID must contain 1–128 non-whitespace characters.",
        ));
    }
    let previous: Option<(String, i64, Option<i64>, Option<i64>)> = tx
        .query_row(
            "SELECT kind, order_id, product_id, quantity FROM order_commands WHERE request_id = ?1",
            [cmd.request_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(storage)?;
    match previous {
        None => Ok(None),
        Some((kind, order_id, product_id, quantity)) => {
            if kind != cmd.kind
                || cmd.order_id.is_some_and(|id| id != order_id)
                || product_id != cmd.product_id
                || quantity != cmd.quantity
            {
                return Err(validation(
                    "Request ID was already used for a different operation.",
                ));
            }
            Ok(Some(order_id))
        }
    }
}

fn record(tx: &Transaction<'_>, cmd: Command<'_>, order_id: i64) -> Result<(), CatalogError> {
    tx.execute(
        "INSERT INTO order_commands (request_id, kind, order_id, product_id, quantity) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![cmd.request_id, cmd.kind, order_id, cmd.product_id, cmd.quantity],
    ).map_err(storage)?;
    Ok(())
}

fn read_order(conn: &rusqlite::Connection, id: i64) -> Result<Order, CatalogError> {
    let (status, created_at, ended_at): (String, String, Option<String>) = conn
        .query_row(
            "SELECT status, created_at, ended_at FROM orders WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(storage)?
        .ok_or_else(|| validation("Order does not exist."))?;
    let status = match status.as_str() {
        "open" => OrderStatus::Open,
        "closed" => OrderStatus::Closed,
        "cancelled" => OrderStatus::Cancelled,
        _ => return Err(CatalogError::Storage("Unknown order status.".into())),
    };
    let mut statement = conn.prepare("SELECT product_id, name, unit_price_cents, quantity FROM order_lines WHERE order_id = ?1 ORDER BY rowid").map_err(storage)?;
    let lines = statement
        .query_map([id], |row| {
            Ok(OrderLine {
                product_id: row.get(0)?,
                name: row.get(1)?,
                unit_price_cents: row.get(2)?,
                quantity: row.get(3)?,
            })
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    let total_cents = lines
        .iter()
        .try_fold(0_i64, |total, line| {
            line.unit_price_cents
                .checked_mul(line.quantity)
                .and_then(|amount| total.checked_add(amount))
        })
        .ok_or_else(|| CatalogError::Storage("Order total exceeds the supported range.".into()))?;
    Ok(Order {
        id,
        status,
        lines,
        total_cents,
        created_at,
        ended_at,
    })
}

fn ensure_open(tx: &Transaction<'_>, id: i64) -> Result<(), CatalogError> {
    let open: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM orders WHERE id = ?1 AND status = 'open')",
            [id],
            |row| row.get(0),
        )
        .map_err(storage)?;
    if open {
        Ok(())
    } else {
        Err(validation("Order is not open."))
    }
}

fn reserve_in_transaction(
    tx: &Transaction<'_>,
    cmd: Command<'_>,
    product_id: i64,
    quantity: i64,
) -> Result<i64, CatalogError> {
    if let Some(id) = replay(tx, cmd)? {
        return Ok(id);
    }
    let product: Option<(String, i64)> = tx
        .query_row(
            "SELECT name, price_cents FROM products WHERE id = ?1",
            [product_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?;
    let (name, price) = product.ok_or_else(|| validation("Product does not exist."))?;
    let id: Option<i64> = tx
        .query_row("SELECT id FROM orders WHERE status = 'open'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(storage)?;
    let id = if let Some(id) = id {
        id
    } else {
        tx.execute("INSERT INTO orders (status) VALUES ('open')", [])
            .map_err(storage)?;
        tx.last_insert_rowid()
    };
    let existing: Option<(i64, i64)> = tx.query_row("SELECT quantity, unit_price_cents FROM order_lines WHERE order_id = ?1 AND product_id = ?2", params![id, product_id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(storage)?;
    let (old_quantity, unit_price) = existing.unwrap_or((0, price));
    let new_quantity = old_quantity
        .checked_add(quantity)
        .ok_or_else(|| validation("Order quantity is too large."))?;
    let current_total = read_order(tx, id)?.total_cents;
    unit_price
        .checked_mul(quantity)
        .and_then(|amount| current_total.checked_add(amount))
        .ok_or_else(|| validation("Order total is too large."))?;
    insert_stock_movement(tx, product_id, -quantity, "order reservation", Some(id))?;
    tx.execute("INSERT INTO order_lines (order_id, product_id, name, unit_price_cents, quantity) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(order_id, product_id) DO UPDATE SET quantity = excluded.quantity", params![id, product_id, name, unit_price, new_quantity]).map_err(storage)?;
    record(tx, cmd, id)?;
    Ok(id)
}

impl SqliteCatalog {
    /// Loads the active draft with a consistent view of its lines.
    pub fn open_order(&self) -> Result<Option<Order>, CatalogError> {
        let mut conn = self.open()?;
        let tx = conn.transaction().map_err(storage)?;
        let id: Option<i64> = tx
            .query_row("SELECT id FROM orders WHERE status = 'open'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)?;
        let order = id.map(|id| read_order(&tx, id)).transpose()?;
        tx.commit().map_err(storage)?;
        Ok(order)
    }

    /// Returns closed and cancelled orders, newest first.
    pub fn order_history(&self) -> Result<Vec<Order>, CatalogError> {
        let mut conn = self.open()?;
        let tx = conn.transaction().map_err(storage)?;
        let ids = {
            let mut stmt = tx
                .prepare("SELECT id FROM orders WHERE status <> 'open' ORDER BY id DESC")
                .map_err(storage)?;
            stmt.query_map([], |r| r.get::<_, i64>(0))
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?
        };
        let orders = ids
            .into_iter()
            .map(|id| read_order(&tx, id))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(storage)?;
        Ok(orders)
    }

    /// Reserves stock immediately; replaying the same request returns the same order ID.
    pub fn add_to_order(
        &self,
        request_id: &str,
        product_id: i64,
        quantity: i64,
    ) -> Result<i64, CatalogError> {
        let cmd = Command {
            request_id,
            kind: "add",
            order_id: None,
            product_id: Some(product_id),
            quantity: Some(quantity),
        };
        if quantity <= 0 {
            return Err(validation("Quantity must be a positive whole number."));
        }
        let mut conn = self.open()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let id = reserve_in_transaction(&tx, cmd, product_id, quantity)?;
        tx.commit().map_err(storage)?;
        Ok(id)
    }

    /// Releases the specified quantity from an open order.
    pub fn remove_from_order(
        &self,
        request_id: &str,
        order_id: i64,
        product_id: i64,
        quantity: i64,
    ) -> Result<i64, CatalogError> {
        let cmd = Command {
            request_id,
            kind: "remove",
            order_id: Some(order_id),
            product_id: Some(product_id),
            quantity: Some(quantity),
        };
        if quantity <= 0 {
            return Err(validation("Quantity must be a positive whole number."));
        }
        let mut conn = self.open()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(id) = replay(&tx, cmd)? {
            return Ok(id);
        }
        ensure_open(&tx, order_id)?;
        let old_quantity: Option<i64> = tx
            .query_row(
                "SELECT quantity FROM order_lines WHERE order_id = ?1 AND product_id = ?2",
                params![order_id, product_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let old_quantity =
            old_quantity.ok_or_else(|| validation("Product is not in this order."))?;
        if quantity > old_quantity {
            return Err(validation("Cannot remove more than the ordered quantity."));
        }
        insert_stock_movement(&tx, product_id, quantity, "order release", Some(order_id))?;
        if quantity == old_quantity {
            tx.execute(
                "DELETE FROM order_lines WHERE order_id = ?1 AND product_id = ?2",
                params![order_id, product_id],
            )
            .map_err(storage)?;
        } else {
            tx.execute(
                "UPDATE order_lines SET quantity = ?3 WHERE order_id = ?1 AND product_id = ?2",
                params![order_id, product_id, old_quantity - quantity],
            )
            .map_err(storage)?;
        }
        record(&tx, cmd, order_id)?;
        tx.commit().map_err(storage)?;
        Ok(order_id)
    }

    /// Closes a nonempty order without changing the already reserved stock.
    pub fn close_order(&self, request_id: &str, order_id: i64) -> Result<i64, CatalogError> {
        self.finish_order(request_id, order_id, false)
    }

    /// Cancels an open order and releases every remaining reservation exactly once.
    pub fn cancel_order(&self, request_id: &str, order_id: i64) -> Result<i64, CatalogError> {
        self.finish_order(request_id, order_id, true)
    }

    fn finish_order(
        &self,
        request_id: &str,
        order_id: i64,
        cancel: bool,
    ) -> Result<i64, CatalogError> {
        let kind = if cancel { "cancel" } else { "close" };
        let cmd = Command {
            request_id,
            kind,
            order_id: Some(order_id),
            product_id: None,
            quantity: None,
        };
        let mut conn = self.open()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        if let Some(id) = replay(&tx, cmd)? {
            return Ok(id);
        }
        ensure_open(&tx, order_id)?;
        let order = read_order(&tx, order_id)?;
        if !cancel && order.lines.is_empty() {
            return Err(validation("Cannot close an empty order."));
        }
        if cancel {
            for line in &order.lines {
                insert_stock_movement(
                    &tx,
                    line.product_id,
                    line.quantity,
                    "order cancellation",
                    Some(order_id),
                )?;
            }
        }
        tx.execute("UPDATE orders SET status = ?1, ended_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2 AND status = 'open'", params![if cancel { "cancelled" } else { "closed" }, order_id]).map_err(storage)?;
        record(&tx, cmd, order_id)?;
        tx.commit().map_err(storage)?;
        Ok(order_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Draft};

    fn product(service: &SqliteCatalog, name: &str, price: &str, stock: i64) -> i64 {
        let product = service
            .add(Draft {
                name: name.into(),
                barcode: String::new(),
                price: price.into(),
            })
            .unwrap();
        if stock > 0 {
            service
                .adjust_stock(product.id, stock, "opening stock")
                .unwrap();
        }
        product.id
    }

    #[test]
    fn reserves_persistently_and_cancellation_releases_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let id = product(&service, "Tea", "12.50", 5);
        let order_id = service.add_to_order("add-1", id, 2).unwrap();
        assert_eq!(service.add_to_order("add-1", id, 2).unwrap(), order_id);
        assert!(service.add_to_order("add-1", id, 1).is_err());
        assert!(service.add_to_order("insufficient", id, 4).is_err());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 3);
        let reopened = SqliteCatalog::at(&path);
        let order = reopened.open_order().unwrap().unwrap();
        assert_eq!(order.id, order_id);
        assert_eq!(order.total_cents, 2500);
        assert_eq!(order.lines[0].quantity, 2);
        reopened
            .remove_from_order("remove-1", order_id, id, 1)
            .unwrap();
        reopened
            .remove_from_order("remove-1", order_id, id, 1)
            .unwrap();
        assert_eq!(reopened.list_stock().unwrap()[0].quantity, 4);
        reopened.cancel_order("cancel-1", order_id).unwrap();
        reopened.cancel_order("cancel-1", order_id).unwrap();
        assert!(reopened.cancel_order("cancel-2", order_id).is_err());
        assert_eq!(reopened.list_stock().unwrap()[0].quantity, 5);
        assert!(reopened.open_order().unwrap().is_none());
        assert_eq!(
            reopened.order_history().unwrap()[0].status,
            OrderStatus::Cancelled
        );
    }

    #[test]
    fn closing_keeps_reservation_and_is_replay_safe() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Coffee", "2.00", 4);
        let order_id = service.add_to_order("add", id, 3).unwrap();
        service.close_order("close", order_id).unwrap();
        service.close_order("close", order_id).unwrap();
        assert!(service.close_order("close-again", order_id).is_err());
        assert!(service.cancel_order("cancel", order_id).is_err());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 1);
        let history = service.order_history().unwrap();
        assert_eq!(history[0].status, OrderStatus::Closed);
        assert_eq!(history[0].total_cents, 600);
        assert!(service.open_order().unwrap().is_none());
        assert_eq!(service.add_to_order("add", id, 3).unwrap(), order_id);
        assert!(service.open_order().unwrap().is_none());
    }

    #[test]
    fn failures_leave_no_order_or_partial_movements() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Spice", "1", 1);
        assert!(service.add_to_order("none", id, 2).is_err());
        assert!(service.open_order().unwrap().is_none());
        let order_id = service.add_to_order("ok", id, 1).unwrap();
        assert!(
            service
                .remove_from_order("too-much", order_id, id, 2)
                .is_err()
        );
        assert!(service.add_to_order("zero", id, 0).is_err());
        assert!(service.add_to_order("", id, 1).is_err());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 0);
        assert_eq!(service.open_order().unwrap().unwrap().lines[0].quantity, 1);
        service
            .remove_from_order("remove", order_id, id, 1)
            .unwrap();
        assert!(service.close_order("empty", order_id).is_err());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 1);
    }

    #[test]
    fn preserves_line_price_when_catalog_price_changes() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Milk", "1.00", 3);
        service.add_to_order("first", id, 1).unwrap();
        service
            .open()
            .unwrap()
            .execute(
                "UPDATE products SET name = 'New milk', price_cents = 200 WHERE id = ?1",
                [id],
            )
            .unwrap();
        service.add_to_order("second", id, 1).unwrap();
        let order = service.open_order().unwrap().unwrap();
        assert_eq!(order.lines[0].name, "Milk");
        assert_eq!(order.lines[0].unit_price_cents, 100);
        assert_eq!(order.total_cents, 200);
    }
    #[test]
    fn command_record_failure_rolls_back_order_and_reservation() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Tea", "1.00", 3);
        service.open().unwrap().execute_batch(
            "CREATE TRIGGER reject_order_commands BEFORE INSERT ON order_commands BEGIN SELECT RAISE(ABORT, 'injected failure'); END;"
        ).unwrap();
        assert!(service.add_to_order("failure", id, 2).is_err());
        assert!(service.open_order().unwrap().is_none());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 3);
    }

    #[test]
    fn concurrent_reservations_cannot_oversell() {
        use std::sync::{Arc, Barrier};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let id = product(&service, "Tea", "1", 1);
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|key| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let service = SqliteCatalog::at(path);
                    barrier.wait();
                    service.add_to_order(key, id, 1)
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(service.list_stock().unwrap()[0].quantity, 0);
        assert_eq!(service.open_order().unwrap().unwrap().lines[0].quantity, 1);
    }
    #[test]
    fn rejects_total_overflow_without_reserving_more_stock() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Rare tea", "92233720368547758.07", 2);
        service.add_to_order("first", id, 1).unwrap();
        assert!(service.add_to_order("overflow", id, 1).is_err());
        assert_eq!(service.list_stock().unwrap()[0].quantity, 1);
        let order = service.open_order().unwrap().unwrap();
        assert_eq!(order.total_cents, i64::MAX);
        assert_eq!(order.lines[0].quantity, 1);
    }
    #[test]
    fn reservation_crash_worker() {
        use std::io::Write;
        let Ok(path) = std::env::var("FACTUROX_ORDER_CRASH_DB") else {
            return;
        };
        let service = SqliteCatalog::at(path);
        let mut conn = service.open().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let cmd = Command {
            request_id: "crash-add",
            kind: "add",
            order_id: None,
            product_id: Some(1),
            quantity: Some(2),
        };
        reserve_in_transaction(&tx, cmd, 1, 2).unwrap();
        if std::env::var_os("FACTUROX_ORDER_CRASH_COMMIT").is_some() {
            tx.commit().unwrap();
        }
        println!("ORDER_CRASH_READY");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn reservation_recovers_across_process_termination() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command as Process, Stdio},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let id = product(&service, "Tea", "1", 5);
        assert_eq!(id, 1);
        for committed in [false, true] {
            let mut command = Process::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "orders::tests::reservation_crash_worker",
                    "--nocapture",
                ])
                .env("FACTUROX_ORDER_CRASH_DB", &path)
                .stdout(Stdio::piped());
            if committed {
                command.env("FACTUROX_ORDER_CRASH_COMMIT", "1");
            }
            let mut child = command.spawn().unwrap();
            let mut reader = BufReader::new(child.stdout.take().unwrap());
            let mut ready = false;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line.contains("ORDER_CRASH_READY") {
                    ready = true;
                    break;
                }
            }
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(ready, "child failed before transaction boundary");
            let reopened = SqliteCatalog::at(&path);
            assert_eq!(
                reopened.list_stock().unwrap()[0].quantity,
                if committed { 3 } else { 5 }
            );
            let order = reopened.open_order().unwrap();
            if committed {
                let order = order.unwrap();
                assert_eq!(order.lines[0].quantity, 2);
                assert_eq!(reopened.add_to_order("crash-add", id, 2).unwrap(), order.id);
                assert_eq!(reopened.list_stock().unwrap()[0].quantity, 3);
            } else {
                assert!(order.is_none());
            }
        }
    }
}
