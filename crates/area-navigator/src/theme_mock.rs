use iced::{Border, Color, Shadow, Theme, Vector};
use iced::widget::{button, container, text_input};

pub mod colors {
    use iced::Color;
    
    pub const CONTROL_CLOSE: Color = Color::from_rgb(0.9, 0.4, 0.4);
    pub const CONTROL_MIN: Color = Color::from_rgb(0.9, 0.7, 0.4);
    pub const CONTROL_MAX: Color = Color::from_rgb(0.4, 0.8, 0.4);
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.7, 0.7, 0.7);
    pub const ACCENT_PRIMARY: Color = Color::from_rgb(0.4, 0.6, 1.0);
}

pub mod styles {
    use iced::{Border, Color, Shadow, Theme, Vector};
    use iced::widget::{button, container, text_input};
    use super::colors;

    pub fn window_control(_theme: &Theme, status: button::Status, color: Color) -> button::Style {
        let base = button::Style {
            background: Some(color.into()),
            border: Border {
                radius: 6.0.into(),
                ..Border::default()
            },
            ..button::Style::default()
        };
        
        match status {
            button::Status::Hovered => button::Style {
                background: Some(Color { a: 0.8, ..color }.into()),
                ..base
            },
            _ => base,
        }
    }

    pub fn app_card(_theme: &Theme, status: button::Status) -> button::Style {
        let base = button::Style {
            background: Some(Color::from_rgba(0.2, 0.2, 0.2, 0.5).into()),
            border: Border {
                radius: 8.0.into(),
                color: Color::WHITE,
                width: 0.0,
            },
            text_color: Color::WHITE,
            ..button::Style::default()
        };

        match status {
            button::Status::Hovered => button::Style {
                background: Some(Color::from_rgba(0.3, 0.3, 0.3, 0.8).into()),
                border: Border {
                    width: 1.0,
                    ..base.border
                },
                ..base
            },
            _ => base,
        }
    }

    pub fn search_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
        let base = text_input::Style {
            background: Color::from_rgba(0.1, 0.1, 0.1, 0.8).into(),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: Color::from_rgba(0.3, 0.3, 0.3, 0.5),
            },
            icon: Color::WHITE,
            placeholder: Color::from_rgba(0.7, 0.7, 0.7, 0.5),
            value: Color::WHITE,
            selection: Color::from_rgba(0.4, 0.6, 1.0, 0.3),
        };

        match status {
            text_input::Status::Focused => text_input::Style {
                border: Border {
                    color: colors::ACCENT_PRIMARY,
                    ..base.border
                },
                ..base
            },
            _ => base,
        }
    }

    pub fn glass_base(_theme: &Theme) -> container::Style {
        container::Style {
            background: Some(Color::from_rgba(0.05, 0.05, 0.05, 0.9).into()),
            border: Border {
                radius: 12.0.into(),
                width: 1.0,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.1),
            },
            ..container::Style::default()
        }
    }

    pub fn glass_highlight_top(_theme: &Theme) -> container::Style {
        container::Style::default() // Simplified
    }
    
    pub fn glass_highlight_bottom(_theme: &Theme) -> container::Style {
        container::Style::default()
    }
    
    pub fn glass_highlight_left(_theme: &Theme) -> container::Style {
        container::Style::default()
    }
    
    pub fn glass_highlight_right(_theme: &Theme) -> container::Style {
        container::Style::default()
    }
}
