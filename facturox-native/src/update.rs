use crate::{
    Draft,
    messages::{BackendHandle, Message},
    state::{App, Field},
};
use facturox_core::{
    backend::{Backend, Error, Receipt, Request},
    domain::{Command, Product},
};
use iced::{Task, widget};

pub fn pesos(value: &str) -> Result<i64, String> {
    let normalized = value.trim().replace(',', ".");
    let (whole, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("Enter a nonnegative peso amount, e.g. 1250.50".into());
    }
    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|e| e.to_string())? * 10,
        _ => fraction.parse::<i64>().map_err(|e| e.to_string())?,
    };
    whole
        .parse::<i64>()
        .ok()
        .and_then(|n| n.checked_mul(100))
        .and_then(|n| n.checked_add(cents))
        .ok_or_else(|| "Price overflow".into())
}
pub fn total(draft: &Draft) -> Result<i64, String> {
    draft.lines.iter().try_fold(0_i64, |sum, line| {
        line.unit_price
            .checked_mul(line.quantity)
            .and_then(|n| sum.checked_add(n))
            .ok_or_else(|| "Draft total overflow".into())
    })
}
// Keep the identical request across an ambiguous response, including its operation ID.
pub async fn execute_with_recovery<F, Fut>(
    request: Request,
    mut execute: F,
) -> Result<Receipt, Error>
where
    F: FnMut(Request) -> Fut,
    Fut: std::future::Future<Output = Result<Receipt, Error>>,
{
    match execute(request.clone()).await {
        Err(Error::Uncertain(_)) => execute(request).await,
        result => result,
    }
}

