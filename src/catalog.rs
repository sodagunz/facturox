use std::{fmt, path::PathBuf, sync::Arc};

use color_eyre::eyre::{Report, eyre};

use directories::ProjectDirs;
use sqlx::{Connection, Row, SqliteConnection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub barcode: Option<String>,
    pub price_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBalance {
    pub product_id: i64,
    pub quantity: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub name: String,
    pub barcode: String,
    pub price: String,
}

#[derive(Debug, Clone)]
pub enum CatalogError {
    Validation(String),
    DuplicateBarcode,
    Storage(Arc<Report>),
}
impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => f.write_str(message),
            Self::Storage(report) => write!(f, "Catalog storage error: {report:#}"),
            Self::DuplicateBarcode => f.write_str("This barcode is already in use."),
        }
    }
}

impl CatalogError {
    pub(crate) fn storage_message(message: &'static str) -> Self {
        Self::Storage(Arc::new(eyre!(message)))
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(report) => Some(report.as_ref().as_ref()),
            _ => None,
        }
    }
}

pub trait Catalog {
    async fn add_with_quantity(
        &self,
        draft: Draft,
        initial_quantity: i64,
    ) -> Result<Product, CatalogError>;
    async fn list(&self) -> Result<Vec<Product>, CatalogError>;
}

#[derive(Debug, Clone)]
pub struct SqliteCatalog {
    path: PathBuf,
}
impl SqliteCatalog {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<PathBuf, CatalogError> {
        ProjectDirs::from("", "", "facturox")
            .map(|dirs| dirs.data_dir().join("catalog.sqlite3"))
            .ok_or_else(|| CatalogError::storage_message("Could not find the user data directory."))
    }

    pub(crate) async fn open(&self) -> Result<SqliteConnection, CatalogError> {
        crate::database::open(&self.path).await
    }

    /// Lists every product's available quantity, including products without movements.
    pub async fn list_stock(&self) -> Result<Vec<StockBalance>, CatalogError> {
        let mut conn = self.open().await?;
        let rows = sqlx::query("SELECT p.id, COALESCE(SUM(m.delta), 0) FROM products p LEFT JOIN stock_movements m ON m.product_id = p.id GROUP BY p.id ORDER BY p.id")
            .fetch_all(&mut conn).await.map_err(storage)?;
        rows.into_iter()
            .map(|row| {
                Ok(StockBalance {
                    product_id: row.try_get(0).map_err(storage)?,
                    quantity: row.try_get(1).map_err(storage)?,
                })
            })
            .collect()
    }

    /// Records a manual adjustment for test fixtures.
    #[cfg(test)]
    pub async fn adjust_stock(
        &self,
        product_id: i64,
        delta: i64,
        reason: &str,
    ) -> Result<i64, CatalogError> {
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        let quantity = insert_stock_movement(&mut tx, product_id, delta, reason, None).await?;
        tx.commit().await.map_err(storage)?;
        Ok(quantity)
    }

