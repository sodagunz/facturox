//! Local company details and appearance preferences.
use sqlx::Row;

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
    pub async fn load_company_details(&self) -> Result<CompanyDetails, CatalogError> {
        let mut conn = self.open().await?;
        let row = sqlx::query(
            "SELECT business_name, tax_identifier, address, contact FROM app_settings WHERE id = 1",
        )
        .fetch_one(&mut conn)
        .await
        .map_err(storage)?;
        Ok(CompanyDetails {
            business_name: row.try_get(0).map_err(storage)?,
            tax_identifier: row.try_get(1).map_err(storage)?,
            address: row.try_get(2).map_err(storage)?,
            contact: row.try_get(3).map_err(storage)?,
        })
    }

    /// Validate and atomically replace the company details without changing appearance.
    pub async fn save_company_details(
        &self,
        details: &CompanyDetails,
    ) -> Result<CompanyDetails, CatalogError> {
        let details = CompanyDetails {
            business_name: validate_field(&details.business_name, "Business name", 200)?,
            tax_identifier: validate_field(&details.tax_identifier, "Tax identifier", 64)?,
            address: validate_field(&details.address, "Address", 300)?,
            contact: validate_field(&details.contact, "Contact", 200)?,
        };
        let mut conn = self.open().await?;
        let changed = sqlx::query(
            "UPDATE app_settings SET business_name = ?1, tax_identifier = ?2, address = ?3, contact = ?4 WHERE id = 1",
        )
        .bind(&details.business_name)
        .bind(&details.tax_identifier)
        .bind(&details.address)
        .bind(&details.contact)
        .execute(&mut conn)
        .await
        .map_err(storage)?
        .rows_affected();
        if changed != 1 {
            return Err(CatalogError::Storage(
                "The settings record is missing.".into(),
            ));
        }
        Ok(details)
    }

    /// Read the stored theme preference.
    pub async fn load_theme(&self) -> Result<ThemePreference, CatalogError> {
        let mut conn = self.open().await?;
        let value: String = sqlx::query_scalar("SELECT theme FROM app_settings WHERE id = 1")
            .fetch_one(&mut conn)
            .await
            .map_err(storage)?;
        match value.as_str() {
            "light" => Ok(ThemePreference::Light),
            "dark" => Ok(ThemePreference::Dark),
            _ => Err(CatalogError::Storage("Unknown theme preference.".into())),
        }
    }

    /// Persist appearance independently of any unsaved company form.
    pub async fn save_theme(&self, theme: ThemePreference) -> Result<(), CatalogError> {
        let mut conn = self.open().await?;
        let value = match theme {
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        };
        let changed = sqlx::query("UPDATE app_settings SET theme = ?1 WHERE id = 1")
            .bind(value)
            .execute(&mut conn)
            .await
            .map_err(storage)?
            .rows_affected();
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

    #[tokio::test]
    async fn preferences_survive_reopen_and_independent_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite3");
        let service = SqliteCatalog::at(&path);
        assert_eq!(
            service.load_company_details().await.unwrap(),
            CompanyDetails::default()
        );
        assert_eq!(service.load_theme().await.unwrap(), ThemePreference::Light);
        let details = CompanyDetails {
            business_name: "  Test shop  ".into(),
            tax_identifier: "TEST-ID".into(),
            address: "Test street".into(),
            contact: "test@example.invalid".into(),
        };
        let saved = service.save_company_details(&details).await.unwrap();
        assert_eq!(saved.business_name, "Test shop");
        service.save_theme(ThemePreference::Dark).await.unwrap();
        let reopened = SqliteCatalog::at(path);
        assert_eq!(reopened.load_company_details().await.unwrap(), saved);
        assert_eq!(reopened.load_theme().await.unwrap(), ThemePreference::Dark);
        reopened
            .save_company_details(&CompanyDetails::default())
            .await
            .unwrap();
        assert_eq!(reopened.load_theme().await.unwrap(), ThemePreference::Dark);
    }

    #[tokio::test]
    async fn invalid_and_failed_saves_leave_previous_details_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite3");
        let service = SqliteCatalog::at(&path);
        let details = CompanyDetails {
            business_name: "Test shop".into(),
            ..CompanyDetails::default()
        };
        service.save_company_details(&details).await.unwrap();
        let invalid = CompanyDetails {
            contact: "bad\nline".into(),
            ..details.clone()
        };
        assert!(matches!(
            service.save_company_details(&invalid).await,
            Err(CatalogError::Validation(_))
        ));
        assert!(matches!(
            service
                .save_company_details(&CompanyDetails {
                    business_name: "x".repeat(201),
                    ..details.clone()
                })
                .await,
            Err(CatalogError::Validation(_))
        ));
        let mut conn = service.open().await.unwrap();
        sqlx::query("CREATE TRIGGER reject_settings BEFORE UPDATE ON app_settings BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
            .execute(&mut conn).await.unwrap();
        assert!(
            service
                .save_company_details(&CompanyDetails::default())
                .await
                .is_err()
        );
        assert_eq!(service.load_company_details().await.unwrap(), details);
    }
}
