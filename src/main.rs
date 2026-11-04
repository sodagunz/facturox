mod catalog;

use catalog::{Catalog, CatalogError, Draft, Product, SqliteCatalog};
use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Element, Task};

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Facturox")
        .run()
}

#[derive(Default)]
struct App {
    screen: Screen,
    draft: Draft,
    products: Vec<Product>,
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
    Name(String),
    Barcode(String),
    Price(String),
    Submit,
    Refresh,
    Loaded(u64, Result<Vec<Product>, CatalogError>),
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
impl App {
    fn boot() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let task = app.load();
        (app, task)
    }
    fn load(&mut self) -> Task<Message> {
        self.busy = true;
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        Task::perform(worker(|| service()?.list()), move |result| {
            Message::Loaded(id, result)
        })
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Catalog => self.screen = Screen::Catalog,
            Message::About => self.screen = Screen::About,
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
            Message::Refresh if !self.busy => {
                self.error = None;
                return self.load();
            }
            Message::Loaded(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(products) => {
                        self.products = products;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Message::Added(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(product) => {
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
                for product in &self.products {
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
