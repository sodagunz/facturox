use super::palette::tokens;
use crate::messages::Message;
use iced::{Element, Length, Theme, widget::container};

pub fn page<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style)
        .into()
}

fn style(theme: &Theme) -> container::Style {
    let p = tokens(theme);
    container::Style {
        background: Some(p.page.into()),
        text_color: Some(p.text),
        ..container::Style::default()
    }
}
