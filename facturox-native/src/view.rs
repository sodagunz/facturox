mod alert;
mod button;
mod card;
mod format;
mod input;
mod page;
pub mod palette;
mod product_matches;
mod typography;

use self::{
    alert::alert,
    button::{Variant, action, control, selected},
    card::{card, inset},
    format::money,
    input::input,
    page::page,
    product_matches::product_matches,
    typography::{heading, muted},
};
use crate::{
    messages::Message,
    product_search,
    state::{App, Field, Screen},
    update::total,
};
use iced::{
    Element, Length,
    widget::{column, row, scrollable, text},
};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let nav = column![
            row![heading("Facturox"), muted("LOCAL WORKSPACE")]
                .spacing(16)
                .wrap(),
            row![
                selected(
                    "Catalog",
                    Message::Navigate(Screen::Catalog),
                    true,
                    self.screen == Screen::Catalog
                ),
                selected(
                    "Manage inventory",
                    Message::Navigate(Screen::Inventory),
                    true,
                    self.screen == Screen::Inventory
                ),
                selected(
                    "Settings",
                    Message::Navigate(Screen::Settings),
                    true,
                    self.screen == Screen::Settings
                ),
                selected(
                    "About",
                    Message::Navigate(Screen::About),
                    true,
                    self.screen == Screen::About
                )
            ]
            .spacing(6)
            .wrap()
        ]
        .spacing(18);
        let mut body = column![card(nav)].spacing(20);

        if self.active.is_some() {
            body = body.push(muted("Working…"));
        }
        if !self.error.is_empty() {
            body = body.push(alert(
                column![heading("Operation needs attention"), text(&self.error)].spacing(8),
            ));
        }

        let content = match self.screen {
            Screen::Catalog => self.catalog(),
            Screen::Inventory => self.manage(),
            Screen::Settings => card(column![heading("Company details"), muted("Operator-entered details; not fiscal registration verification."), input("Business name", &self.company.business_name, Field::Business, enabled), input("Tax identifier", &self.company.tax_identifier, Field::Tax, enabled), input("Address", &self.company.address, Field::Address, enabled), input("Contact", &self.company.contact, Field::Contact, enabled), action("Save company details", Message::SaveCompany, enabled), heading("Appearance"), action(if self.dark { "Use light theme" } else { "Use dark theme" }, Message::Toggle, enabled)].spacing(14)),
            Screen::About => card(column![heading("About Facturox"), muted("Local catalog, inventory and order recording."), muted("Local checkout is not an authorized fiscal invoice. Fiscal authorization and printing are not implemented.")].spacing(14)),
        };
        body = body.push(content);
        page(scrollable(body.padding(24)))
    }
    fn product_picker(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let matches =
            product_search::filter(self.inventory.products.values(), &self.catalog_search);
        let products = column![
            heading("Available products"),
            input(
                "Search by product name or barcode",
                &self.catalog_search,
                Field::CatalogSearch,
                enabled
            ),
            muted(format!(
                "{} of {} products",
                matches.len(),
                self.inventory.products.len()
            )),
        ]
        .spacing(12);
        let mut listing = column![].spacing(12);
        if matches.is_empty() {
            listing = listing.push(muted(if self.inventory.products.is_empty() {
                "Create your first product in Manage inventory."
            } else {
                "No matching products. Try a different name or barcode."
            }));
        }
        for product in matches {
            let quantity = self
                .inventory
                .quantities
                .get(&product.id)
                .map_or(0, |q| q.quantity);
            listing = listing.push(inset(
                row![
                    column![
                        text(&product.name).size(18),
                        muted(format!("{} · {quantity} available", money(product.price)))
                    ]
                    .width(Length::Fill),
                    action(
                        "Add +",
                        Message::Add(product.id.clone()),
                        enabled && quantity > 0
                    )
                ]
                .spacing(12)
                .wrap(),
            ));
        }
        products.push(scrollable(listing).height(420)).into()
    }

    fn catalog(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let products = self.product_picker();
        let mut draft = column![
            heading("Current draft"),
            muted("Review quantities before recording checkout.")
        ]
        .spacing(12);
        if self.draft.lines.is_empty() {
            draft = draft.push(muted("Your draft is empty. Add a product to get started."));
        }
        for line in &self.draft.lines {
            draft = draft.push(inset(
                column![
                    text(format!(
                        "{} · {} × {}",
                        line.name,
                        line.quantity,
                        money(line.unit_price)
                    )),
                    row![
                        control(
                            "−",
                            Message::Minus(line.product_id.clone()),
                            enabled,
                            Variant::Secondary
                        ),
                        control(
                            "+",
                            Message::Add(line.product_id.clone()),
                            enabled,
                            Variant::Secondary
                        )
                    ]
                    .spacing(8)
                ]
                .spacing(6),
            ));
        }
        draft = draft
            .push(
                text(format!(
                    "Total {}",
                    total(&self.draft).map_or_else(|e| e, money)
                ))
                .size(22),
            )
            .push(
                row![
                    control(
                        "Discard",
                        Message::Discard,
                        enabled && !self.draft.lines.is_empty(),
                        Variant::Destructive
                    ),
                    action(
                        "Record local checkout",
                        Message::Checkout,
                        enabled && !self.draft.lines.is_empty()
                    )
                ]
                .spacing(8)
                .wrap(),
            )
            .push(muted(
                "Pending fiscal issuance — not an authorized invoice.",
            ));
        let panes: Element<'_, Message> = if self.width > 850.0 {
            row![card(products), card(draft)].spacing(18).into()
        } else {
            column![card(products), card(draft)].spacing(18).into()
        };
        column![panes, self.history()].spacing(18).into()
    }
    fn history(&self) -> Element<'_, Message> {
        let mut history = column![heading("Order history")].spacing(12);
        let mut orders: Vec<_> = self.inventory.orders.values().collect();
        orders.sort_by_key(|order| std::cmp::Reverse(order.created_at));
        if orders.is_empty() {
            history = history.push(muted("Recorded local orders will appear here."));
        }
        for order in orders {
            history = history.push(inset(text(format!(
                "{} · {} · Unix ms {} · Pending fiscal issuance",
                order.id,
                order.total().map_or_else(|error| error, money),
                order.created_at
            ))));
        }
        card(history)
    }
    fn create_product_form(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let mut form = column![
            heading("Create product"),
            input("Product name", &self.name, Field::Name, enabled),
        ]
        .spacing(14);
        if let Some(matches) =
            product_matches(self.inventory.products.values(), &self.name, &self.barcode)
        {
            form = form.push(matches);
        }
        card(
            form.push(input(
                "Barcode (optional)",
                &self.barcode,
                Field::Barcode,
                enabled,
            ))
            .push(input(
                "Price in pesos (e.g. 1250.50)",
                &self.price,
                Field::Price,
                enabled,
            ))
            .push(input(
                "Opening quantity",
                &self.opening,
                Field::Opening,
                enabled,
            ))
            .push(action("Create product", Message::Create, enabled)),
        )
    }

    fn adjust_product_quantity(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let matches =
            product_search::filter(self.inventory.products.values(), &self.inventory_search);
        let mut adjust = column![
            heading("Adjust inventory"),
            input(
                "Search by product name or barcode",
                &self.inventory_search,
                Field::InventorySearch,
                enabled
            ),
            muted(format!(
                "{} of {} products",
                matches.len(),
                self.inventory.products.len()
            )),
        ]
        .spacing(12);
        let mut listing = column![].spacing(8);
        if matches.is_empty() {
            listing = listing.push(muted(if self.inventory.products.is_empty() {
                "No products yet. Create your first product alongside."
            } else {
                "No matching products. Try a different name or barcode."
            }));
        }
        for product in matches {
            let quantity = self
                .inventory
                .quantities
                .get(&product.id)
                .map_or(0, |q| q.quantity);
            listing = listing.push(selected(
                format!(
                    "{}{} · {quantity} units",
                    if self.selected.as_ref() == Some(&product.id) {
                        "✓ "
                    } else {
                        ""
                    },
                    product.name
                ),
                Message::Select(product.id.clone()),
                enabled,
                self.selected.as_ref() == Some(&product.id),
            ));
        }
        adjust = adjust.push(scrollable(listing).height(320));
        if let Some(product) = self
            .selected
            .as_ref()
            .and_then(|id| self.inventory.products.get(id))
        {
            adjust = adjust.push(inset(text(format!("Adjusting: {}", product.name))));
        }
        adjust = adjust
            .push(input(
                "Quantity change (+ / − units)",
                &self.delta,
                Field::Delta,
                enabled,
            ))
            .push(input(
                "Reason (optional)",
                &self.reason,
                Field::Reason,
                enabled,
            ))
            .push(action(
                "Apply adjustment",
                Message::Adjust,
                enabled && self.selected.is_some(),
            ));
        card(adjust)
    }

    fn manage(&self) -> Element<'_, Message> {
        let create = self.create_product_form();
        let adjust = self.adjust_product_quantity();
        if self.width > 850.0 {
            row![create, adjust].spacing(18).into()
        } else {
            column![create, adjust].spacing(18).into()
        }
    }
}
