//! Local company details and appearance preferences.
use rusqlite::{Transaction, params};

use crate::catalog::{CatalogError, SqliteCatalog, storage};

/// Operator-entered business details. These values are not fiscal verification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompanyDetails {
    pub business_name: String,
    pub tax_identifier: String,
    pub address: String,
    pub contact: String,
}

/// The preferred application appearance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemePreference {
    #[default]
    Light,
    Dark,
}

/// Create the settings row in the same transaction as the schema migration.
pub(crate) fn migrate_v4(tx: &Transaction<'_>) -> Result<(), CatalogError> {
    tx.execute_batch(
        "CREATE TABLE app_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            business_name TEXT NOT NULL DEFAULT '',
            tax_identifier TEXT NOT NULL DEFAULT '',
            address TEXT NOT NULL DEFAULT '',
            contact TEXT NOT NULL DEFAULT '',
            theme TEXT NOT NULL DEFAULT 'light' CHECK (theme IN ('light', 'dark'))
        );
        INSERT INTO app_settings (id) VALUES (1);
        PRAGMA user_version = 4;",
    )
    .map_err(storage)
}

fn validate_field(value: &str, field: &str, max_chars: usize) -> Result<String, CatalogError> {
    let value = value.trim();
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        return Err(CatalogError::Validation(format!(
            "{field} must contain at most {max_chars} characters and no control characters."
        )));
    }
    Ok(value.to_owned())
}

impl SqliteCatalog {
    /// Read the locally configured company details.
    pub fn load_company_details(&self) -> Result<CompanyDetails, CatalogError> {
        let conn = self.open()?;
        conn.query_row(
            "SELECT business_name, tax_identifier, address, contact FROM app_settings WHERE id = 1",
            [],
            |row| {
                Ok(CompanyDetails {
                    business_name: row.get(0)?,
                    tax_identifier: row.get(1)?,
                    address: row.get(2)?,
                    contact: row.get(3)?,
                })
            },
        )
        .map_err(storage)
    }

    /// Validate and atomically replace the company details without changing appearance.
    pub fn save_company_details(
        &self,
        details: &CompanyDetails,
    ) -> Result<CompanyDetails, CatalogError> {
        let details = CompanyDetails {
            business_name: validate_field(&details.business_name, "Business name", 200)?,
            tax_identifier: validate_field(&details.tax_identifier, "Tax identifier", 64)?,
            address: validate_field(&details.address, "Address", 300)?,
            contact: validate_field(&details.contact, "Contact", 200)?,
        };
        let conn = self.open()?;
        let changed = conn.execute(
            "UPDATE app_settings SET business_name = ?1, tax_identifier = ?2, address = ?3, contact = ?4 WHERE id = 1",
            params![details.business_name, details.tax_identifier, details.address, details.contact],
        ).map_err(storage)?;
        if changed != 1 {
            return Err(CatalogError::Storage(
                "The settings record is missing.".into(),
            ));
        }
        Ok(details)
    }

    /// Read the stored theme preference.
    pub fn load_theme(&self) -> Result<ThemePreference, CatalogError> {
        let conn = self.open()?;
        let value: String = conn
            .query_row("SELECT theme FROM app_settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(storage)?;
        match value.as_str() {
            "light" => Ok(ThemePreference::Light),
            "dark" => Ok(ThemePreference::Dark),
            _ => Err(CatalogError::Storage("Unknown theme preference.".into())),
        }
    }

    /// Persist appearance independently of any unsaved company form.
    pub fn save_theme(&self, theme: ThemePreference) -> Result<(), CatalogError> {
        let conn = self.open()?;
        let value = match theme {
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        };
        let changed = conn
            .execute("UPDATE app_settings SET theme = ?1 WHERE id = 1", [value])
            .map_err(storage)?;
        if changed != 1 {
            return Err(CatalogError::Storage(
                "The settings record is missing.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Draft};
    use rusqlite::Connection;

    #[test]
    fn preferences_survive_reopen_and_independent_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite3");
        let service = SqliteCatalog::at(&path);
        assert_eq!(
            service.load_company_details().unwrap(),
            CompanyDetails::default()
        );
        assert_eq!(service.load_theme().unwrap(), ThemePreference::Light);
        let details = CompanyDetails {
            business_name: "  Test shop  ".into(),
            tax_identifier: "TEST-ID".into(),
            address: "Test street".into(),
            contact: "test@example.invalid".into(),
        };
        let saved = service.save_company_details(&details).unwrap();
        assert_eq!(saved.business_name, "Test shop");
        service.save_theme(ThemePreference::Dark).unwrap();
        let reopened = SqliteCatalog::at(path);
        assert_eq!(reopened.load_company_details().unwrap(), saved);
        assert_eq!(reopened.load_theme().unwrap(), ThemePreference::Dark);
        reopened
            .save_company_details(&CompanyDetails::default())
            .unwrap();
        assert_eq!(reopened.load_theme().unwrap(), ThemePreference::Dark);
    }

    #[test]
    fn invalid_and_failed_saves_leave_previous_details_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite3");
        let service = SqliteCatalog::at(&path);
        let details = CompanyDetails {
            business_name: "Test shop".into(),
            ..CompanyDetails::default()
        };
        service.save_company_details(&details).unwrap();
        let invalid = CompanyDetails {
            contact: "bad\nline".into(),
            ..details.clone()
        };
        assert!(matches!(
            service.save_company_details(&invalid),
            Err(CatalogError::Validation(_))
        ));
        assert!(matches!(
            service.save_company_details(&CompanyDetails {
                business_name: "x".repeat(201),
                ..details.clone()
            }),
            Err(CatalogError::Validation(_))
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_settings BEFORE UPDATE ON app_settings BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(
            service
                .save_company_details(&CompanyDetails::default())
                .is_err()
        );
        assert_eq!(service.load_company_details().unwrap(), details);
    }

    #[test]
    fn migration_preserves_existing_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite3");
        let service = SqliteCatalog::at(&path);
        let product = service
            .add(Draft {
                name: "Test item".into(),
                barcode: String::new(),
                price: "1.25".into(),
            })
            .unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE app_settings; PRAGMA user_version = 3;")
            .unwrap();
        drop(conn);
        assert_eq!(
            service.load_company_details().unwrap(),
            CompanyDetails::default()
        );
        assert_eq!(service.list().unwrap(), vec![product]);
        assert_eq!(service.load_theme().unwrap(), ThemePreference::Light);
    }
}
