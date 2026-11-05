use super::{card::inset, format::money, typography::muted};
use crate::{messages::Message, product_search};
use facturox_core::domain::Product;
use iced::{
    Element,
    widget::{column, text},
};

pub fn product_matches<'a>(
    products: impl Iterator<Item = &'a Product>,
    name: &str,
    barcode: &str,
) -> Option<Element<'a, Message>> {
    let matches = product_search::similar(products, name, barcode);
    if matches.is_empty() {
        return None;
    }
    let mut content = column![
        text("Similar products already exist").size(15),
        muted("Check these before creating a new product. Similar names are a hint, not a restriction.")
    ].spacing(10);
    for product in matches {
        let barcode_note = product.barcode.as_ref().map_or_else(
            || "No barcode".to_owned(),
            |stored| {
                if !barcode.trim().is_empty() && stored == barcode.trim() {
                    format!("Barcode {stored} is already in use")
                } else {
                    format!("Barcode {stored}")
                }
            },
        );
        content = content.push(
            column![
                text(&product.name).size(14),
                muted(format!("{} · {barcode_note}", money(product.price)))
            ]
            .spacing(3),
        );
    }
    Some(inset(content))
}
