use facturox_core::{
    backend::{Backend, CompanyDetails, Request},
    domain::Inventory,
};

use crate::Draft;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Catalog,
    Inventory,
    Settings,
    About,
}
#[derive(Debug, Clone, Copy)]
pub enum Field {
    Name,
    Barcode,
    Price,
    Opening,
    Delta,
    Reason,
    Business,
    Tax,
    Address,
    Contact,
}
#[derive(Default)]
pub struct App {
    pub backend: Option<Backend>,
    pub inventory: Inventory,
    pub draft: Draft,
    pub pending: Option<Request>,
    pub active: Option<u64>,
    pub serial: u64,
    pub stale: bool,
    pub dark: bool,
    pub screen: Screen,
    pub width: f32,
    pub error: String,
    pub name: String,
    pub barcode: String,
    pub price: String,
    pub opening: String,
    pub selected: Option<String>,
    pub delta: String,
    pub reason: String,
    pub company: CompanyDetails,
}
impl App {
    pub const fn editable(&self) -> bool {
        self.backend.is_some() && self.active.is_none() && self.pending.is_none() && !self.stale
    }
    pub const fn begin(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.active = Some(self.serial);
        self.serial
    }
}
