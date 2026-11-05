mod alert;
mod button;
mod card;
mod format;
mod input;
mod page;
pub mod palette;
mod typography;

use self::{
    alert::alert,
    button::{Variant, action, control, selected},
    card::{card, inset},
    format::money,
    input::input,
    page::page,
    typography::{heading, muted},
};
use crate::{
    messages::Message,
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
    fn catalog(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let mut products = column![heading("Available products")].spacing(12);
        for product in self.inventory.products.values() {
            let quantity = self
                .inventory
                .quantities
                .get(&product.id)
                .map_or(0, |q| q.quantity);
            products = products.push(inset(
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
        if self.inventory.products.is_empty() {
            products = products.push(muted("Create your first product in Manage inventory."));
        }
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
    fn manage(&self) -> Element<'_, Message> {
        let enabled = self.editable();
        let create = card(
            column![
                heading("Create product"),
                input("Product name", &self.name, Field::Name, enabled),
                input("Barcode (optional)", &self.barcode, Field::Barcode, enabled),
                input(
                    "Price in pesos (e.g. 1250.50)",
                    &self.price,
                    Field::Price,
                    enabled
                ),
                input("Opening quantity", &self.opening, Field::Opening, enabled),
                action("Create product", Message::Create, enabled)
            ]
            .spacing(14),
        );
        let mut adjust =
            column![heading("Adjust inventory"), muted("Select a product")].spacing(12);
        for product in self.inventory.products.values() {
            let quantity = self
                .inventory
                .quantities
                .get(&product.id)
                .map_or(0, |q| q.quantity);
            adjust = adjust.push(selected(
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
        if self.width > 850.0 {
            row![create, card(adjust)].spacing(18).into()
        } else {
            column![create, card(adjust)].spacing(18).into()
        }
    }
}
