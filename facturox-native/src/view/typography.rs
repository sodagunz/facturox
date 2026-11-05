use super::palette::tokens;
use iced::{Theme, font, widget::text};

pub fn heading(value: &str) -> text::Text<'_> {
    text(value).size(23).font(iced::Font {
        weight: font::Weight::Semibold,
        ..iced::Font::DEFAULT
    })
}

pub fn muted<'a>(value: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(value).size(13).style(|theme: &Theme| text::Style {
        color: Some(tokens(theme).muted),
    })
}
