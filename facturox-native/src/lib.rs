//! Native Elm-style composition; persistence is owned exclusively by the core backend.

mod messages;
mod state;
#[cfg(test)]
mod tests;
mod update;
mod view;
use facturox_core::domain::{OrderLine, Product};
use iced::{Subscription, keyboard, window};
use messages::Message;
use state::App;

/// Frontend-only order editing state; the first add captures the name and price.
#[derive(Debug, Clone, Default)]
struct Draft {
    lines: Vec<OrderLine>,
}

impl Draft {
    fn add(&mut self, product: &Product) -> Result<(), String> {
        if let Some(line) = self
            .lines
            .iter_mut()
            .find(|line| line.product_id == product.id)
        {
            line.quantity = line.quantity.checked_add(1).ok_or("Quantity overflow")?;
        } else {
            self.lines.push(OrderLine {
                product_id: product.id.clone(),
                name: product.name.clone(),
                unit_price: product.price,
                quantity: 1,
            });
        }
        Ok(())
    }
}

fn subscription(_: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::event::listen_with(|event, _, _| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Tab),
                modifiers,
                ..
            }) => Some(Message::Focus(modifiers.shift())),
            _ => None,
        }),
        window::resize_events().map(|(_, size)| Message::Resize(size.width)),
    ])
}
/// Run the native desktop application.
/// # Errors
/// Returns errors from the windowing or rendering backend.
pub fn run() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Facturox")
        .theme(|app: &App| view::palette::theme(app.dark))
        .subscription(subscription)
        .run()
}
