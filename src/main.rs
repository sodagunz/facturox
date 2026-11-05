mod catalog;
mod database;
mod orders;
mod settings;

use catalog::{Catalog, CatalogError, Draft, Product, SqliteCatalog, StockBalance};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Border, Color, Element, Fill, Subscription, Task, Theme, keyboard};
use orders::{Order, OrderStatus};
use settings::{CompanyDetails, ThemePreference};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    iced::application(App::boot, App::update, App::view)
        .title("Facturox")
        .theme(|app: &App| {
            if app.theme == ThemePreference::Dark {
                Theme::Dark
            } else {
                Theme::Light
            }
        })
        .subscription(|_| App::subscription())
        .run()?;
    Ok(())
}

const MUTED: Color = Color::from_rgb8(100, 106, 116);
fn card_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(if matches!(theme, Theme::Dark) {
            Color::from_rgb8(34, 36, 42)
        } else {
            Color::WHITE
        })
        .border(Border {
            color: if matches!(theme, Theme::Dark) {
                Color::from_rgb8(65, 68, 76)
            } else {
                Color::from_rgb8(226, 229, 233)
            },
            width: 1.0,
            radius: 12.0.into(),
        })
}
fn inset_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(if matches!(theme, Theme::Dark) {
            Color::from_rgb8(43, 46, 53)
        } else {
            Color::from_rgb8(249, 250, 251)
        })
        .border(Border {
            color: if matches!(theme, Theme::Dark) {
                Color::from_rgb8(65, 68, 76)
            } else {
                Color::from_rgb8(235, 237, 240)
            },
            width: 1.0,
            radius: 8.0.into(),
        })
}
fn brand_style(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(if matches!(theme, Theme::Dark) {
            Color::from_rgb8(65, 72, 85)
        } else {
            Color::from_rgb8(25, 30, 38)
        })
        .border(Border {
            radius: 8.0.into(),
            ..Border::default()
        })
}
fn page_style(theme: &Theme) -> container::Style {
    container::Style::default().background(if matches!(theme, Theme::Dark) {
        Color::from_rgb8(21, 23, 27)
    } else {
        Color::from_rgb8(246, 247, 249)
    })
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
fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    if matches!(theme, Theme::Dark) {
        let (background, foreground) = match status {
            button::Status::Disabled => (
                Color::from_rgb8(55, 58, 64),
                Color::from_rgb8(145, 150, 160),
            ),
            button::Status::Hovered | button::Status::Pressed => (
                Color::from_rgb8(205, 210, 218),
                Color::from_rgb8(24, 26, 30),
            ),
            button::Status::Active => (
                Color::from_rgb8(236, 238, 242),
                Color::from_rgb8(24, 26, 30),
            ),
        };
        return button::Style {
            background: Some(background.into()),
            text_color: foreground,
            border: Border {
                radius: 8.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        };
    }
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

fn ghost_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::text(theme, status);
    if matches!(theme, Theme::Dark) {
        style.text_color = Color::from_rgb8(232, 234, 238);
    }
    style
}

fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    if matches!(theme, Theme::Dark) {
        let (background, foreground) = match status {
            button::Status::Disabled => (
                Color::from_rgb8(39, 42, 48),
                Color::from_rgb8(125, 130, 140),
            ),
            button::Status::Hovered | button::Status::Pressed => (
                Color::from_rgb8(64, 68, 76),
                Color::from_rgb8(238, 240, 244),
            ),
            button::Status::Active => (
                Color::from_rgb8(46, 49, 56),
                Color::from_rgb8(238, 240, 244),
            ),
        };
        return button::Style {
            background: Some(background.into()),
            text_color: foreground,
            border: Border {
                color: Color::from_rgb8(76, 80, 90),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..button::Style::default()
        };
    }
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
    theme: ThemePreference,
    company: CompanyDetails,
    settings_loaded: bool,
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
    About,
    Settings,
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
    company: CompanyDetails,
    theme: ThemePreference,
}

#[derive(Debug, Clone)]
enum Message {
    Catalog,
    Orders,
    About,
    Settings,
    ToggleTheme,
    ThemeSaved(u64, Result<ThemePreference, CatalogError>),
    CompanyName(String),
    CompanyTaxId(String),
    CompanyAddress(String),
    CompanyContact(String),
    SaveCompany,
    CompanySaved(u64, Result<CompanyDetails, CatalogError>),
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
    fn muted(&self) -> Color {
        if self.theme == ThemePreference::Dark {
            Color::from_rgb8(170, 175, 185)
        } else {
            MUTED
        }
    }
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
            async move {
                let service = service()?;
                Ok(Snapshot {
                    products: service.list().await?,
                    stock: service.list_stock().await?,
                    open_order: service.open_order().await?,
                    history: service.order_history().await?,
                    company: service.load_company_details().await?,
                    theme: service.load_theme().await?,
                })
            },
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
            async move {
                let service = service()?;
                match pending.action {
                    OrderAction::Add(product_id) => {
                        service
                            .add_to_order(&pending.request_id, product_id, 1)
                            .await
                    }
                    OrderAction::Remove(order_id, product_id) => {
                        service
                            .remove_from_order(&pending.request_id, order_id, product_id, 1)
                            .await
                    }
                    OrderAction::Close(order_id) => {
                        service.close_order(&pending.request_id, order_id).await
                    }
                    OrderAction::Cancel(order_id) => {
                        service.cancel_order(&pending.request_id, order_id).await
                    }
                }
            },
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
            async move {
                service()?
                    .adjust_stock_once(
                        &pending.request_id,
                        pending.product_id,
                        pending.delta,
                        &pending.reason,
                    )
                    .await
            },
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

    fn save_company(&mut self) -> Task<Message> {
        self.busy = true;
        self.error = None;
        self.notice = None;
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        let details = self.company.clone();
        Task::perform(
            async move { service()?.save_company_details(&details).await },
            move |result| Message::CompanySaved(id, result),
        )
    }

    fn toggle_theme(&mut self) -> Task<Message> {
        self.busy = true;
        self.error = None;
        self.request = self.request.wrapping_add(1);
        let id = self.request;
        let next = if self.theme == ThemePreference::Light {
            ThemePreference::Dark
        } else {
            ThemePreference::Light
        };
        Task::perform(
            async move {
                service()?.save_theme(next).await?;
                Ok(next)
            },
            move |result| Message::ThemeSaved(id, result),
        )
    }

    fn loaded(&mut self, result: Result<Snapshot, CatalogError>) {
        self.busy = false;
        match result {
            Ok(snapshot) => {
                if !self.settings_loaded {
                    self.company = snapshot.company;
                    self.theme = snapshot.theme;
                    self.settings_loaded = true;
                }
                self.products = snapshot.products;
                self.stock = snapshot.stock;
                self.open_order = snapshot.open_order;
                self.order_history = snapshot.history;
                self.state_stale = false;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Catalog => self.screen = Screen::Catalog,
            Message::Orders => self.screen = Screen::Orders,
            Message::About => self.screen = Screen::About,
            Message::Settings => self.screen = Screen::Settings,
            Message::CompanyName(value) if !self.busy => self.company.business_name = value,
            Message::CompanyTaxId(value) if !self.busy => self.company.tax_identifier = value,
            Message::CompanyAddress(value) if !self.busy => self.company.address = value,
            Message::CompanyContact(value) if !self.busy => self.company.contact = value,
            Message::SaveCompany if self.can_modify() => return self.save_company(),
            Message::CompanySaved(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(details) => {
                        self.company = details;
                        self.notice = Some("Company details saved.".into());
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Message::ToggleTheme if self.can_modify() => return self.toggle_theme(),
            Message::ThemeSaved(id, result) if self.busy && id == self.request => {
                self.busy = false;
                match result {
                    Ok(theme) => self.theme = theme,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
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
                return Task::perform(async move { service()?.add(draft).await }, move |result| {
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
            Message::Loaded(id, result) if self.busy && id == self.request => self.loaded(result),
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
                .color(self.muted())
        ]
        .spacing(16);
        if let Some(order) = &self.open_order {
            content = content.push(
                text(format!("ORDER #{:04}", order.id))
                    .size(12)
                    .color(self.muted()),
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
                    .color(self.muted())
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
            let mut cancel = button("Discard order")
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
                            .color(self.muted())
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
                .color(self.muted())
        ]
        .spacing(16);
        if self.order_history.is_empty() {
            content = content.push(
                container(text("No orders yet.").color(self.muted()))
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
                    text(status).size(13).color(self.muted())
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
                    .color(self.muted()),
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
                .color(self.muted()),
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
            text(selected).size(13).color(self.muted()),
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
                    .color(self.muted())
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        if self.products.is_empty() {
            listing = listing.push(
                text("No products yet. Add your first item above.")
                    .size(14)
                    .color(self.muted()),
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
            let info = column![
                text(&product.name).size(16),
                text(subtitle).size(13).color(self.muted())
            ]
            .spacing(4)
            .width(Fill);
            listing = listing.push(
                container(row![info, select].spacing(8).align_y(Alignment::Center))
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
                        .color(self.muted())
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

    fn view_items(&self) -> Element<'_, Message> {
        let mut items = column![text("Existing items").size(20)].spacing(12);
        if self.products.is_empty() {
            items = items.push(
                text("No items yet. Add products in Manage inventory.")
                    .size(14)
                    .color(self.muted()),
            );
        }
        for product in &self.products {
            let quantity = self
                .stock
                .iter()
                .find(|stock| stock.product_id == product.id)
                .map_or(0, |stock| stock.quantity);
            let mut add = button("Add").padding([8, 12]).style(primary_button);
            if self.can_modify() && quantity > 0 {
                add = add.on_press(Message::OrderAction(OrderAction::Add(product.id)));
            }
            let info = column![
                text(&product.name).size(16),
                text(format!(
                    "${}.{:02} · {} available",
                    product.price_cents / 100,
                    product.price_cents % 100,
                    quantity
                ))
                .size(13)
                .color(self.muted())
            ]
            .spacing(4)
            .width(Fill);
            items = items.push(
                container(row![info, add].spacing(8).align_y(Alignment::Center))
                    .padding(12)
                    .width(Fill)
                    .style(inset_style),
            );
        }
        scrollable(items).height(Fill).into()
    }

    fn view_sales(&self) -> Element<'_, Message> {
        iced::widget::responsive(move |size| {
            if size.width < 850.0 {
                scrollable(
                    column![
                        container(self.view_items())
                            .width(Fill)
                            .height(320)
                            .padding(16)
                            .style(card_style),
                        container(self.view_order())
                            .width(Fill)
                            .height(400)
                            .padding(16)
                            .style(card_style),
                        container(self.view_history())
                            .width(Fill)
                            .height(320)
                            .padding(16)
                            .style(card_style)
                    ]
                    .spacing(12),
                )
                .height(Fill)
                .into()
            } else {
                row![
                    container(self.view_items())
                        .width(Fill)
                        .height(Fill)
                        .padding(16)
                        .style(card_style),
                    container(self.view_order())
                        .width(Fill)
                        .height(Fill)
                        .padding(16)
                        .style(card_style),
                    container(self.view_history())
                        .width(Fill)
                        .height(Fill)
                        .padding(16)
                        .style(card_style)
                ]
                .spacing(12)
                .height(Fill)
                .into()
            }
        })
        .into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let mut save = button("Save company details")
            .padding([10, 16])
            .style(primary_button);
        if self.can_modify() {
            save = save.on_press(Message::SaveCompany);
        }
        let form = column![
            text("Company details").size(22),
            text("Stored locally on this computer.")
                .size(14)
                .color(self.muted()),
            text("Business name").size(14),
            text_input("Business name", &self.company.business_name)
                .on_input(Message::CompanyName)
                .padding(10),
            text("Tax identifier").size(14),
            text_input("CUIT (optional)", &self.company.tax_identifier)
                .on_input(Message::CompanyTaxId)
                .padding(10),
            text("Address").size(14),
            text_input("Address (optional)", &self.company.address)
                .on_input(Message::CompanyAddress)
                .padding(10),
            text("Contact").size(14),
            text_input("Contact (optional)", &self.company.contact)
                .on_input(Message::CompanyContact)
                .padding(10),
            save,
        ]
        .spacing(10)
        .max_width(640);
        scrollable(container(form).padding(24).width(Fill).style(card_style))
            .height(Fill)
            .into()
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
                    ghost_button
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
                "Manage inventory",
                Message::Orders,
                matches!(self.screen, Screen::Orders)
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
                text("POINT OF SALE").size(10).color(self.muted())
            ]
            .spacing(2),
            container(text("LOCAL").size(11).color(self.muted()))
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
                        .color(self.muted())
                ]
                .spacing(10),
            )
            .width(Fill)
            .padding(24)
            .style(card_style)
            .into(),
            Screen::Settings => self.view_settings(),
            Screen::Orders => self.view_catalog(),
            Screen::Catalog => self.view_sales(),
        };
        let header = row![
            brand,
            iced::widget::Space::new().width(Fill),
            button("Settings")
                .padding([10, 16])
                .style(secondary_button)
                .on_press(Message::Settings)
        ]
        .align_y(Alignment::Center);
        let mut layout = column![header, nav].spacing(16);
        if self.busy {
            layout = layout.push(text("Working…").size(14).color(self.muted()));
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
            layout = layout.push(text(notice).size(14).color(
                if self.theme == ThemePreference::Dark {
                    Color::from_rgb8(134, 239, 172)
                } else {
                    Color::from_rgb8(22, 101, 52)
                },
            ));
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
        let mut theme_button = button(if self.theme == ThemePreference::Light {
            "☾  Night mode"
        } else {
            "☀  Day mode"
        })
        .padding([10, 16])
        .style(secondary_button);
        if self.can_modify() {
            theme_button = theme_button.on_press(Message::ToggleTheme);
        }
        container(
            layout
                .push(container(content).width(Fill).height(Fill))
                .push(theme_button)
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
            Err(CatalogError::storage_message("connection lost")),
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
            Err(CatalogError::storage_message("connection lost")),
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
