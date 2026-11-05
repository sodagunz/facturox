//! Public UI-neutral application interface; SQLite stays private.
#![allow(
    clippy::significant_drop_tightening,
    reason = "The transaction borrows the locked connection until commit or rollback; keep the guard scoped to the operation."
)]
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
use crate::domain::{Command, Event, EventEnvelope, Inventory};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub operation_id: String,

    pub command: Command,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub order: i64,
    pub events: Vec<Event>,
    pub envelopes: Vec<EventEnvelope>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Rejected(String),

    Uncertain(String),
}
/// Operator-entered details, not fiscal verification or Inventory events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompanyDetails {
    pub business_name: String,
    pub tax_identifier: String,
    pub address: String,
    pub contact: String,
}
fn validate_field(value: &str, field: &str, max_chars: usize) -> Result<String, Error> {
    let value = value.trim();
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        return Err(Error::Rejected(format!(
            "{field} must contain at most {max_chars} characters and no control characters."
        )));
    }
    Ok(value.to_owned())
}
/// Concrete local adapter: no generic repository or UI-specific service trait.
/// Clones share one connection and committed aggregate behind the same mutex.
#[derive(Clone)]
pub struct Backend {
    state: Arc<Mutex<BackendState>>,
}
struct BackendState {
    connection: SqliteConnection,
    inventory: Inventory,
    #[cfg(test)]
    reloads: usize,
}
fn uncertain(error: impl std::fmt::Display) -> Error {
    Error::Uncertain(error.to_string())
}
// A head comparison is valid for the append-only event log. In-place edits are
// unsupported; reopen or explicitly rebuild to audit the full event log.
async fn event_head(connection: &mut SqliteConnection) -> Result<i64, Error> {
    sqlx::query_scalar!(
        "SELECT COALESCE(MAX(\"order\"), 0) AS \"head!: i64\" FROM inventory_events"
    )
    .fetch_one(connection)
    .await
    .map_err(uncertain)
}
async fn replay(connection: &mut SqliteConnection) -> Result<Inventory, Error> {
    let rows = sqlx::query!(
        "SELECT \"order\", json(payload) AS \"payload!: String\", occurred_at, operation_id FROM inventory_events ORDER BY \"order\""
    )
    .fetch_all(connection)
    .await
    .map_err(uncertain)?;
    let mut inventory = Inventory::default();
    for row in rows {
        if inventory.order.checked_add(1) != Some(row.order) {
            return Err(uncertain("Noncontiguous inventory event order"));
        }
        let event: Event = serde_json::from_str(&row.payload).map_err(uncertain)?;
        if row.occurred_at < 0
            || row.operation_id.trim().is_empty()
            || matches!(&event, Event::OrderCreated(order) if order.created_at != row.occurred_at)
        {
            return Err(uncertain("Invalid event recording metadata"));
        }
        inventory.apply(&event).map_err(uncertain)?;
    }
    Ok(inventory)
}
async fn project(
    connection: &mut SqliteConnection,
    event: &Event,
    inventory: &Inventory,
) -> Result<(), Error> {
    match event {
        Event::ProductCreated(p) => {
            sqlx::query!(
                "INSERT INTO products(id,name,barcode,price,quantity) VALUES(?,?,?,?,0)",
                p.id,
                p.name,
                p.barcode,
                p.price
            )
            .execute(connection)
            .await
            .map_err(uncertain)?;
        }
        Event::ProductQuantityAdjusted { product_id, .. } => {
            let q = inventory
                .quantities
                .get(product_id)
                .ok_or_else(|| uncertain("Missing quantity"))?;
            sqlx::query!(
                "UPDATE products SET quantity=? WHERE id=?",
                q.quantity,
                product_id
            )
            .execute(connection)
            .await
            .map_err(uncertain)?;
        }
        Event::OrderCreated(order) => {
            for line in &order.lines {
                let q = inventory
                    .quantities
                    .get(&line.product_id)
                    .ok_or_else(|| uncertain("Missing quantity"))?;
                sqlx::query!(
                    "UPDATE products SET quantity=? WHERE id=?",
                    q.quantity,
                    line.product_id
                )
                .execute(&mut *connection)
                .await
                .map_err(uncertain)?;
            }
            let payload = serde_json::to_string(order).map_err(uncertain)?;
            sqlx::query!(
                "INSERT INTO orders(id,payload) VALUES(?,jsonb(?))",
                order.id,
                payload
            )
            .execute(connection)
            .await
            .map_err(uncertain)?;
        }
    }
    Ok(())
}
impl Backend {
    /// Open once at composition time using the platform data directory.
    /// # Errors
    /// Returns an error if the directory, database or event log cannot be opened.
    pub async fn open_default() -> Result<Self, Error> {
        let dirs = directories::ProjectDirs::from("", "", "facturox")
            .ok_or_else(|| uncertain("No platform data directory"))?;
        Self::open(dirs.data_dir().join("db.sqlite")).await
    }
    /// Open an explicit database, useful for isolated tests and maintenance.
    /// # Errors
    /// Returns an error for migration, event log or filesystem failures.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut connection = crate::database::open(path.as_ref())
            .await
            .map_err(uncertain)?;
        let mut tx = connection.begin().await.map_err(uncertain)?;
        let inventory = replay(&mut tx).await?;
        tx.commit().await.map_err(uncertain)?;
        Ok(Self {
            state: Arc::new(Mutex::new(BackendState {
                connection,
                inventory,
                #[cfg(test)]
                reloads: 0,
            })),
        })
    }
    /// Return a consistent event-derived snapshot.
    /// # Errors
    /// Fails safely on invalid event entries or unreadable storage.
    pub async fn snapshot(&self) -> Result<Inventory, Error> {
        let mut state = self.state.lock().await;
        let BackendState {
            connection,
            inventory,
            ..
        } = &mut *state;
        let mut tx = connection.begin().await.map_err(uncertain)?;
        let head = event_head(&mut tx).await?;
        let refreshed = if head == inventory.order {
            None
        } else {
            Some(replay(&mut tx).await?)
        };
        tx.commit().await.map_err(uncertain)?;
        if let Some(refreshed) = refreshed {
            state.inventory = refreshed;
            #[cfg(test)]
            {
                state.reloads += 1;
            }
        }
        Ok(state.inventory.clone())
    }
    /// Commit events, projections and exact-retry identity in one write transaction.
    /// # Errors
    /// Rejected outcomes are definite; uncertain outcomes require exact retry.
    pub async fn execute(&self, request: Request) -> Result<Receipt, Error> {
        if request.operation_id.trim().is_empty() {
            return Err(Error::Rejected("Missing operation id".into()));
        }
        let payload = serde_json::to_string(&request).map_err(uncertain)?;
        let mut state = self.state.lock().await;
        let BackendState {
            connection,
            inventory: cached,
            ..
        } = &mut *state;
        let mut tx = connection
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(uncertain)?;
        if let Some(previous) = sqlx::query!(
            "SELECT request, receipt FROM inventory_commands WHERE id=?",
            request.operation_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(uncertain)?
        {
            if previous.request != payload {
                return Err(Error::Rejected(
                    "Operation id reused with conflicting payload".into(),
                ));
            }
            return serde_json::from_str(&previous.receipt).map_err(uncertain);
        }
        let head = event_head(&mut tx).await?;
        let reloaded = head != cached.order;
        let mut inventory = if reloaded {
            replay(&mut tx).await?
        } else {
            cached.clone()
        };

        let occurred_at = sqlx::query_scalar!(
            "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER) AS \"occurred_at!: i64\""
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(uncertain)?;
        let events = inventory
            .decide(&request.command, occurred_at)
            .map_err(Error::Rejected)?;
        let mut envelopes = Vec::with_capacity(events.len());
        for event in &events {
            inventory.apply(event).map_err(uncertain)?;
            let json = serde_json::to_string(event).map_err(uncertain)?;
            sqlx::query!(
                "INSERT INTO inventory_events(\"order\",payload,occurred_at,operation_id) VALUES(?,jsonb(?),?,?)",
                inventory.order,
                json,
                occurred_at,
                request.operation_id
            )
            .execute(&mut *tx)
            .await
            .map_err(uncertain)?;
            project(&mut tx, event, &inventory).await?;
            envelopes.push(EventEnvelope {
                order: inventory.order,
                operation_id: request.operation_id.clone(),
                occurred_at,
                event: event.clone(),
            });
        }
        let receipt = Receipt {
            order: inventory.order,
            events,
            envelopes,
        };
        let json = serde_json::to_string(&receipt).map_err(uncertain)?;
        sqlx::query!(
            "INSERT INTO inventory_commands(id,request,receipt) VALUES(?,?,?)",
            request.operation_id,
            payload,
            json
        )
        .execute(&mut *tx)
        .await
        .map_err(uncertain)?;
        tx.commit().await.map_err(uncertain)?;
        state.inventory = inventory;
        #[cfg(test)]
        if reloaded {
            state.reloads += 1;
        }
        Ok(receipt)
    }
    /// Atomically regenerate disposable projections from the authoritative event log.
    /// # Errors
    /// Invalid event entries or projection errors roll back the entire rebuild.
    pub async fn rebuild_projections(&self) -> Result<(), Error> {
        let mut state = self.state.lock().await;
        let BackendState {
            connection,
            inventory: cached,
            ..
        } = &mut *state;
        let mut tx = connection
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(uncertain)?;
        let inventory = replay(&mut tx).await?;
        sqlx::query!("DELETE FROM orders")
            .execute(&mut *tx)
            .await
            .map_err(uncertain)?;
        sqlx::query!("DELETE FROM products")
            .execute(&mut *tx)
            .await
            .map_err(uncertain)?;
        for p in inventory.products.values() {
            let q = inventory
                .quantities
                .get(&p.id)
                .ok_or_else(|| uncertain("Missing quantity"))?;
            sqlx::query!(
                "INSERT INTO products(id,name,barcode,price,quantity) VALUES(?,?,?,?,?)",
                p.id,
                p.name,
                p.barcode,
                p.price,
                q.quantity
            )
            .execute(&mut *tx)
            .await
            .map_err(uncertain)?;
        }
        for order in inventory.orders.values() {
            let payload = serde_json::to_string(order).map_err(uncertain)?;
            sqlx::query!(
                "INSERT INTO orders(id,payload) VALUES(?,jsonb(?))",
                order.id,
                payload
            )
            .execute(&mut *tx)
            .await
            .map_err(uncertain)?;
        }
        tx.commit().await.map_err(uncertain)?;
        *cached = inventory;
        Ok(())
    }
    /// Read company details independently of the inventory event log.
    /// # Errors
    /// Returns storage failures or a missing settings record.
    pub async fn load_company_details(&self) -> Result<CompanyDetails, Error> {
        let mut state = self.state.lock().await;
        sqlx::query_as!(
            CompanyDetails,
            "SELECT business_name, tax_identifier, address, contact FROM app_settings WHERE id=1"
        )
        .fetch_one(&mut state.connection)
        .await
        .map_err(uncertain)
    }
    /// Validate and atomically save company details without changing appearance.
    /// # Errors
    /// Returns a definite validation rejection or an uncertain storage failure.
    pub async fn save_company_details(
        &self,
        details: &CompanyDetails,
    ) -> Result<CompanyDetails, Error> {
        let details = CompanyDetails {
            business_name: validate_field(&details.business_name, "Business name", 200)?,
            tax_identifier: validate_field(&details.tax_identifier, "Tax identifier", 64)?,
            address: validate_field(&details.address, "Address", 300)?,
            contact: validate_field(&details.contact, "Contact", 200)?,
        };
        let mut state = self.state.lock().await;
        let changed = sqlx::query!("UPDATE app_settings SET business_name=?, tax_identifier=?, address=?, contact=? WHERE id=1", details.business_name, details.tax_identifier, details.address, details.contact)
            .execute(&mut state.connection).await.map_err(uncertain)?.rows_affected();
        if changed != 1 {
            return Err(uncertain("Missing settings record"));
        }
        Ok(details)
    }
    /// Read appearance independently of the inventory event log.
    /// # Errors
    /// Returns storage failures.
    pub async fn dark_theme(&self) -> Result<bool, Error> {
        let mut state = self.state.lock().await;
        let c = &mut state.connection;
        let value = sqlx::query_scalar!("SELECT theme FROM app_settings WHERE id=1")
            .fetch_one(&mut *c)
            .await
            .map_err(uncertain)?;
        Ok(value == "dark")
    }
    /// Save appearance without creating domain events.
    /// # Errors
    /// Returns storage failures.
    pub async fn set_dark_theme(&self, dark: bool) -> Result<(), Error> {
        let mut state = self.state.lock().await;
        let c = &mut state.connection;
        let theme = if dark { "dark" } else { "light" };
        sqlx::query!("UPDATE app_settings SET theme=? WHERE id=1", theme)
            .execute(&mut *c)
            .await
            .map_err(uncertain)?;
        Ok(())
    }
}
