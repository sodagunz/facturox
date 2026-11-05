use super::palette::tokens;
use crate::messages::Message;
use iced::{Border, Element, Length, Theme, widget::container};

pub fn alert<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(16)
        .width(Length::Fill)
        .style(style)
        .into()
}

fn style(theme: &Theme) -> container::Style {
    let p = tokens(theme);
    container::Style {
        background: Some(p.danger_surface.into()),
        text_color: Some(p.danger),
        border: Border {
            color: p.danger,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    }
}
