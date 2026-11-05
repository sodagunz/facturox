mod catalog;
mod orders;

use catalog::{Catalog, CatalogError, Draft, Product, SqliteCatalog, StockBalance};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Border, Color, Element, Fill, Subscription, Task, Theme, keyboard};
use orders::{Order, OrderStatus};

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Facturox")
        .theme(|_: &App| Theme::Light)
        .subscription(|_| App::subscription())
        .run()
}

const MUTED: Color = Color::from_rgb8(100, 106, 116);
fn card_style(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::WHITE)
        .border(Border {
            color: Color::from_rgb8(226, 229, 233),
            width: 1.0,
            radius: 12.0.into(),
        })
}
fn inset_style(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb8(249, 250, 251))
        .border(Border {
            color: Color::from_rgb8(235, 237, 240),
            width: 1.0,
            radius: 8.0.into(),
        })
}
fn brand_style(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb8(25, 30, 38))
        .border(Border {
            radius: 8.0.into(),
            ..Border::default()
        })
}
fn page_style(_: &Theme) -> container::Style {
    container::Style::default().background(Color::from_rgb8(246, 247, 249))
}
fn error_style(_: &Theme) -> container::Style {
    container::Style::default()
        .background(Color::from_rgb8(254, 242, 242))
        .border(Border {
            color: Color::from_rgb8(254, 202, 202),
            width: 1.0,
            radius: 8.0.into(),
        })
}
fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, foreground) = match status {
        button::Status::Disabled => (
            Color::from_rgb8(229, 231, 235),
            Color::from_rgb8(156, 163, 175),
        ),
        button::Status::Hovered | button::Status::Pressed => {
            (Color::from_rgb8(55, 65, 81), Color::WHITE)
        }
        button::Status::Active => (Color::from_rgb8(25, 30, 38), Color::WHITE),
    };
    button::Style {
        background: Some(background.into()),
        text_color: foreground,
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn secondary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, foreground, border) = match status {
        button::Status::Disabled => (
            Color::from_rgb8(248, 249, 250),
            Color::from_rgb8(156, 163, 175),
            Color::from_rgb8(235, 237, 240),
        ),
        button::Status::Hovered | button::Status::Pressed => (
            Color::from_rgb8(241, 243, 245),
            Color::from_rgb8(25, 30, 38),
            Color::from_rgb8(209, 213, 219),
        ),
        button::Status::Active => (
            Color::WHITE,
            Color::from_rgb8(25, 30, 38),
            Color::from_rgb8(226, 229, 233),
        ),
    };
    button::Style {
        background: Some(background.into()),
        text_color: foreground,
        border: Border {
            color: border,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

#[derive(Default)]
struct App {
    screen: Screen,
    draft: Draft,
    products: Vec<Product>,
    stock: Vec<StockBalance>,
    open_order: Option<Order>,
    order_history: Vec<Order>,
    pending_order: Option<PendingOrder>,
    pending_stock: Option<PendingStock>,
    selected_stock: Option<i64>,
    stock_delta: String,
    stock_reason: String,
    busy: bool,
    state_stale: bool,
    request: u64,
    error: Option<String>,
    notice: Option<String>,
}
#[derive(Default)]
enum Screen {
    #[default]
    Catalog,
    Orders,
    History,
    About,
}
#[derive(Debug, Clone, Copy)]
enum OrderAction {
    Add(i64),
    Remove(i64, i64),
    Close(i64),
    Cancel(i64),
}

#[derive(Debug, Clone)]
struct PendingOrder {
    request_id: String,
    action: OrderAction,
}

#[derive(Debug, Clone)]
struct PendingStock {
    request_id: String,
    product_id: i64,
    delta: i64,
    reason: String,
}

#[derive(Debug, Clone)]
struct Snapshot {
    products: Vec<Product>,
    stock: Vec<StockBalance>,
    open_order: Option<Order>,
    history: Vec<Order>,
}

#[derive(Debug, Clone)]
enum Message {
    Catalog,
    Orders,
    History,
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
    RetryStock,
    StockAdjusted(u64, Result<i64, CatalogError>),
    OrderAction(OrderAction),
    RetryOrder,
    OrderCompleted(u64, Result<i64, CatalogError>),
    Loaded(u64, Result<Snapshot, CatalogError>),
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
        self.state_stale = true;
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        Task::perform(
            worker(|| {
                let service = service()?;
                Ok(Snapshot {
                    products: service.list()?,
                    stock: service.list_stock()?,
                    open_order: service.open_order()?,
                    history: service.order_history()?,
                })
            }),
            move |result| Message::Loaded(id, result),
        )
    }
    fn execute_pending_order(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_order.clone() else {
            return Task::none();
        };
        self.busy = true;
        self.request = self.request.wrapping_add(1);
        let request = self.request;
        Task::perform(
            worker(move || {
                let service = service()?;
                match pending.action {
                    OrderAction::Add(product_id) => {
                        service.add_to_order(&pending.request_id, product_id, 1)
                    }
                    OrderAction::Remove(order_id, product_id) => {
                        service.remove_from_order(&pending.request_id, order_id, product_id, 1)
                    }
                    OrderAction::Close(order_id) => {
                        service.close_order(&pending.request_id, order_id)
                    }
                    OrderAction::Cancel(order_id) => {
                        service.cancel_order(&pending.request_id, order_id)
                    }
                }
            }),
            move |result| Message::OrderCompleted(request, result),
        )
    }
    fn can_modify(&self) -> bool {
        !self.busy
            && !self.state_stale
            && self.pending_order.is_none()
            && self.pending_stock.is_none()
    }

    fn start_stock_adjustment(&mut self) -> Task<Message> {
        let Some(product_id) = self.selected_stock else {
            self.error = Some("Select a product to adjust.".into());
            return Task::none();
        };
        let Ok(delta) = self.stock_delta.trim().parse::<i64>() else {
            self.error = Some("Enter a whole-unit adjustment.".into());
            return Task::none();
        };
        self.pending_stock = Some(PendingStock {
            request_id: uuid::Uuid::new_v4().to_string(),
            product_id,
            delta,
            reason: self.stock_reason.clone(),
        });
        self.error = None;
        self.execute_pending_stock()
    }

    fn execute_pending_stock(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_stock.clone() else {
            return Task::none();
        };
        self.busy = true;
        self.request = self.request.wrapping_add(1);
        let request = self.request;
        Task::perform(
            worker(move || {
                service()?.adjust_stock_once(
                    &pending.request_id,
                    pending.product_id,
                    pending.delta,
                    &pending.reason,
                )
            }),
            move |result| Message::StockAdjusted(request, result),
        )
    }

    fn stock_completed(&mut self, result: Result<i64, CatalogError>) -> Task<Message> {
        self.busy = false;
        match result {
            Ok(_) => {
                self.pending_stock = None;
                self.stock_delta.clear();
                self.stock_reason.clear();
                self.notice = Some("Stock adjusted.".into());
                self.load()
            }
            Err(CatalogError::Validation(message)) => {
                self.pending_stock = None;
                self.error = Some(message);
                self.load()
            }
            Err(error) => {
                self.error = Some(format!(
                    "The stock update may have completed: {error}. Retry the same operation to reconcile."
                ));
                Task::none()
            }
        }
    }

    fn order_completed(&mut self, result: Result<i64, CatalogError>) -> Task<Message> {
        self.busy = false;
        match result {
            Ok(_) => {
                self.pending_order = None;
                self.notice = Some("Order updated.".into());
                self.load()
            }
            Err(CatalogError::Validation(message)) => {
                self.pending_order = None;
                self.error = Some(message);
                self.load()
            }
            Err(error) => {
                self.error = Some(format!(
                    "The order update may have completed: {error}. Retry the same operation to reconcile."
                ));
                Task::none()
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Catalog => self.screen = Screen::Catalog,
            Message::Orders => self.screen = Screen::Orders,
            Message::History => self.screen = Screen::History,
            Message::About => self.screen = Screen::About,
            Message::FocusNext => return iced::widget::operation::focus_next(),
            Message::FocusPrevious => return iced::widget::operation::focus_previous(),
            Message::Name(value) if !self.busy => self.draft.name = value,
            Message::Barcode(value) if !self.busy => self.draft.barcode = value,
            Message::Price(value) if !self.busy => self.draft.price = value,
            Message::Submit if self.can_modify() => {
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
            Message::AdjustStock if self.can_modify() => return self.start_stock_adjustment(),
            Message::RetryStock if !self.busy && self.pending_stock.is_some() => {
                self.error = None;
                return self.execute_pending_stock();
            }
            Message::StockAdjusted(id, result) if self.busy && id == self.request => {
                return self.stock_completed(result);
            }
            Message::OrderAction(action) if self.can_modify() => {
                self.pending_order = Some(PendingOrder {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    action,
                });
                self.error = None;
                self.notice = None;
                return self.execute_pending_order();
            }
            Message::RetryOrder if !self.busy && self.pending_order.is_some() => {
                self.error = None;
                return self.execute_pending_order();
            }
            Message::OrderCompleted(id, result) if self.busy && id == self.request => {
                return self.order_completed(result);
            }
            Message::Refresh if !self.busy => {
                self.error = None;
                return self.load();
            }
            Message::Loaded(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(snapshot) => {
                        self.products = snapshot.products;
                        self.stock = snapshot.stock;
                        self.open_order = snapshot.open_order;
                        self.order_history = snapshot.history;
                        self.state_stale = false;
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
    fn view_order(&self) -> Element<'_, Message> {
        let mut content = column![
            text("Current order").size(22),
            text("Items are reserved as soon as they are added.")
                .size(14)
                .color(MUTED)
        ]
        .spacing(16);
        if let Some(order) = &self.open_order {
            content = content.push(
                text(format!("ORDER #{:04}", order.id))
                    .size(12)
                    .color(MUTED),
            );
            for line in &order.lines {
                let mut add = button("+").style(secondary_button).padding([8, 14]);
                let mut remove = button("−").style(secondary_button).padding([8, 14]);
                if self.can_modify() {
                    add = add.on_press(Message::OrderAction(OrderAction::Add(line.product_id)));
                    remove = remove.on_press(Message::OrderAction(OrderAction::Remove(
                        order.id,
                        line.product_id,
                    )));
                }
                let details = column![
                    text(&line.name).size(16),
                    text(format!(
                        "{} units  ·  ${}.{:02} each",
                        line.quantity,
                        line.unit_price_cents / 100,
                        line.unit_price_cents % 100
                    ))
                    .size(13)
                    .color(MUTED)
                ]
                .spacing(4)
                .width(Fill);
                content = content.push(
                    container(
                        row![details, remove, add]
                            .spacing(8)
                            .align_y(Alignment::Center),
                    )
                    .padding(16)
                    .width(Fill)
                    .style(card_style),
                );
            }
            content = content.push(
                container(
                    row![
                        text("Total").size(16).width(Fill),
                        text(format!(
                            "${}.{:02}",
                            order.total_cents / 100,
                            order.total_cents % 100
                        ))
                        .size(24)
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([16, 4]),
            );
            let mut close = button("Close order")
                .padding([11, 18])
                .style(primary_button);
            let mut cancel = button("Cancel order")
                .padding([11, 18])
                .style(secondary_button);
            if self.can_modify() {
                if !order.lines.is_empty() {
                    close = close.on_press(Message::OrderAction(OrderAction::Close(order.id)));
                }
                cancel = cancel.on_press(Message::OrderAction(OrderAction::Cancel(order.id)));
            }
            content = content.push(row![close, cancel].spacing(10));
        } else {
            content = content.push(
                container(
                    column![
                        text("Your order is empty").size(17),
                        text("Add a product from the catalog to get started.")
                            .size(14)
                            .color(MUTED)
                    ]
                    .spacing(8),
                )
                .padding(24)
                .width(Fill)
                .style(card_style),
            );
        }
        scrollable(content).height(Fill).into()
    }

    fn view_history(&self) -> Element<'_, Message> {
        let mut content = column![
            text("Order log").size(22),
            text("A record of closed and cancelled orders.")
                .size(14)
                .color(MUTED)
        ]
        .spacing(16);
        if self.order_history.is_empty() {
            content = content.push(
                container(text("No orders yet.").color(MUTED))
                    .padding(24)
                    .width(Fill)
                    .style(card_style),
            );
        }
        for order in &self.order_history {
            let status = match order.status {
                OrderStatus::Closed => "Closed · pending fiscal handling",
                OrderStatus::Cancelled => "Cancelled",
                OrderStatus::Open => "Open",
            };
            let heading = row![
                column![
                    text(format!("Order #{:04}", order.id)).size(17),
                    text(status).size(13).color(MUTED)
                ]
                .spacing(5)
                .width(Fill),
                text(format!(
                    "${}.{:02}",
                    order.total_cents / 100,
                    order.total_cents % 100
                ))
                .size(18)
            ]
            .align_y(Alignment::Center);
            let mut details = column![heading].spacing(12);
            for line in &order.lines {
                details = details.push(
                    text(format!(
                        "{} × {}  ·  ${}.{:02}",
                        line.quantity,
                        line.name,
                        line.unit_price_cents / 100,
                        line.unit_price_cents % 100
                    ))
                    .size(14)
                    .color(MUTED),
                );
            }
            content = content.push(container(details).width(Fill).padding(20).style(card_style));
        }
        scrollable(content).height(Fill).into()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep the catalog form and list together while the screen is being refined."
    )]
    fn view_catalog(&self) -> Element<'_, Message> {
        let mut save = button("Add product")
            .padding([10, 16])
            .style(primary_button);
        let mut refresh = button("Refresh").padding([10, 16]).style(secondary_button);
        if self.can_modify() {
            save = save.on_press(Message::Submit);
            refresh = refresh.on_press(Message::Refresh);
        }
        let form = column![
            text("Add a product").size(18),
            text("Create a catalog item with a price.")
                .size(13)
                .color(MUTED),
            text_input("Product name", &self.draft.name)
                .on_input(Message::Name)
                .padding(11),
            text_input("Barcode (optional)", &self.draft.barcode)
                .on_input(Message::Barcode)
                .padding(11),
            text_input("Price in pesos, e.g. 12.50", &self.draft.price)
                .on_input(Message::Price)
                .padding(11),
            row![save, refresh].spacing(8)
        ]
        .spacing(10);
        let selected = self.selected_stock.map_or_else(
            || "Select a product below".to_owned(),
            |id| {
                self.products
                    .iter()
                    .find(|p| p.id == id)
                    .map_or_else(|| id.to_string(), |p| p.name.clone())
            },
        );
        let mut adjust = button("Apply adjustment")
            .padding([10, 16])
            .style(secondary_button);
        if self.can_modify() {
            adjust = adjust.on_press(Message::AdjustStock);
        }
        let stock_form = column![
            text("Adjust stock").size(18),
            text(selected).size(13).color(MUTED),
            text_input("Change in units, e.g. 5 or -2", &self.stock_delta)
                .on_input(Message::StockDelta)
                .padding(11),
            text_input("Reason for adjustment", &self.stock_reason)
                .on_input(Message::StockReason)
                .padding(11),
            adjust
        ]
        .spacing(10);
        let mut listing = column![
            row![
                text("Products").size(20).width(Fill),
                text(format!("{} items", self.products.len()))
                    .size(13)
                    .color(MUTED)
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        if self.products.is_empty() {
            listing = listing.push(
                text("No products yet. Add your first item above.")
                    .size(14)
                    .color(MUTED),
            );
        }
        for product in &self.products {
            let quantity = self
                .stock
                .iter()
                .find(|b| b.product_id == product.id)
                .map_or(0, |b| b.quantity);
            let subtitle = format!(
                "${}.{:02}  ·  {} available{}",
                product.price_cents / 100,
                product.price_cents % 100,
                quantity,
                product
                    .barcode
                    .as_ref()
                    .map_or_else(String::new, |b| format!("  ·  {b}"))
            );
            let mut select = button("Adjust").padding([8, 12]).style(secondary_button);
            if !self.busy {
                select = select.on_press(Message::SelectStock(product.id));
            }
            let mut add = button("Add to order")
                .padding([8, 12])
                .style(primary_button);
            if self.can_modify() && quantity > 0 {
                add = add.on_press(Message::OrderAction(OrderAction::Add(product.id)));
            }
            let info = column![
                text(&product.name).size(16),
                text(subtitle).size(13).color(MUTED)
            ]
            .spacing(4)
            .width(Fill);
            listing = listing.push(
                container(
                    row![info, select, add]
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(14)
                .style(inset_style),
            );
        }
        let forms = row![
            container(form).width(Fill).padding(20).style(card_style),
            container(stock_form)
                .width(Fill)
                .padding(20)
                .style(card_style)
        ]
        .spacing(16);
        let content = column![
            row![
                column![
                    text("Catalog & stock").size(24),
                    text("Manage products and available inventory.")
                        .size(14)
                        .color(MUTED)
                ]
                .spacing(5)
                .width(Fill)
            ]
            .align_y(Alignment::Center),
            forms,
            container(listing).padding(20).width(Fill).style(card_style)
        ]
        .spacing(18)
        .max_width(1120);
        scrollable(content).height(Fill).into()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep the application shell and its status controls visible together."
    )]
    fn view(&self) -> Element<'_, Message> {
        let tab = |label, message, selected| {
            button(text(label).size(14))
                .padding([10, 16])
                .style(if selected {
                    secondary_button
                } else {
                    button::text
                })
                .on_press(message)
        };
        let nav = row![
            tab(
                "Catalog",
                Message::Catalog,
                matches!(self.screen, Screen::Catalog)
            ),
            tab(
                "Open order",
                Message::Orders,
                matches!(self.screen, Screen::Orders)
            ),
            tab(
                "Order log",
                Message::History,
                matches!(self.screen, Screen::History)
            ),
            tab(
                "About",
                Message::About,
                matches!(self.screen, Screen::About)
            )
        ]
        .spacing(4);
        let brand = row![
            container(text("F").size(20).color(Color::WHITE))
                .padding([7, 13])
                .style(brand_style),
            column![
                text("facturox").size(20),
                text("POINT OF SALE").size(10).color(MUTED)
            ]
            .spacing(2),
            container(text("LOCAL").size(11).color(MUTED))
                .padding([5, 9])
                .style(inset_style)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let content: Element<'_, Message> = match self.screen {
            Screen::About => container(
                column![
                    text("About Facturox").size(22),
                    text("A local-first point of sale in development.")
                        .size(14)
                        .color(MUTED)
                ]
                .spacing(10),
            )
            .width(Fill)
            .padding(24)
            .style(card_style)
            .into(),
            Screen::Orders => self.view_order(),
            Screen::History => self.view_history(),
            Screen::Catalog => self.view_catalog(),
        };
        let mut layout = column![brand, nav].spacing(16);
        if self.busy {
            layout = layout.push(text("Working…").size(14).color(MUTED));
        }
        if let Some(error) = &self.error {
            layout = layout.push(
                container(text(error).size(14).color(Color::from_rgb8(153, 27, 27)))
                    .width(Fill)
                    .padding(12)
                    .style(error_style),
            );
        }
        if let Some(notice) = &self.notice {
            layout = layout.push(text(notice).size(14).color(Color::from_rgb8(22, 101, 52)));
        }
        if self.pending_order.is_some() && !self.busy {
            layout = layout.push(button("Retry order operation").on_press(Message::RetryOrder));
        }
        if self.pending_stock.is_some() && !self.busy {
            layout = layout.push(button("Retry stock adjustment").on_press(Message::RetryStock));
        }
        if self.state_stale && !self.busy {
            layout = layout.push(button("Refresh state").on_press(Message::Refresh));
        }
        container(
            layout
                .push(content)
                .spacing(18)
                .padding(28)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(page_style)
        .into()
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
    #[test]
    fn uncertain_order_error_keeps_request_and_blocks_changes() {
        let mut app = App {
            busy: true,
            request: 7,
            pending_order: Some(PendingOrder {
                request_id: "test-order".into(),
                action: OrderAction::Add(3),
            }),
            ..App::default()
        };
        drop(app.update(Message::OrderCompleted(
            7,
            Err(CatalogError::Storage("connection lost".into())),
        )));
        assert!(!app.busy);
        assert!(!app.can_modify());
        assert_eq!(
            app.pending_order.as_ref().map(|p| p.request_id.as_str()),
            Some("test-order")
        );
        assert!(app.error.is_some());
    }

    #[test]
    fn uncertain_stock_error_keeps_request_and_blocks_changes() {
        let mut app = App {
            busy: true,
            request: 9,
            pending_stock: Some(PendingStock {
                request_id: "test-stock".into(),
                product_id: 3,
                delta: 1,
                reason: "stock count".into(),
            }),
            ..App::default()
        };
        drop(app.update(Message::StockAdjusted(
            9,
            Err(CatalogError::Storage("connection lost".into())),
        )));
        assert!(!app.busy);
        assert!(!app.can_modify());
        assert_eq!(
            app.pending_stock.as_ref().map(|p| p.request_id.as_str()),
            Some("test-stock")
        );
        assert!(app.error.is_some());
    }
}
