use super::palette::tokens;
use crate::{messages::Message, state::Field};
use iced::{
    Border, Element, Theme,
    widget::{column, text, text_input},
};

pub fn input<'a>(
    label: &'a str,
    value: &'a str,
    field: Field,
    enabled: bool,
) -> Element<'a, Message> {
    let input = text_input(label, value)
        .padding([11, 12])
        .size(14)
        .style(style);
    column![
        text(label).size(13),
        if enabled {
            input.on_input(move |value| Message::Edit(field, value))
        } else {
            input
        }
    ]
    .spacing(7)
    .into()
}

fn style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let p = tokens(theme);
    let disabled = matches!(status, text_input::Status::Disabled);
    text_input::Style {
        background: if disabled { p.inset } else { p.surface }.into(),
        border: Border {
            color: match status {
                text_input::Status::Focused { .. } => p.text,
                text_input::Status::Hovered => p.muted,
                _ => p.border,
            },
            width: if matches!(status, text_input::Status::Focused { .. }) {
                2.0
            } else {
                1.0
            },
            radius: 7.0.into(),
        },
        icon: p.muted,
        placeholder: p.muted,
        value: if disabled { p.muted } else { p.text },
        selection: p.hover,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_use_neutral_focus_and_disabled_colors() {
        for theme in [Theme::Light, Theme::Dark] {
            let p = tokens(&theme);
            let focused = style(&theme, text_input::Status::Focused { is_hovered: false });
            assert_eq!(focused.border.color, p.text);
            let disabled = style(&theme, text_input::Status::Disabled);
            assert_eq!(disabled.value, p.muted);
            assert_eq!(disabled.background, p.inset.into());
        }
    }
}
