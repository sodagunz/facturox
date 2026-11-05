use crate::state::{Field, Screen};
use facturox_core::{
    backend::{Backend, CompanyDetails, Error, Receipt},
    domain::Inventory,
};
#[derive(Clone)]
pub struct BackendHandle(pub Backend);
impl std::fmt::Debug for BackendHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Backend")
    }
}
pub type Loaded = (BackendHandle, Inventory, CompanyDetails, bool);
#[derive(Debug, Clone)]
pub enum Message {
    Open,
    Loaded(u64, Result<Loaded, Error>),
    Completed(u64, String, Result<Receipt, Error>),
    Synchronized(u64, Result<Inventory, Error>),
    CompanySaved(u64, Result<CompanyDetails, Error>),
    ThemeSaved(u64, Result<bool, Error>),
    Edit(Field, String),
    Navigate(Screen),
    Select(String),
    Add(String),
    Minus(String),
    Discard,
    Checkout,
    Create,
    Adjust,
    SaveCompany,
    Toggle,

    Focus(bool),
    Resize(f32),
}