fn optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}
impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let task = app.update(Message::Open);
        (app, task)
    }
    fn submit(&mut self, command: Command) -> Task<Message> {
        let Some(backend) = self.backend.clone() else {
            return Task::none();
        };
        let request = Request {
            operation_id: uuid::Uuid::new_v4().to_string(),
            command,
        };
        self.pending = Some(request.clone());
        self.error.clear();
        let token = self.begin();
        let id = request.operation_id.clone();
        Task::perform(
            execute_with_recovery(request, move |request| {
                let backend = backend.clone();
                async move { backend.execute(request).await }
            }),
            move |result| Message::Completed(token, id, result),
        )
    }
    fn synchronize(&mut self) -> Task<Message> {
        self.stale = true;
        self.error = "Operation recorded, but local data could not be updated. Changes are locked until data is synchronized.".into();
        let Some(backend) = self.backend.clone() else {
            return Task::none();
        };
        let token = self.begin();
        Task::perform(async move { backend.snapshot().await }, move |result| {
            Message::Synchronized(token, result)
        })
    }
    fn committed(&mut self, receipt: &Receipt, request: &Request) -> bool {
        // A successful backend result is definitive even if this read model is stale.
        match request.command {
            Command::CreateOrder { .. } => self.draft = Draft::default(),
            Command::CreateProduct { .. } => {
                self.name.clear();
                self.barcode.clear();
                self.price.clear();
                self.opening.clear();
            }
            Command::AdjustProductQuantity { .. } => {
                self.delta.clear();
                self.reason.clear();
            }
        }
        let mut next = self.inventory.clone();
        if receipt.events.is_empty() || receipt.events.len() != receipt.envelopes.len() {
            return false;
        }
        for (event, envelope) in receipt.events.iter().zip(&receipt.envelopes) {
            if envelope.operation_id != request.operation_id
                || envelope.event != *event
                || envelope.occurred_at < 0
                || next.order.checked_add(1) != Some(envelope.order)
                || next.apply(event).is_err()
            {
                return false;
            }
        }
        if next.order != receipt.order {
            return false;
        }
        self.inventory = next;
        true
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Explicit Elm transitions keep asynchronous operation guards visible."
    )]
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => self.screen = screen,
            Message::Resize(width) => self.width = width,
            Message::Focus(backwards) => {
                return if backwards {
                    widget::operation::focus_previous()
                } else {
                    widget::operation::focus_next()
                };
            }
            Message::Open if self.backend.is_none() && self.active.is_none() => {
                let token = self.begin();
                return Task::perform(
                    async {
                        let backend = Backend::open_default().await?;
                        let inventory = backend.snapshot().await?;
                        let company = backend.load_company_details().await?;
                        let dark = backend.dark_theme().await?;
                        Ok((BackendHandle(backend), inventory, company, dark))
                    },
                    move |result| Message::Loaded(token, result),
                );
            }
            Message::Loaded(token, result) if self.active == Some(token) => {
                self.active = None;
                match result {
                    Ok((backend, inventory, company, dark)) => {
                        self.backend = Some(backend.0);
                        self.inventory = inventory;
                        self.company = company;
                        self.dark = dark;
                        self.error.clear();
                    }
                    Err(error) => self.error = format!("{error:?}"),
                }
            }
            Message::Completed(token, id, result)
                if self.active == Some(token)
                    && self.pending.as_ref().is_some_and(|p| p.operation_id == id) =>
            {
                self.active = None;
                match result {
                    Ok(receipt) => {
                        if let Some(request) = self.pending.take() {
                            if !self.committed(&receipt, &request) {
                                return self.synchronize();
                            }
                            self.error.clear();
                        }
                    }
                    Err(Error::Uncertain(error)) => {
                        self.error = format!(
                            "Operation outcome could not be confirmed: {error}. Changes are locked. Inspect recorded history before restarting or submitting this transaction again; it may already be recorded."
                        );
                    }

                    Err(Error::Rejected(error)) => {
                        self.pending = None;
                        self.error = error;
                    }
                }
            }
            Message::Synchronized(token, result)
                if self.active == Some(token) && self.stale && self.pending.is_none() =>
            {
                self.active = None;
                match result {
                    Ok(inventory) => {
                        self.inventory = inventory;
                        self.stale = false;
                        self.error.clear();
                    }
                    Err(error) => {
                        self.error = format!(
                            "Operation recorded, but local data synchronization failed: {error:?}. Changes remain locked to prevent duplicate recording."
                        );
                    }
                }
            }
            Message::CompanySaved(token, result) if self.active == Some(token) => {
                self.active = None;
                match result {
                    Ok(details) => {
                        self.company = details;
                        self.error.clear();
                    }
                    Err(error) => {
                        self.error = format!("Company details not confirmed saved: {error:?}");
                    }
                }
            }
            Message::ThemeSaved(token, result) if self.active == Some(token) => {
                self.active = None;
                match result {
                    Ok(dark) => self.dark = dark,
                    Err(error) => self.error = format!("{error:?}"),
                }
            }
            local
                if self.active.is_none()
                    && self.pending.is_none()
                    && !self.stale
                    && matches!(
                        local,
                        Message::Edit(..)
                            | Message::Select(_)
                            | Message::Add(_)
                            | Message::Minus(_)
                            | Message::Discard
                    ) =>
            {
                match local {
                    Message::Edit(field, value) => {
                        *match field {
                            Field::Name => &mut self.name,
                            Field::Barcode => &mut self.barcode,
                            Field::Price => &mut self.price,
                            Field::Opening => &mut self.opening,
                            Field::Delta => &mut self.delta,
                            Field::Reason => &mut self.reason,
                            Field::Business => &mut self.company.business_name,
                            Field::Tax => &mut self.company.tax_identifier,
                            Field::Address => &mut self.company.address,
                            Field::Contact => &mut self.company.contact,
                        } = value;
                    }
                    Message::Select(id) => {
                        if self.selected.as_ref() != Some(&id) {
                            self.delta.clear();
                            self.reason.clear();
                        }
                        self.selected = Some(id);
                    }
                    Message::Add(id) => {
                        if let Some(product) = self.inventory.products.get(&id) {
                            let mut next = self.draft.clone();
                            match next.add(product).and_then(|()| total(&next).map(|_| ())) {
                                Ok(()) => self.draft = next,
                                Err(error) => self.error = error,
                            }
                        }
                    }
                    Message::Minus(id) => {
                        if let Some(line) = self.draft.lines.iter_mut().find(|l| l.product_id == id)
                        {
                            line.quantity = line.quantity.saturating_sub(1);
                        }
                        self.draft.lines.retain(|l| l.quantity > 0);
                    }
                    Message::Discard => self.draft = Draft::default(),
                    _ => {}
                }
            }
            other if self.editable() => match other {
                Message::Checkout if !self.draft.lines.is_empty() => {
                    return self.submit(Command::CreateOrder {
                        id: uuid::Uuid::new_v4().to_string(),
                        lines: self.draft.lines.clone(),
                    });
                }
                Message::Create => {
                    match (pesos(&self.price), self.opening.trim().parse::<i64>().or_else(|e| if self.opening.trim().is_empty() { Ok(0) } else { Err(e) })) {
                        (Ok(price), Ok(opening_quantity)) if opening_quantity >= 0 => return self.submit(Command::CreateProduct { product: Product { id: uuid::Uuid::new_v4().to_string(), name: self.name.trim().into(), barcode: optional(&self.barcode), price }, opening_quantity }),
                        _ => self.error = "Enter a decimal peso price and nonnegative whole-unit opening quantity.".into(),
                    }
                }
                Message::Adjust => {
                    match (self.selected.clone(), self.delta.trim().parse::<i64>()) {
                        (Some(product_id), Ok(delta)) if delta != 0 => {
                            return self.submit(Command::AdjustProductQuantity {
                                product_id,
                                delta,
                                reason: optional(&self.reason),
                            });
                        }
                        _ => {
                            self.error =
                                "Select a product and enter a nonzero signed whole-unit adjustment."
                                    .into();
                        }
                    }
                }
                Message::SaveCompany => {
                    if let Some(backend) = self.backend.clone() {
                        let details = self.company.clone();
                        let token = self.begin();
                        return Task::perform(
                            async move { backend.save_company_details(&details).await },
                            move |result| Message::CompanySaved(token, result),
                        );
                    }
                }
                Message::Toggle => {
                    if let Some(backend) = self.backend.clone() {
                        let dark = !self.dark;
                        let token = self.begin();
                        return Task::perform(
                            async move { backend.set_dark_theme(dark).await.map(|()| dark) },
                            move |result| Message::ThemeSaved(token, result),
                        );
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Task::none()
    }
}
