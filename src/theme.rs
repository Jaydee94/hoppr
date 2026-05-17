//! Centralized design tokens for the hoppr UI.
//!
//! The palette is inspired by modern developer tooling (Linear, Vercel, Raycast):
//! a deep, near-black canvas with electric accent colors and tight contrast tiers
//! so that focus, selection and status all read clearly at a glance.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub border_strong: Color,
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

#[allow(dead_code)]
impl Theme {
    pub const fn midnight() -> Self {
        Self {
            bg: Color::Rgb(0x0a, 0x0b, 0x10),
            surface: Color::Rgb(0x12, 0x13, 0x1a),
            surface_alt: Color::Rgb(0x1a, 0x1c, 0x25),
            border: Color::Rgb(0x25, 0x28, 0x34),
            border_strong: Color::Rgb(0x3a, 0x3e, 0x4e),
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

    pub fn surface_style(&self) -> Style {
        Style::default().fg(self.text).bg(self.surface)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_dim).bg(self.bg)
    }

    pub fn dim(&self) -> Style {
        Style::default().fg(self.text_muted).bg(self.bg)
    }

    pub fn border_style(&self, active: bool) -> Style {
        let color = if active { self.primary } else { self.border };
        Style::default().fg(color).bg(self.bg)
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
