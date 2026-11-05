//! Durable orders and their inventory reservations.
use sqlx::{Connection, SqliteConnection};

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

type RecordedCommand = (String, i64, Option<i64>, Option<i64>);

async fn replay(
    conn: &mut SqliteConnection,
    cmd: Command<'_>,
) -> Result<Option<i64>, CatalogError> {
    if cmd.request_id.trim() != cmd.request_id
        || cmd.request_id.is_empty()
        || cmd.request_id.len() > 128
    {
        return Err(validation(
            "Request ID must contain 1–128 non-whitespace characters.",
        ));
    }
    let previous: Option<RecordedCommand> = sqlx::query_as(
        "SELECT kind, order_id, product_id, quantity FROM order_commands WHERE request_id = ?1",
    )
    .bind(cmd.request_id)
    .fetch_optional(conn)
    .await
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

async fn record(
    conn: &mut SqliteConnection,
    cmd: Command<'_>,
    order_id: i64,
) -> Result<(), CatalogError> {
    sqlx::query("INSERT INTO order_commands (request_id, kind, order_id, product_id, quantity) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(cmd.request_id).bind(cmd.kind).bind(order_id).bind(cmd.product_id).bind(cmd.quantity)
        .execute(conn).await.map_err(storage)?;
    Ok(())
}

async fn read_order(conn: &mut SqliteConnection, id: i64) -> Result<Order, CatalogError> {
    let (status, created_at, ended_at): (String, String, Option<String>) =
        sqlx::query_as("SELECT status, created_at, ended_at FROM orders WHERE id = ?1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(storage)?
            .ok_or_else(|| validation("Order does not exist."))?;
    let status = match status.as_str() {
        "open" => OrderStatus::Open,
        "closed" => OrderStatus::Closed,
        "cancelled" => OrderStatus::Cancelled,
        _ => return Err(CatalogError::storage_message("Unknown order status.")),
    };
    let lines: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT product_id, name, unit_price_cents, quantity FROM order_lines WHERE order_id = ?1 ORDER BY rowid"
    ).bind(id).fetch_all(conn).await.map_err(storage)?;
    let lines: Vec<OrderLine> = lines
        .into_iter()
        .map(|(product_id, name, unit_price_cents, quantity)| OrderLine {
            product_id,
            name,
            unit_price_cents,
            quantity,
        })
        .collect();
    let total_cents = lines
        .iter()
        .try_fold(0_i64, |total, line| {
            line.unit_price_cents
                .checked_mul(line.quantity)
                .and_then(|amount| total.checked_add(amount))
        })
        .ok_or_else(|| CatalogError::storage_message("Order total exceeds the supported range."))?;
    Ok(Order {
        id,
        status,
        lines,
        total_cents,
        created_at,
        ended_at,
    })
}

async fn ensure_open(conn: &mut SqliteConnection, id: i64) -> Result<(), CatalogError> {
    let open: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orders WHERE id = ?1 AND status = 'open')")
            .bind(id)
            .fetch_one(conn)
            .await
            .map_err(storage)?;
    if open {
        Ok(())
    } else {
        Err(validation("Order is not open."))
    }
}

async fn reserve_in_transaction(
    conn: &mut SqliteConnection,
    cmd: Command<'_>,
    product_id: i64,
    quantity: i64,
) -> Result<i64, CatalogError> {
    if let Some(id) = replay(conn, cmd).await? {
        return Ok(id);
    }
    let product: Option<(String, i64)> =
        sqlx::query_as("SELECT name, price_cents FROM products WHERE id = ?1")
            .bind(product_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(storage)?;
    let (name, price) = product.ok_or_else(|| validation("Product does not exist."))?;
    let existing_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM orders WHERE status = 'open'")
            .fetch_optional(&mut *conn)
            .await
            .map_err(storage)?;
    let id = if let Some(id) = existing_id {
        id
    } else {
        sqlx::query("INSERT INTO orders (status) VALUES ('open')")
            .execute(&mut *conn)
            .await
            .map_err(storage)?
            .last_insert_rowid()
    };
    let existing: Option<(i64, i64)> = sqlx::query_as("SELECT quantity, unit_price_cents FROM order_lines WHERE order_id = ?1 AND product_id = ?2")
        .bind(id).bind(product_id).fetch_optional(&mut *conn).await.map_err(storage)?;
    let (old_quantity, unit_price) = existing.unwrap_or((0, price));
    let new_quantity = old_quantity
        .checked_add(quantity)
        .ok_or_else(|| validation("Order quantity is too large."))?;
    let current_total = read_order(conn, id).await?.total_cents;
    unit_price
        .checked_mul(quantity)
        .and_then(|amount| current_total.checked_add(amount))
        .ok_or_else(|| validation("Order total is too large."))?;
    insert_stock_movement(conn, product_id, -quantity, "order reservation", Some(id)).await?;
    sqlx::query("INSERT INTO order_lines (order_id, product_id, name, unit_price_cents, quantity) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(order_id, product_id) DO UPDATE SET quantity = excluded.quantity")
        .bind(id).bind(product_id).bind(name).bind(unit_price).bind(new_quantity).execute(&mut *conn).await.map_err(storage)?;
    record(conn, cmd, id).await?;
    Ok(id)
}

impl SqliteCatalog {
    /// Loads the active draft with a consistent view of its lines.
    pub async fn open_order(&self) -> Result<Option<Order>, CatalogError> {
        let mut conn = self.open().await?;
        let mut tx = conn.begin().await.map_err(storage)?;
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM orders WHERE status = 'open'")
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?;
        let order = if let Some(id) = id {
            Some(read_order(&mut tx, id).await?)
        } else {
            None
        };
        tx.commit().await.map_err(storage)?;
        Ok(order)
    }

    /// Returns closed and cancelled orders, newest first.
    pub async fn order_history(&self) -> Result<Vec<Order>, CatalogError> {
        let mut conn = self.open().await?;
        let mut tx = conn.begin().await.map_err(storage)?;
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM orders WHERE status <> 'open' ORDER BY id DESC")
                .fetch_all(&mut *tx)
                .await
                .map_err(storage)?;
        let mut orders = Vec::with_capacity(ids.len());
        for id in ids {
            orders.push(read_order(&mut tx, id).await?);
        }
        tx.commit().await.map_err(storage)?;
        Ok(orders)
    }

    /// Reserves stock immediately; replaying the same request returns the same order ID.
    pub async fn add_to_order(
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
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        let id = reserve_in_transaction(&mut tx, cmd, product_id, quantity).await?;
        tx.commit().await.map_err(storage)?;
        Ok(id)
    }

    /// Releases the specified quantity from an open order.
    pub async fn remove_from_order(
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
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        if let Some(id) = replay(&mut tx, cmd).await? {
            return Ok(id);
        }
        ensure_open(&mut tx, order_id).await?;
        let old_quantity: Option<i64> = sqlx::query_scalar(
            "SELECT quantity FROM order_lines WHERE order_id = ?1 AND product_id = ?2",
        )
        .bind(order_id)
        .bind(product_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let old_quantity =
            old_quantity.ok_or_else(|| validation("Product is not in this order."))?;
        if quantity > old_quantity {
            return Err(validation("Cannot remove more than the ordered quantity."));
        }
        insert_stock_movement(
            &mut tx,
            product_id,
            quantity,
            "order release",
            Some(order_id),
        )
        .await?;
        if quantity == old_quantity {
            sqlx::query("DELETE FROM order_lines WHERE order_id = ?1 AND product_id = ?2")
                .bind(order_id)
                .bind(product_id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        } else {
            sqlx::query(
                "UPDATE order_lines SET quantity = ?3 WHERE order_id = ?1 AND product_id = ?2",
            )
            .bind(order_id)
            .bind(product_id)
            .bind(old_quantity - quantity)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        record(&mut tx, cmd, order_id).await?;
        tx.commit().await.map_err(storage)?;
        Ok(order_id)
    }

    /// Closes a nonempty order without changing the already reserved stock.
    pub async fn close_order(&self, request_id: &str, order_id: i64) -> Result<i64, CatalogError> {
        self.finish_order(request_id, order_id, false).await
    }

    /// Cancels an open order and releases every remaining reservation exactly once.
    pub async fn cancel_order(&self, request_id: &str, order_id: i64) -> Result<i64, CatalogError> {
        self.finish_order(request_id, order_id, true).await
    }

    async fn finish_order(
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
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        if let Some(id) = replay(&mut tx, cmd).await? {
            return Ok(id);
        }
        ensure_open(&mut tx, order_id).await?;
        let order = read_order(&mut tx, order_id).await?;
        if !cancel && order.lines.is_empty() {
            return Err(validation("Cannot close an empty order."));
        }
        if cancel {
            for line in &order.lines {
                insert_stock_movement(
                    &mut tx,
                    line.product_id,
                    line.quantity,
                    "order cancellation",
                    Some(order_id),
                )
                .await?;
            }
        }
        sqlx::query("UPDATE orders SET status = ?1, ended_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2 AND status = 'open'")
            .bind(if cancel { "cancelled" } else { "closed" }).bind(order_id).execute(&mut *tx).await.map_err(storage)?;
        record(&mut tx, cmd, order_id).await?;
        tx.commit().await.map_err(storage)?;
        Ok(order_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Draft};

    async fn product(service: &SqliteCatalog, name: &str, price: &str, stock: i64) -> i64 {
        let product = service
            .add_with_quantity(
                Draft {
                    name: name.into(),
                    barcode: String::new(),
                    price: price.into(),
                },
                0,
            )
            .await
            .unwrap();
        if stock > 0 {
            service
                .adjust_stock_once("fixture", product.id, stock, "opening stock")
                .await
                .unwrap();
        }
        product.id
    }

    #[tokio::test]
    async fn reserves_persistently_and_cancellation_releases_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let id = product(&service, "Tea", "12.50", 5).await;
        let order_id = service.add_to_order("add-1", id, 2).await.unwrap();
        assert_eq!(
            service.add_to_order("add-1", id, 2).await.unwrap(),
            order_id
        );
        assert!(service.add_to_order("add-1", id, 1).await.is_err());
        assert!(service.add_to_order("insufficient", id, 4).await.is_err());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 3);
        let reopened = SqliteCatalog::at(&path);
        let order = reopened.open_order().await.unwrap().unwrap();
        assert_eq!(order.id, order_id);
        assert_eq!(order.total_cents, 2500);
        assert_eq!(order.lines[0].quantity, 2);
        reopened
            .remove_from_order("remove-1", order_id, id, 1)
            .await
            .unwrap();
        reopened
            .remove_from_order("remove-1", order_id, id, 1)
            .await
            .unwrap();
        assert_eq!(reopened.list_stock().await.unwrap()[0].quantity, 4);
        reopened.cancel_order("cancel-1", order_id).await.unwrap();
        reopened.cancel_order("cancel-1", order_id).await.unwrap();
        assert!(reopened.cancel_order("cancel-2", order_id).await.is_err());
        assert_eq!(reopened.list_stock().await.unwrap()[0].quantity, 5);
        assert!(reopened.open_order().await.unwrap().is_none());
        assert_eq!(
            reopened.order_history().await.unwrap()[0].status,
            OrderStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn closing_keeps_reservation_and_is_replay_safe() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Coffee", "2.00", 4).await;
        let order_id = service.add_to_order("add", id, 3).await.unwrap();
        service.close_order("close", order_id).await.unwrap();
        service.close_order("close", order_id).await.unwrap();
        assert!(service.close_order("close-again", order_id).await.is_err());
        assert!(service.cancel_order("cancel", order_id).await.is_err());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 1);
        let history = service.order_history().await.unwrap();
        assert_eq!(history[0].status, OrderStatus::Closed);
        assert_eq!(history[0].total_cents, 600);
        assert!(service.open_order().await.unwrap().is_none());
        assert_eq!(service.add_to_order("add", id, 3).await.unwrap(), order_id);
        assert!(service.open_order().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn failures_leave_no_order_or_partial_movements() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Spice", "1", 1).await;
        assert!(service.add_to_order("none", id, 2).await.is_err());
        assert!(service.open_order().await.unwrap().is_none());
        let order_id = service.add_to_order("ok", id, 1).await.unwrap();
        assert!(
            service
                .remove_from_order("too-much", order_id, id, 2)
                .await
                .is_err()
        );
        assert!(service.add_to_order("zero", id, 0).await.is_err());
        assert!(service.add_to_order("", id, 1).await.is_err());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 0);
        assert_eq!(
            service.open_order().await.unwrap().unwrap().lines[0].quantity,
            1
        );
        service
            .remove_from_order("remove", order_id, id, 1)
            .await
            .unwrap();
        assert!(service.close_order("empty", order_id).await.is_err());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 1);
    }

    #[tokio::test]
    async fn preserves_line_price_when_catalog_price_changes() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Milk", "1.00", 3).await;
        service.add_to_order("first", id, 1).await.unwrap();
        sqlx::query("UPDATE products SET name = 'New milk', price_cents = 200 WHERE id = ?1")
            .bind(id)
            .execute(&mut service.open().await.unwrap())
            .await
            .unwrap();
        service.add_to_order("second", id, 1).await.unwrap();
        let order = service.open_order().await.unwrap().unwrap();
        assert_eq!(order.lines[0].name, "Milk");
        assert_eq!(order.lines[0].unit_price_cents, 100);
        assert_eq!(order.total_cents, 200);
    }

    #[tokio::test]
    async fn command_record_failure_rolls_back_order_and_reservation() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Tea", "1.00", 3).await;
        sqlx::query("CREATE TRIGGER reject_order_commands BEFORE INSERT ON order_commands BEGIN SELECT RAISE(ABORT, 'injected failure'); END;")
            .execute(&mut service.open().await.unwrap()).await.unwrap();
        assert!(service.add_to_order("failure", id, 2).await.is_err());
        assert!(service.open_order().await.unwrap().is_none());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 3);
    }

    #[tokio::test]
    async fn concurrent_reservations_cannot_oversell() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Tea", "1", 1).await;
        let (a, b) = tokio::join!(
            service.add_to_order("a", id, 1),
            service.add_to_order("b", id, 1)
        );
        assert_ne!(a.is_ok(), b.is_ok());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 0);
        assert_eq!(
            service.open_order().await.unwrap().unwrap().lines[0].quantity,
            1
        );
    }

    #[tokio::test]
    async fn rejects_total_overflow_without_reserving_more_stock() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let id = product(&service, "Rare tea", "92233720368547758.07", 2).await;
        service.add_to_order("first", id, 1).await.unwrap();
        assert!(service.add_to_order("overflow", id, 1).await.is_err());
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 1);
        let order = service.open_order().await.unwrap().unwrap();
        assert_eq!(order.total_cents, i64::MAX);
        assert_eq!(order.lines[0].quantity, 1);
    }

    #[tokio::test]
    async fn reservation_crash_worker() {
        use std::io::Write;
        let Ok(path) = std::env::var("FACTUROX_ORDER_CRASH_DB") else {
            return;
        };
        let service = SqliteCatalog::at(path);
        let mut conn = service.open().await.unwrap();
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let cmd = Command {
            request_id: "crash-add",
            kind: "add",
            order_id: None,
            product_id: Some(1),
            quantity: Some(2),
        };
        reserve_in_transaction(&mut tx, cmd, 1, 2).await.unwrap();
        if std::env::var_os("FACTUROX_ORDER_CRASH_COMMIT").is_some() {
            tx.commit().await.unwrap();
        }
        println!("ORDER_CRASH_READY");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[tokio::test]
    async fn reservation_recovers_across_process_termination() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command as Process, Stdio},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let id = product(&service, "Tea", "1", 5).await;
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
                reopened.list_stock().await.unwrap()[0].quantity,
                if committed { 3 } else { 5 }
            );
            let order = reopened.open_order().await.unwrap();
            if committed {
                let order = order.unwrap();
                assert_eq!(order.lines[0].quantity, 2);
                assert_eq!(
                    reopened.add_to_order("crash-add", id, 2).await.unwrap(),
                    order.id
                );
                assert_eq!(reopened.list_stock().await.unwrap()[0].quantity, 3);
            } else {
                assert!(order.is_none());
            }
        }
    }
}