    /// Records a manual adjustment once for a stable caller-generated request ID.
    pub async fn adjust_stock_once(
        &self,
        request_id: &str,
        product_id: i64,
        delta: i64,
        reason: &str,
    ) -> Result<i64, CatalogError> {
        let reason = match reason.trim() {
            "" => "Manual adjustment",
            reason => reason,
        };
        if request_id.is_empty() || request_id.len() > 128 {
            return Err(CatalogError::Validation(
                "Invalid stock adjustment request ID.".into(),
            ));
        }
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        let existing: Option<(i64, i64, String, i64)> = sqlx::query_as("SELECT product_id, delta, reason, result_quantity FROM stock_adjustment_commands WHERE request_id = ?1")
            .bind(request_id).fetch_optional(&mut *tx).await.map_err(storage)?;
        if let Some((saved_product, saved_delta, saved_reason, quantity)) = existing {
            if (saved_product, saved_delta, saved_reason.as_str()) != (product_id, delta, reason) {
                return Err(CatalogError::Validation(
                    "Stock adjustment request ID was already used for a different adjustment."
                        .into(),
                ));
            }
            return Ok(quantity);
        }
        let quantity = insert_stock_movement(&mut tx, product_id, delta, reason, None).await?;
        sqlx::query("INSERT INTO stock_adjustment_commands (request_id, product_id, delta, reason, result_quantity) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(request_id).bind(product_id).bind(delta).bind(reason).bind(quantity).execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(quantity)
    }
}

pub(crate) fn storage(error: impl Into<Report>) -> CatalogError {
    CatalogError::Storage(Arc::new(error.into()))
}

/// Applies a movement inside a transaction that already holds the SQLite write lock.
pub(crate) async fn insert_stock_movement(
    conn: &mut SqliteConnection,
    product_id: i64,
    delta: i64,
    reason: &str,
    order_id: Option<i64>,
) -> Result<i64, CatalogError> {
    let reason = reason.trim();
    if delta == 0 || reason.is_empty() {
        return Err(CatalogError::Validation(
            "A nonzero quantity and reason are required.".into(),
        ));
    }
    let current: Option<i64> = sqlx::query_scalar("SELECT COALESCE((SELECT SUM(delta) FROM stock_movements WHERE product_id = p.id), 0) FROM products p WHERE p.id = ?1")
        .bind(product_id).fetch_optional(&mut *conn).await.map_err(storage)?;
    let current =
        current.ok_or_else(|| CatalogError::Validation("Product does not exist.".into()))?;
    let next = current
        .checked_add(delta)
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            CatalogError::Validation("Stock quantity would be negative or too large.".into())
        })?;
    sqlx::query(
        "INSERT INTO stock_movements (product_id, delta, reason, order_id) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(product_id)
    .bind(delta)
    .bind(reason)
    .bind(order_id)
    .execute(&mut *conn)
    .await
    .map_err(storage)?;
    Ok(next)
}

