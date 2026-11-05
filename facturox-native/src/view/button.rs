use super::palette::tokens;
use crate::messages::Message;
use iced::{
    Border, Color, Element, Theme,
    widget::{button, text},
};

#[derive(Clone, Copy)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
    Destructive,
}

pub fn action(label: &str, message: Message, enabled: bool) -> Element<'_, Message> {
    control(label, message, enabled, Variant::Primary)
}

pub fn control<'a>(
    label: impl text::IntoFragment<'a>,
    message: Message,
    enabled: bool,
    variant: Variant,
) -> Element<'a, Message> {
    button(text(label).size(14))
        .padding([10, 16])
        .style(move |theme, status| style(theme, status, variant))
        .on_press_maybe(enabled.then_some(message))
        .into()
}

pub fn selected<'a>(
    label: impl text::IntoFragment<'a>,
    message: Message,
    enabled: bool,
    selected: bool,
) -> Element<'a, Message> {
    control(
        label,
        message,
        enabled,
        if selected {
            Variant::Secondary
        } else {
            Variant::Ghost
        },
    )
}

fn style(theme: &Theme, status: button::Status, variant: Variant) -> button::Style {
    let p = tokens(theme);
    let (background, foreground) = match variant {
        Variant::Primary => (p.primary, p.on_primary),
        Variant::Secondary => (p.inset, p.text),
        Variant::Ghost => (Color::TRANSPARENT, p.text),
        Variant::Destructive => (p.danger_surface, p.danger),
    };
    let background = match (variant, status) {
        (_, button::Status::Disabled) | (Variant::Ghost, button::Status::Hovered) => p.inset,
        (Variant::Ghost | Variant::Secondary, button::Status::Pressed)
        | (Variant::Secondary, button::Status::Hovered) => p.hover,
        (_, button::Status::Hovered) => background.scale_alpha(0.85),
        (_, button::Status::Pressed) => background.scale_alpha(0.65),
        (_, button::Status::Active) => background,
    };
    button::Style {
        background: Some(background.into()),
        text_color: if matches!(status, button::Status::Disabled) {
            p.muted
        } else {
            foreground
        },
        border: Border {
            color: if matches!(variant, Variant::Ghost) {
                background
            } else {
                p.border
            },
            width: 1.0,
            radius: 7.0.into(),
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_hover_is_visible_and_disabled_buttons_are_muted() {
        for theme in [Theme::Light, Theme::Dark] {
            let active = style(&theme, button::Status::Active, Variant::Ghost);
            let hovered = style(&theme, button::Status::Hovered, Variant::Ghost);
            assert_ne!(active.background, hovered.background);
            let disabled = style(&theme, button::Status::Disabled, Variant::Primary);
            assert_eq!(disabled.text_color, tokens(&theme).muted);
        }
    }
}
