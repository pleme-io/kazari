//! The typed SGR atom — the one place role + capability become bytes.
//!
//! Emission goes through `anstyle`'s tested `Display`: kazari never spells
//! `\x1b` and never `format!()`s a sequence (TYPED EMISSION). A `StyleAtom`
//! is produced by the total morphism [`StyleAtom::resolve`], which binds a
//! [`Role`] → [`Rgb`] (via the theme) → degrades through the
//! [`Capability`]'s [`ColorLevel`] → an `anstyle` color. At
//! [`ColorLevel::None`] the atom is empty — zero SGR bytes.

use std::io::{self, Write};

use crate::caps::{Capability, ColorLevel, Rgb};
use crate::theme::{Role, Theme};

/// A resolved, capability-degraded style. Newtype over `anstyle::Style` so
/// the only escape bytes in the crate come from anstyle's serializer.
#[derive(Clone, Copy, Debug, Default)]
pub struct StyleAtom(anstyle::Style);

impl StyleAtom {
    /// The empty style — plain text, no SGR.
    #[must_use]
    pub fn none() -> Self {
        Self(anstyle::Style::new())
    }

    /// The total morphism: (role, weight) × capability → a concrete style.
    /// At `ColorLevel::None` this is [`StyleAtom::none`] — the wall makes
    /// "colored on a pipe" unrepresentable, because the caps come from the
    /// environment, not the block.
    #[must_use]
    pub fn resolve(role: Role, theme: Theme, caps: &Capability, bold: bool, dim: bool) -> Self {
        if caps.level == ColorLevel::None {
            return Self::none();
        }
        let rgb = theme.color(role);
        let mut style = anstyle::Style::new().fg_color(Some(rgb.to_anstyle(caps.level)));
        if bold {
            style = style.bold();
        }
        if dim {
            style = style.dimmed();
        }
        Self(style)
    }

    /// A style straight from a concrete color (the inline-painting path used
    /// by adapters that carry an `Rgb`, not a role — e.g. a tool migrating
    /// its own hand-rolled palette onto kazari). Degrades through the
    /// capability just like [`StyleAtom::resolve`].
    #[must_use]
    pub fn from_rgb(rgb: Rgb, caps: &Capability, bold: bool, dim: bool) -> Self {
        if caps.level == ColorLevel::None {
            return Self::none();
        }
        let mut style = anstyle::Style::new().fg_color(Some(rgb.to_anstyle(caps.level)));
        if bold {
            style = style.bold();
        }
        if dim {
            style = style.dimmed();
        }
        Self(style)
    }

    /// Render `text` in this style into a `String` (the inline API). Infallible
    /// — the sink is an in-memory buffer.
    #[must_use]
    pub fn paint(self, text: &str) -> String {
        let mut buf: Vec<u8> = Vec::with_capacity(text.len() + 16);
        // Infallible over a Vec.
        let _ = self.write_into(&mut buf, text);
        String::from_utf8(buf).unwrap_or_else(|_| text.to_string())
    }

    /// Write `text` wrapped in this style's SGR set/reset. When the style is
    /// empty, only the plain text is written (no escape bytes at all).
    pub fn write_into(self, w: &mut dyn Write, text: &str) -> io::Result<()> {
        // anstyle's Display renders the set prefix; render_reset the reset.
        // write! is the sanctioned typed surface; the escape bytes are
        // anstyle's, never hand-spelled.
        let set = self.0.render();
        let reset = self.0.render_reset();
        write!(w, "{set}")?;
        w.write_all(text.as_bytes())?;
        write!(w, "{reset}")
    }
}

impl Rgb {
    /// Lower to an `anstyle::Color` at the given depth (the degrade step).
    #[must_use]
    pub fn to_anstyle(self, level: ColorLevel) -> anstyle::Color {
        match level {
            ColorLevel::Truecolor => {
                anstyle::Color::Rgb(anstyle::RgbColor(self.r, self.g, self.b))
            }
            ColorLevel::Ansi256 => {
                anstyle::Color::Ansi256(anstyle::Ansi256Color(self.to_ansi256()))
            }
            // At Ansi16 / None the caller has already gated None; map to the
            // nearest ANSI slot for Ansi16.
            ColorLevel::Ansi16 | ColorLevel::None => anstyle::Color::Ansi(self.to_ansi16()),
        }
    }
}
