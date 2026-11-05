use super::palette::tokens;
use crate::messages::Message;
use iced::{Border, Element, Length, Theme, widget::container};

pub fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(22)
        .width(Length::Fill)
        .style(style)
        .into()
}

pub fn inset<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(14)
        .width(Length::Fill)
        .style(|theme| {
            let p = tokens(theme);
            container::Style {
                background: Some(p.inset.into()),
                ..style(theme)
            }
        })
        .into()
}

fn style(theme: &Theme) -> container::Style {
    let p = tokens(theme);
    container::Style {
        background: Some(p.surface.into()),
        text_color: Some(p.text),
        border: Border {
            color: p.border,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    }
}
