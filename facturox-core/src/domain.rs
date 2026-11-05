//! One shop-wide aggregate. No persistence or UI dependencies.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub barcode: Option<String>,
    pub price: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductQuantity {
    pub product_id: String,
    pub quantity: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    pub product_id: String,
    pub name: String,
    pub unit_price: i64,
    pub quantity: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub lines: Vec<OrderLine>,
    /// Backend-recorded Unix epoch milliseconds; replay never reads a clock.
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    CreateProduct {
        product: Product,
        opening_quantity: i64,
    },
    AdjustProductQuantity {
        product_id: String,
        delta: i64,
        reason: Option<String>,
    },
    CreateOrder {
        id: String,
        lines: Vec<OrderLine>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    ProductCreated(Product),
    ProductQuantityAdjusted {
        product_id: String,
        delta: i64,
        reason: Option<String>,
    },
    OrderCreated(Order),
}
/// Ordered historical fact and its application-assigned recording context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub order: i64,
    pub operation_id: String,
    /// Unix epoch milliseconds, captured once per committed command batch.
    pub occurred_at: i64,
    pub event: Event,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub order: i64,
    pub products: BTreeMap<String, Product>,
    pub quantities: BTreeMap<String, ProductQuantity>,
    pub orders: BTreeMap<String, Order>,
}
fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn order_total(lines: &[OrderLine]) -> Result<i64, String> {
    lines.iter().try_fold(0_i64, |total, line| {
        line.unit_price
            .checked_mul(line.quantity)
            .and_then(|amount| total.checked_add(amount))
            .ok_or_else(|| "Order amount overflow".into())
    })
}
impl Order {
    /// Compute the total from the captured lines.
    /// # Errors
    /// Returns an error if a line amount or their sum overflows.
    pub fn total(&self) -> Result<i64, String> {
        order_total(&self.lines)
    }
}
impl Command {
    /// Validate acceptance against the current inventory without mutation.
    /// # Errors
    /// Rejects invalid identity, quantity, price, duplicate lines or arithmetic overflow.
    pub fn validate(&self, inventory: &Inventory) -> Result<(), String> {
        match self {
            Self::CreateProduct {
                product,
                opening_quantity,
            } => {
                require(
                    !product.id.trim().is_empty() && !inventory.products.contains_key(&product.id),
                    "Invalid or duplicate product id",
                )?;
                require(
                    !product.name.is_empty()
                        && product.name.chars().count() <= 200
                        && product.name.trim() == product.name
                        && !product.name.chars().any(char::is_control),
                    "Name must be trimmed, contain 1–200 characters, and have no control characters",
                )?;
                require(
                    product.price >= 0 && *opening_quantity >= 0,
                    "Negative price or quantity",
                )?;
                require(
                    product.barcode.as_ref().is_none_or(|b| {
                        !b.is_empty()
                            && b.chars().count() <= 100
                            && b.trim() == b
                            && !b.chars().any(char::is_control)
                    }),
                    "Barcode must be trimmed, contain 1–100 characters, and have no control characters",
                )?;
                require(
                    !inventory.products.values().any(|p| {
                        p.name == product.name
                            || product.barcode.is_some() && p.barcode == product.barcode
                    }),
                    "Duplicate name or barcode",
                )?;
            }
            Self::AdjustProductQuantity {
                product_id, delta, ..
            } => {
                require(*delta != 0, "Delta must be nonzero")?;
                let quantity = inventory
                    .quantities
                    .get(product_id)
                    .ok_or("Unknown product")?;
                require(
                    quantity
                        .quantity
                        .checked_add(*delta)
                        .is_some_and(|q| q >= 0),
                    "Quantity overflow or insufficient quantity",
                )?;
            }
            Self::CreateOrder { id, lines } => {
                require(
                    !id.trim().is_empty() && !inventory.orders.contains_key(id),
                    "Invalid or duplicate order id",
                )?;
                require(!lines.is_empty(), "Order must contain lines")?;
                let mut seen = BTreeSet::new();
                for line in lines {
                    require(seen.insert(&line.product_id), "Duplicate product line")?;
                    let product = inventory
                        .products
                        .get(&line.product_id)
                        .ok_or("Unknown product")?;
                    require(
                        line.name == product.name,
                        "Captured name does not match catalog",
                    )?;
                    require(
                        line.unit_price == product.price,
                        "Captured quote does not match catalog",
                    )?;
                    require(
                        line.quantity > 0
                            && inventory
                                .quantities
                                .get(&line.product_id)
                                .is_some_and(|q| q.quantity >= line.quantity),
                        "Insufficient quantity or nonpositive line",
                    )?;
                }
                order_total(lines)?;
            }
        }
        Ok(())
    }
}
impl Inventory {
    /// Decide without mutation using an explicit recording timestamp (Unix epoch milliseconds).
    /// # Errors
    /// Rejects invalid commands or negative timestamps without mutation.
    pub fn decide(&self, command: &Command, occurred_at: i64) -> Result<Vec<Event>, String> {
        require(occurred_at >= 0, "Invalid recording timestamp")?;
        command.validate(self)?;
        Ok(match command {
            Command::CreateProduct {
                product,
                opening_quantity,
            } => {
                let mut events = vec![Event::ProductCreated(product.clone())];
                if *opening_quantity != 0 {
                    events.push(Event::ProductQuantityAdjusted {
                        product_id: product.id.clone(),
                        delta: *opening_quantity,
                        reason: Some("Opening quantity".into()),
                    });
                }
                events
            }
            Command::AdjustProductQuantity {
                product_id,
                delta,
                reason,
            } => {
                vec![Event::ProductQuantityAdjusted {
                    product_id: product_id.clone(),
                    delta: *delta,
                    reason: reason.clone(),
                }]
            }
            Command::CreateOrder { id, lines } => vec![Event::OrderCreated(Order {
                id: id.clone(),
                lines: lines.clone(),
                created_at: occurred_at,
            })],
        })
    }
    // Replay validates facts, not today's command admission or quote policy.
    fn validate_event(&self, event: &Event) -> Result<(), String> {
        match event {
            Event::ProductCreated(p) => {
                require(
                    !p.id.is_empty() && !self.products.contains_key(&p.id),
                    "Duplicate or empty product id",
                )?;
                require(!p.name.is_empty() && p.price >= 0, "Invalid product fact")?;
                require(
                    !self.products.values().any(|existing| {
                        existing.name == p.name
                            || p.barcode.is_some() && existing.barcode == p.barcode
                    }),
                    "Duplicate product identity",
                )?;
            }
            Event::ProductQuantityAdjusted {
                product_id, delta, ..
            } => {
                let q = self.quantities.get(product_id).ok_or("Unknown product")?;
                require(
                    *delta != 0
                        && q.quantity
                            .checked_add(*delta)
                            .is_some_and(|value| value >= 0),
                    "Invalid quantity fact",
                )?;
            }
            Event::OrderCreated(order) => {
                require(
                    !order.id.is_empty()
                        && !self.orders.contains_key(&order.id)
                        && !order.lines.is_empty()
                        && order.created_at >= 0,
                    "Invalid order identity or timestamp",
                )?;
                let mut seen = BTreeSet::new();
                for line in &order.lines {
                    require(
                        seen.insert(&line.product_id)
                            && self.products.contains_key(&line.product_id),
                        "Duplicate or unknown product line",
                    )?;
                    require(
                        !line.name.is_empty() && line.unit_price >= 0 && line.quantity > 0,
                        "Invalid order line fact",
                    )?;
                    let q = self
                        .quantities
                        .get(&line.product_id)
                        .ok_or("Unknown quantity")?;
                    require(
                        q.quantity
                            .checked_sub(line.quantity)
                            .is_some_and(|value| value >= 0),
                        "Insufficient quantity",
                    )?;
                }
                order.total()?;
            }
        }
        Ok(())
    }
    /// Apply a validated event atomically, also checking replay invariants.
    /// # Errors
    /// Returns an error for corrupt or incompatible event contents.
    pub fn apply(&mut self, event: &Event) -> Result<(), String> {
        self.validate_event(event)?;
        let order = self.order.checked_add(1).ok_or("Order overflow")?;
        match event {
            Event::ProductCreated(p) => {
                self.products.insert(p.id.clone(), p.clone());
                self.quantities.insert(
                    p.id.clone(),
                    ProductQuantity {
                        product_id: p.id.clone(),
                        quantity: 0,
                    },
                );
            }
            Event::ProductQuantityAdjusted {
                product_id, delta, ..
            } => {
                let q = self
                    .quantities
                    .get_mut(product_id)
                    .ok_or("Unknown product")?;
                q.quantity = q.quantity.checked_add(*delta).ok_or("Quantity overflow")?;
            }
            Event::OrderCreated(order) => {
                for line in &order.lines {
                    let q = self
                        .quantities
                        .get_mut(&line.product_id)
                        .ok_or("Unknown product")?;
                    q.quantity = q
                        .quantity
                        .checked_sub(line.quantity)
                        .ok_or("Quantity overflow")?;
                }
                self.orders.insert(order.id.clone(), order.clone());
            }
        }
        self.order = order;
        Ok(())
    }
}
