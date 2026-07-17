//! The capability-independent Block IR — pure data, no depth, no width.
//!
//! A [`Fragment`] is a run of text that names a [`Role`] (never a color).
//! Width is always measurable pre-emit because text and role are separate
//! (the escape-blind-width trap closed by construction). A [`StyledLine`]
//! is a row of fragments. A block lays itself out into `Vec<StyledLine>`
//! given only a [`Capability`](crate::caps::Capability) + [`Theme`] — the
//! same value degrades to every tier.

use unicode_width::UnicodeWidthStr;

use crate::theme::Role;

/// One styled run of text. Carries a role, not a color, so width is
/// measurable on the un-escaped text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub text: String,
    pub role: Option<Role>,
    pub bold: bool,
    pub dim: bool,
}

impl Fragment {
    /// Unstyled text.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), role: None, bold: false, dim: false }
    }

    /// Text in a role.
    #[must_use]
    pub fn styled(text: impl Into<String>, role: Role) -> Self {
        Self { text: text.into(), role: Some(role), bold: false, dim: false }
    }

    /// A bold accent fragment (used sparingly — restraint is the house style).
    #[must_use]
    pub fn accent(text: impl Into<String>, role: Role) -> Self {
        Self { text: text.into(), role: Some(role), bold: true, dim: false }
    }

    /// A dimmed fragment.
    #[must_use]
    pub fn faint(text: impl Into<String>, role: Role) -> Self {
        Self { text: text.into(), role: Some(role), bold: false, dim: true }
    }

    /// Display width of the text (grapheme/CJK-aware, escape-free).
    #[must_use]
    pub fn width(&self) -> usize {
        UnicodeWidthStr::width(self.text.as_str())
    }
}

/// A rendered line: an ordered row of fragments.
pub type StyledLine = Vec<Fragment>;

/// Total display width of a styled line.
#[must_use]
pub fn line_width(line: &StyledLine) -> usize {
    line.iter().map(Fragment::width).sum()
}

/// The load-bearing severity ladder — referenced by callouts, list task
/// markers, badges, status blocks. State-only Aurora colors.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CalloutSeverity {
    Note,
    Info,
    Ok,
    Warn,
    Error,
    Debug,
}

impl CalloutSeverity {
    /// Every severity, for the verification matrix + catalog.
    pub const ALL: [CalloutSeverity; 6] = [
        CalloutSeverity::Note,
        CalloutSeverity::Info,
        CalloutSeverity::Ok,
        CalloutSeverity::Warn,
        CalloutSeverity::Error,
        CalloutSeverity::Debug,
    ];

    /// The role this severity resolves through.
    #[must_use]
    pub const fn role(self) -> Role {
        match self {
            CalloutSeverity::Note => Role::TextMuted,
            CalloutSeverity::Info => Role::Info,
            CalloutSeverity::Ok => Role::Ok,
            CalloutSeverity::Warn => Role::Warn,
            CalloutSeverity::Error => Role::Error,
            CalloutSeverity::Debug => Role::TextDim,
        }
    }

    /// The uppercase tag used in the NO_COLOR / non-unicode projection:
    /// `[ERROR] message`.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            CalloutSeverity::Note => "NOTE",
            CalloutSeverity::Info => "INFO",
            CalloutSeverity::Ok => "OK",
            CalloutSeverity::Warn => "WARN",
            CalloutSeverity::Error => "ERROR",
            CalloutSeverity::Debug => "DEBUG",
        }
    }

    /// The unicode glyph used on a capable terminal.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            CalloutSeverity::Note => "•",
            CalloutSeverity::Info => "ℹ",
            CalloutSeverity::Ok => "✓",
            CalloutSeverity::Warn => "⚠",
            CalloutSeverity::Error => "✗",
            CalloutSeverity::Debug => "»",
        }
    }
}
