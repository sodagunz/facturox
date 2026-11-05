use iced::{Color, Theme};

pub struct Palette {
    pub page: Color,
    pub surface: Color,
    pub inset: Color,
    pub hover: Color,
    pub border: Color,
    pub muted: Color,
    pub text: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub danger: Color,
    pub danger_surface: Color,
}

pub fn theme(dark: bool) -> Theme {
    let p = tokens(if dark { &Theme::Dark } else { &Theme::Light });
    Theme::custom(
        if dark {
            "Facturox Dark"
        } else {
            "Facturox Light"
        },
        iced::theme::Palette {
            background: p.page,
            text: p.text,
            primary: p.primary,
            danger: p.danger,
            ..if dark {
                iced::theme::Palette::DARK
            } else {
                iced::theme::Palette::LIGHT
            }
        },
    )
}

pub fn tokens(theme: &Theme) -> Palette {
    if theme.extended_palette().is_dark {
        Palette {
            page: Color::from_rgb8(9, 9, 11),
            surface: Color::from_rgb8(18, 18, 21),
            inset: Color::from_rgb8(27, 27, 30),
            hover: Color::from_rgb8(45, 45, 49),
            border: Color::from_rgb8(63, 63, 70),
            muted: Color::from_rgb8(161, 161, 170),
            text: Color::from_rgb8(250, 250, 250),
            primary: Color::from_rgb8(244, 244, 245),
            on_primary: Color::from_rgb8(24, 24, 27),
            danger: Color::from_rgb8(248, 113, 113),
            danger_surface: Color::from_rgb8(48, 20, 24),
        }
    } else {
        Palette {
            page: Color::from_rgb8(250, 250, 250),
            surface: Color::WHITE,
            inset: Color::from_rgb8(244, 244, 245),
            hover: Color::from_rgb8(228, 228, 231),
            border: Color::from_rgb8(212, 212, 216),
            muted: Color::from_rgb8(113, 113, 122),
            text: Color::from_rgb8(24, 24, 27),
            primary: Color::from_rgb8(24, 24, 27),
            on_primary: Color::WHITE,
            danger: Color::from_rgb8(185, 28, 28),
            danger_surface: Color::from_rgb8(254, 242, 242),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_theme_uses_the_component_palette() {
        for dark in [false, true] {
            let theme = theme(dark);
            let p = tokens(&theme);
            assert_eq!(theme.extended_palette().is_dark, dark);
            assert_eq!(theme.palette().primary, p.primary);
            assert_eq!(theme.palette().background, p.page);
        }
    }
}
