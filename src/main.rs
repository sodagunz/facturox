mod catalog;

use catalog::{Catalog, CatalogError, Draft, Product, SqliteCatalog, StockBalance};
use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Element, Subscription, Task, keyboard};

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Facturox")
        .subscription(|_| App::subscription())
        .run()
}

#[derive(Default)]
struct App {
    screen: Screen,
    draft: Draft,
    products: Vec<Product>,
    stock: Vec<StockBalance>,
    selected_stock: Option<i64>,
    stock_delta: String,
    stock_reason: String,
    busy: bool,
    request: u64,
    error: Option<String>,
    notice: Option<String>,
}
#[derive(Default)]
enum Screen {
    #[default]
    Catalog,
    About,
}
#[derive(Debug, Clone)]
enum Message {
    Catalog,
    About,
    FocusNext,
    FocusPrevious,
    Name(String),
    Barcode(String),
    Price(String),
    Submit,
    Refresh,
    SelectStock(i64),
    StockDelta(String),
    StockReason(String),
    AdjustStock,
    StockAdjusted(u64, Result<i64, CatalogError>),
    Loaded(u64, Result<(Vec<Product>, Vec<StockBalance>), CatalogError>),
    Added(u64, Result<Product, CatalogError>),
}
fn worker<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CatalogError> + Send + 'static,
) -> impl Future<Output = Result<T, CatalogError>> + Send {
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(f());
    });
    async move {
        receiver.await.unwrap_or_else(|_| {
            Err(CatalogError::Storage(
                "Catalog worker stopped unexpectedly.".into(),
            ))
        })
    }
}
fn service() -> Result<SqliteCatalog, CatalogError> {
    Ok(SqliteCatalog::at(SqliteCatalog::default_path()?))
}
fn focus_navigation(event: &keyboard::Event) -> Option<Message> {
    match event {
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Tab),
            modifiers,
            ..
        } if !modifiers.control() && !modifiers.alt() && !modifiers.logo() => {
            Some(if modifiers.shift() {
                Message::FocusPrevious
            } else {
                Message::FocusNext
            })
        }
        _ => None,
    }
}
impl App {
    fn subscription() -> Subscription<Message> {
        keyboard::listen().filter_map(|event| focus_navigation(&event))
    }
    fn boot() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let task = app.load();
        (app, task)
    }
    fn load(&mut self) -> Task<Message> {
        self.busy = true;
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        Task::perform(
            worker(|| {
                let service = service()?;
                Ok((service.list()?, service.list_stock()?))
            }),
            move |result| Message::Loaded(id, result),
        )
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Catalog => self.screen = Screen::Catalog,
            Message::About => self.screen = Screen::About,
            Message::FocusNext => return iced::widget::operation::focus_next(),
            Message::FocusPrevious => return iced::widget::operation::focus_previous(),
            Message::Name(value) if !self.busy => self.draft.name = value,
            Message::Barcode(value) if !self.busy => self.draft.barcode = value,
            Message::Price(value) if !self.busy => self.draft.price = value,
            Message::Submit if !self.busy => {
                self.busy = true;
                self.error = None;
                self.notice = None;
                self.request = self.request.wrapping_add(1);
                let id = self.request;
                let draft = self.draft.clone();
                return Task::perform(worker(move || service()?.add(draft)), move |result| {
                    Message::Added(id, result)
                });
            }
            Message::SelectStock(id) if !self.busy => self.selected_stock = Some(id),
            Message::StockDelta(value) if !self.busy => self.stock_delta = value,
            Message::StockReason(value) if !self.busy => self.stock_reason = value,
            Message::AdjustStock if !self.busy => {
                let Some(id) = self.selected_stock else {
                    self.error = Some("Select a product to adjust.".into());
                    return Task::none();
                };
                let Ok(delta) = self.stock_delta.trim().parse::<i64>() else {
                    self.error = Some("Enter a whole-unit adjustment.".into());
                    return Task::none();
                };
                self.busy = true;
                self.error = None;
                self.request = self.request.wrapping_add(1);
                let request = self.request;
                let reason = self.stock_reason.clone();
                return Task::perform(
                    worker(move || service()?.adjust_stock(id, delta, &reason)),
                    move |result| Message::StockAdjusted(request, result),
                );
            }
            Message::StockAdjusted(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(_) => {
                        self.stock_delta.clear();
                        self.stock_reason.clear();
                        self.notice = Some("Stock adjusted.".into());
                        return self.load();
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Message::Refresh if !self.busy => {
                self.error = None;
                return self.load();
            }
            Message::Loaded(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok((products, stock)) => {
                        self.products = products;
                        self.stock = stock;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Message::Added(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(product) => {
                        self.stock.push(StockBalance {
                            product_id: product.id,
                            quantity: 0,
                        });
                        self.products.push(product);
                        self.draft = Draft::default();
                        self.notice = Some("Product saved.".into());
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            _ => {}
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> {
        let nav = row![
            button("Catalog").on_press(Message::Catalog),
            button("About").on_press(Message::About)
        ]
        .spacing(8);
        let content: Element<'_, Message> = match self.screen {
            Screen::About => column![text("Facturox is in development.")].into(),
            Screen::Catalog => {
                let mut save = button("Add product");
                let mut refresh = button("Refresh");
                if !self.busy {
                    save = save.on_press(Message::Submit);
                    refresh = refresh.on_press(Message::Refresh);
                }
                let mut content = column![
                    text("Product catalog"),
                    text_input("Name", &self.draft.name).on_input(Message::Name),
                    text_input("Barcode (optional)", &self.draft.barcode)
                        .on_input(Message::Barcode),
                    text_input("Price in pesos, e.g. 12.50", &self.draft.price)
                        .on_input(Message::Price),
                    row![save, refresh].spacing(8)
                ]
                .spacing(8);
                if self.busy {
                    content = content.push(text("Loading…"));
                }
                if let Some(error) = &self.error {
                    content = content.push(text(error));
                }
                if let Some(notice) = &self.notice {
                    content = content.push(text(notice));
                }
                content = content.push(text(format!(
                    "Adjust stock for product: {}",
                    self.selected_stock.map_or_else(
                        || "none selected".to_owned(),
                        |id| self
                            .products
                            .iter()
                            .find(|p| p.id == id)
                            .map_or_else(|| id.to_string(), |p| p.name.clone())
                    )
                )));
                let mut adjust = button("Apply adjustment");
                if !self.busy {
                    adjust = adjust.on_press(Message::AdjustStock);
                }
                content = content.push(
                    text_input("Change in units, e.g. 5 or -2", &self.stock_delta)
                        .on_input(Message::StockDelta),
                );
                content = content.push(
                    text_input("Reason for adjustment", &self.stock_reason)
                        .on_input(Message::StockReason),
                );
                content = content.push(adjust);
                for product in &self.products {
                    let quantity = self
                        .stock
                        .iter()
                        .find(|b| b.product_id == product.id)
                        .map_or(0, |b| b.quantity);
                    let mut select = button("Adjust");
                    if !self.busy {
                        select = select.on_press(Message::SelectStock(product.id));
                    }
                    content = content
                        .push(row![select, text(format!("Available: {quantity}"))].spacing(8));
                    content = content.push(text(format!(
                        "{} — ${}.{:02}{}",
                        product.name,
                        product.price_cents / 100,
                        product.price_cents % 100,
                        product
                            .barcode
                            .as_ref()
                            .map_or_else(String::new, |b| format!(" — {b}"))
                    )));
                }
                scrollable(content).into()
            }
        };
        column![nav, content].spacing(16).padding(24).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_save_keeps_form_and_stale_responses_are_ignored() {
        let mut app = App {
            busy: true,
            request: 2,
            draft: Draft {
                name: "Tea".into(),
                barcode: String::new(),
                price: "1.20".into(),
            },
            ..App::default()
        };
        drop(app.update(Message::Added(
            1,
            Ok(Product {
                id: 1,
                name: "Old".into(),
                barcode: None,
                price_cents: 10,
            }),
        )));
        assert!(app.busy);
        assert_eq!(app.products, []);
        drop(app.update(Message::Added(2, Err(CatalogError::DuplicateBarcode))));
        assert!(!app.busy);
        assert_eq!(app.draft.name, "Tea");
        assert!(app.error.is_some());
    }
}