fn validate(draft: &Draft) -> Result<(String, Option<String>, i64), CatalogError> {
    let name = draft.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(CatalogError::Validation(
            "Name must contain 1–200 characters.".into(),
        ));
    }
    let barcode = draft.barcode.trim();
    if barcode.chars().count() > 100 {
        return Err(CatalogError::Validation(
            "Barcode must contain at most 100 characters.".into(),
        ));
    }
    let barcode = (!barcode.is_empty()).then(|| barcode.to_owned());
    let price = draft.price.trim();
    let (whole, fraction) = price.split_once('.').unwrap_or((price, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (price.contains('.') && fraction.is_empty())
    {
        return Err(CatalogError::Validation(
            "Enter a nonnegative price in pesos with up to two decimal places.".into(),
        ));
    }
    let cents = whole
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(100))
        .and_then(|n| {
            let fractional = match fraction.len() {
                0 => Some(0),
                1 => fraction.parse::<i64>().ok().map(|v| v * 10),
                _ => fraction.parse::<i64>().ok(),
            };
            fractional.and_then(|v| n.checked_add(v))
        })
        .ok_or_else(|| CatalogError::Validation("Price is too large.".into()))?;
    Ok((name, barcode, cents))
}
impl Catalog for SqliteCatalog {
    async fn add_with_quantity(
        &self,
        draft: Draft,
        initial_quantity: i64,
    ) -> Result<Product, CatalogError> {
        if initial_quantity < 0 {
            return Err(CatalogError::Validation(
                "Initial stock must be a nonnegative whole number.".into(),
            ));
        }
        let (name, barcode, price_cents) = validate(&draft)?;
        let mut conn = self.open().await?;
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(storage)?;
        let result =
            sqlx::query("INSERT INTO products (name, barcode, price_cents) VALUES (?1, ?2, ?3)")
                .bind(&name)
                .bind(&barcode)
                .bind(price_cents)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    if barcode.is_some()
                        && matches!(&error, sqlx::Error::Database(db) if db.is_unique_violation())
                    {
                        CatalogError::DuplicateBarcode
                    } else {
                        storage(error)
                    }
                })?;
        let id = result.last_insert_rowid();
        if initial_quantity > 0 {
            insert_stock_movement(&mut tx, id, initial_quantity, "Initial stock", None).await?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(Product {
            id,
            name,
            barcode,
            price_cents,
        })
    }
    async fn list(&self) -> Result<Vec<Product>, CatalogError> {
        let mut conn = self.open().await?;
        let rows = sqlx::query("SELECT id, name, barcode, price_cents FROM products ORDER BY id")
            .fetch_all(&mut conn)
            .await
            .map_err(storage)?;
        rows.into_iter()
            .map(|row| {
                Ok(Product {
                    id: row.try_get(0).map_err(storage)?,
                    name: row.try_get(1).map_err(storage)?,
                    barcode: row.try_get(2).map_err(storage)?,
                    price_cents: row.try_get(3).map_err(storage)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_errors_preserve_the_underlying_cause() {
        use std::error::Error;
        let error = storage(std::io::Error::other("simulated disk failure"));
        assert!(error.to_string().contains("simulated disk failure"));
        assert!(
            error
                .source()
                .is_some_and(|source| source.to_string().contains("simulated disk failure"))
        );
    }

    #[tokio::test]
    async fn initial_stock_is_saved_atomically_with_product() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let draft = Draft {
            name: "Tea".into(),
            barcode: String::new(),
            price: "2".into(),
        };
        assert!(matches!(
            service.add_with_quantity(draft.clone(), -1).await,
            Err(CatalogError::Validation(_))
        ));
        assert_eq!(service.list().await.unwrap(), []);
        let mut conn = service.open().await.unwrap();
        sqlx::query("CREATE TRIGGER reject_initial_stock BEFORE INSERT ON stock_movements BEGIN SELECT RAISE(ABORT, 'test failure'); END")
            .execute(&mut conn).await.unwrap();
        assert!(service.add_with_quantity(draft.clone(), 5).await.is_err());
        assert_eq!(service.list().await.unwrap(), []);
        sqlx::query("DROP TRIGGER reject_initial_stock")
            .execute(&mut conn)
            .await
            .unwrap();
        drop(conn);
        let product = service.add_with_quantity(draft, 5).await.unwrap();
        let reopened = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        assert_eq!(
            reopened.list_stock().await.unwrap(),
            vec![StockBalance {
                product_id: product.id,
                quantity: 5
            }]
        );
        let mut conn = reopened.open().await.unwrap();
        let movements: Vec<(i64, String)> =
            sqlx::query_as("SELECT delta, reason FROM stock_movements WHERE product_id = ?1")
                .bind(product.id)
                .fetch_all(&mut conn)
                .await
                .unwrap();
        assert_eq!(movements, vec![(5, "Initial stock".into())]);
    }

    #[tokio::test]
    async fn empty_adjustment_reason_is_recorded_and_replay_safe() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let product = service
            .add_with_quantity(
                Draft {
                    name: "Coffee".into(),
                    barcode: String::new(),
                    price: "1".into(),
                },
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .adjust_stock_once("request", product.id, 3, " ")
                .await
                .unwrap(),
            3
        );
        assert_eq!(
            service
                .adjust_stock_once("request", product.id, 3, "")
                .await
                .unwrap(),
            3
        );
        let mut conn = service.open().await.unwrap();
        let movements: Vec<(i64, String)> =
            sqlx::query_as("SELECT delta, reason FROM stock_movements WHERE product_id = ?1")
                .bind(product.id)
                .fetch_all(&mut conn)
                .await
                .unwrap();
        assert_eq!(movements, vec![(3, "Manual adjustment".into())]);
    }

    #[tokio::test]
    async fn stock_adjustment_replay_is_atomic_and_rejects_reused_ids() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let product = service
            .add_with_quantity(
                Draft {
                    name: "Tea".into(),
                    barcode: String::new(),
                    price: "1".into(),
                },
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .adjust_stock_once("request-1", product.id, 3, "received")
                .await
                .unwrap(),
            3
        );
        assert_eq!(
            service
                .adjust_stock_once("request-2", product.id, -1, "damaged")
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            service
                .adjust_stock_once("request-1", product.id, 3, "received")
                .await
                .unwrap(),
            3
        );
        assert!(
            service
                .adjust_stock_once("request-1", product.id, 4, "received")
                .await
                .is_err()
        );
        assert!(
            service
                .adjust_stock_once("request-1", product.id, 3, "different reason")
                .await
                .is_err()
        );
        assert!(
            service
                .adjust_stock_once("", product.id, 3, "received")
                .await
                .is_err()
        );
        assert!(
            service
                .adjust_stock_once("request-3", product.id, -3, "too many")
                .await
                .is_err()
        );
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 2);
        let mut conn = service.open().await.unwrap();
        let movement_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements")
            .fetch_one(&mut conn)
            .await
            .unwrap();
        let request_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM stock_adjustment_commands")
                .fetch_one(&mut conn)
                .await
                .unwrap();
        assert_eq!((movement_count, request_count), (2, 2));
    }

    #[tokio::test]
    async fn stock_changes_roll_back_with_their_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let service = SqliteCatalog::at(dir.path().join("catalog.sqlite3"));
        let product = service
            .add_with_quantity(
                Draft {
                    name: "Coffee".into(),
                    barcode: String::new(),
                    price: "1".into(),
                },
                0,
            )
            .await
            .unwrap();
        service
            .adjust_stock(product.id, 3, "received")
            .await
            .unwrap();
        let mut conn = service.open().await.unwrap();
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.unwrap();
        assert_eq!(
            insert_stock_movement(&mut tx, product.id, -2, "reservation", Some(7))
                .await
                .unwrap(),
            1
        );
        tx.rollback().await.unwrap();
        assert_eq!(service.list_stock().await.unwrap()[0].quantity, 3);
    }

    #[tokio::test]
    async fn catalog_survives_reopen_and_rejects_duplicates_and_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let draft = Draft {
            name: "  Tea ".into(),
            barcode: " 123 ".into(),
            price: "12.3".into(),
        };
        let saved = service.add_with_quantity(draft.clone(), 0).await.unwrap();
        assert_eq!(saved.price_cents, 1230);
        assert!(matches!(
            service.add_with_quantity(draft, 0).await,
            Err(CatalogError::DuplicateBarcode)
        ));
        assert!(matches!(
            service
                .add_with_quantity(
                    Draft {
                        name: "Invalid".into(),
                        barcode: String::new(),
                        price: "1.001".into()
                    },
                    0
                )
                .await,
            Err(CatalogError::Validation(_))
        ));
        assert_eq!(SqliteCatalog::at(path).list().await.unwrap(), vec![saved]);
        assert!(SqliteCatalog::at(dir.path()).list().await.is_err());
    }

    #[tokio::test]
    async fn crash_worker() {
        use std::io::Write;
        let Ok(path) = std::env::var("FACTUROX_CRASH_TEST_DB") else {
            return;
        };
        let service = SqliteCatalog::at(path);
        let mut conn = service.open().await.unwrap();
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query(
            "INSERT INTO products (name, barcode, price_cents) VALUES ('Crash test', NULL, 100)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        if std::env::var_os("FACTUROX_CRASH_TEST_COMMIT").is_some() {
            tx.commit().await.unwrap();
        }
        println!("FACTUROX_READY");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[tokio::test]
    async fn interrupted_write_preserves_only_committed_records() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.sqlite3");
        SqliteCatalog::at(&path).list().await.unwrap();
        for committed in [false, true] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "catalog::tests::crash_worker", "--nocapture"])
                .env("FACTUROX_CRASH_TEST_DB", &path)
                .stdout(Stdio::piped());
            if committed {
                command.env("FACTUROX_CRASH_TEST_COMMIT", "1");
            }
            let mut child = command.spawn().unwrap();
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            let mut ready = false;
            loop {
                let mut line = String::new();
                if stdout.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line.contains("FACTUROX_READY") {
                    ready = true;
                    break;
                }
            }
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(ready, "child exited before the transaction boundary");
            let products = SqliteCatalog::at(&path).list().await.unwrap();
            assert_eq!(products.len(), usize::from(committed));
            if committed {
                assert_eq!(products[0].name, "Crash test");
            }
        }
    }
}
