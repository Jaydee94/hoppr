//! Centralized design tokens for the hoppr UI.
//!
//! The palette is inspired by modern developer tooling (Linear, Vercel, Raycast):
//! a deep, near-black canvas with electric accent colors and tight contrast tiers
//! so that focus, selection and status all read clearly at a glance.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_muted: Color,
    pub primary: Color,
    pub primary_glow: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
}

impl Theme {
    pub const fn midnight() -> Self {
        Self {
            bg: Color::Rgb(0x0a, 0x0b, 0x10),
            surface: Color::Rgb(0x12, 0x13, 0x1a),
            surface_alt: Color::Rgb(0x1a, 0x1c, 0x25),
            border: Color::Rgb(0x25, 0x28, 0x34),
            text: Color::Rgb(0xf1, 0xf3, 0xf9),
            text_dim: Color::Rgb(0x8a, 0x92, 0xa6),
            text_muted: Color::Rgb(0x55, 0x5c, 0x70),
            primary: Color::Rgb(0x7c, 0x5c, 0xff),
            primary_glow: Color::Rgb(0xa7, 0x8a, 0xff),
            accent: Color::Rgb(0x00, 0xe5, 0xff),
            success: Color::Rgb(0x00, 0xd6, 0x8f),
            warning: Color::Rgb(0xff, 0xb5, 0x47),
            error: Color::Rgb(0xff, 0x54, 0x70),
        }
    }

    pub fn base(&self) -> Style {
        Style::default().fg(self.text).bg(self.bg)
    }

    pub fn border_style(&self, active: bool) -> Style {
        let color = if active { self.primary } else { self.border };
        Style::default().fg(color).bg(self.bg)
    }

    /// Bold text in `color`, leaving the background to the widget.
    pub fn bold(color: Color) -> Style {
        Style::new().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn title(&self) -> Style {
        Self::bold(self.primary_glow)
    }

    pub fn key(&self) -> Style {
        Self::bold(self.accent)
    }

    pub fn cursor(&self) -> Style {
        Style::new()
            .fg(self.accent)
            .add_modifier(Modifier::SLOW_BLINK)
    }

    pub fn selectable(&self, selected: bool) -> Style {
        if selected {
            Self::bold(self.text)
        } else {
            Style::new().fg(self.text_dim)
        }
    }

    pub fn panel(&self) -> Style {
        Style::new().bg(self.surface)
    }

    pub fn button_fill(&self, active: bool) -> Style {
        Style::new().bg(if active {
            self.surface_alt
        } else {
            self.surface
        })
    }

    pub fn panel_border(&self, color: Color) -> Style {
        Style::new().fg(color).bg(self.surface)
    }

    pub fn input_border(&self, active: bool, invalid: bool) -> Color {
        if invalid {
            self.error
        } else if active {
            self.primary
        } else {
            self.border
        }
    }

    pub fn highlight_primary(&self) -> Style {
        Style::default()
            .fg(self.primary_glow)
            .bg(self.surface_alt)
            .add_modifier(Modifier::BOLD)
    }

    pub fn highlight_accent(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(self.surface_alt)
            .add_modifier(Modifier::BOLD)
    }
}

pub const ACTIVE_GLYPH: &str = "▍ ";
pub const INACTIVE_GLYPH: &str = "  ";

/// Leading marker of a list row: a bar for the selected row, padding of
/// the same width otherwise.
pub const fn selection_glyph(selected: bool) -> &'static str {
    if selected {
        ACTIVE_GLYPH
    } else {
        INACTIVE_GLYPH
    }
}
