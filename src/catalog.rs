use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use directories::ProjectDirs;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub barcode: Option<String>,
    pub price_cents: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub name: String,
    pub barcode: String,
    pub price: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Validation(String),
    DuplicateBarcode,
    Storage(String),
}
impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) | Self::Storage(message) => f.write_str(message),
            Self::DuplicateBarcode => f.write_str("This barcode is already in use."),
        }
    }
}

pub trait Catalog {
    fn add(&self, draft: Draft) -> Result<Product, CatalogError>;
    fn list(&self) -> Result<Vec<Product>, CatalogError>;
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
            .ok_or_else(|| CatalogError::Storage("Could not find the user data directory.".into()))
    }

    fn open(&self) -> Result<Connection, CatalogError> {
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(storage)?;
        let mut conn = Connection::open(&self.path).map_err(storage)?;
        conn.busy_timeout(Duration::from_secs(2)).map_err(storage)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(storage)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(storage)?;
        if version > 1 {
            return Err(CatalogError::Storage(format!(
                "Unsupported catalog schema version {version}."
            )));
        }
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(storage)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(storage)?;
        if version == 0 {
            let tx = conn.transaction().map_err(storage)?;
            tx.execute_batch("CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 200), barcode TEXT UNIQUE CHECK(barcode IS NULL OR length(barcode) BETWEEN 1 AND 100), price_cents INTEGER NOT NULL CHECK(price_cents >= 0)); PRAGMA user_version = 1;").map_err(storage)?;
            tx.commit().map_err(storage)?;
        }
        Ok(conn)
    }
}
fn storage(error: impl fmt::Display) -> CatalogError {
    CatalogError::Storage(format!("Catalog storage error: {error}"))
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
    fn add(&self, draft: Draft) -> Result<Product, CatalogError> {
        let (name, barcode, price_cents) = validate(&draft)?;
        let mut conn = self.open()?;
        let tx = conn.transaction().map_err(storage)?;
        if let Some(code) = &barcode {
            let exists: Option<i64> = tx
                .query_row("SELECT id FROM products WHERE barcode = ?1", [code], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(storage)?;
            if exists.is_some() {
                return Err(CatalogError::DuplicateBarcode);
            }
        }
        tx.execute(
            "INSERT INTO products (name, barcode, price_cents) VALUES (?1, ?2, ?3)",
            params![name, barcode, price_cents],
        )
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == ErrorCode::ConstraintViolation && barcode.is_some() =>
            {
                CatalogError::DuplicateBarcode
            }
            _ => storage(e),
        })?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(storage)?;
        Ok(Product {
            id,
            name,
            barcode,
            price_cents,
        })
    }
    fn list(&self) -> Result<Vec<Product>, CatalogError> {
        let conn = self.open()?;
        let mut statement = conn
            .prepare("SELECT id, name, barcode, price_cents FROM products ORDER BY id")
            .map_err(storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Product {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    barcode: row.get(2)?,
                    price_cents: row.get(3)?,
                })
            })
            .map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_survives_reopen_and_rejects_duplicates_and_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let service = SqliteCatalog::at(&path);
        let draft = Draft {
            name: "  Tea ".into(),
            barcode: " 123 ".into(),
            price: "12.3".into(),
        };
        let saved = service.add(draft.clone()).unwrap();
        assert_eq!(saved.price_cents, 1230);
        assert_eq!(service.add(draft), Err(CatalogError::DuplicateBarcode));
        assert!(matches!(
            service.add(Draft {
                name: "Invalid".into(),
                barcode: String::new(),
                price: "1.001".into()
            }),
            Err(CatalogError::Validation(_))
        ));
        assert_eq!(SqliteCatalog::at(path).list().unwrap(), vec![saved]);
    }
    #[test]
    fn refuses_newer_schema_and_failed_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        assert!(SqliteCatalog::at(&path).list().is_err());
        assert_eq!(
            Connection::open(&path)
                .unwrap()
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            99
        );
        assert!(SqliteCatalog::at(dir.path()).list().is_err());
    }
    #[test]
    fn crash_worker() {
        use std::io::Write;
        let Ok(path) = std::env::var("FACTUROX_CRASH_TEST_DB") else {
            return;
        };
        let service = SqliteCatalog::at(path);
        let mut conn = service.open().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute(
            "INSERT INTO products (name, barcode, price_cents) VALUES ('Crash test', NULL, 100)",
            [],
        )
        .unwrap();
        if std::env::var_os("FACTUROX_CRASH_TEST_COMMIT").is_some() {
            tx.commit().unwrap();
        }
        println!("FACTUROX_READY");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn interrupted_write_preserves_only_committed_records() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.sqlite3");
        SqliteCatalog::at(&path).list().unwrap();
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
            assert!(
                ready,
                "child exited before reaching the requested transaction boundary"
            );
            let products = SqliteCatalog::at(&path).list().unwrap();
            assert_eq!(products.len(), usize::from(committed));
            if committed {
                assert_eq!(products[0].name, "Crash test");
            }
        }
    }
}
