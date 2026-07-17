//! # kazari (飾り) — the line-output face of the fleet
//!
//! kazari is pleme-io's **line-output typescape**: the styled output of an
//! ordinary CLI tool — banners, callouts, panels, rules, lists, badges (and,
//! at M2, tables / trees / progress / diagnostics) — as a typed, pure-data
//! [`ir::Block`] set that renders **beautiful, dark-Nord, capability-honest,
//! and `format!()`-free by construction**.
//!
//! It is the **sibling of seki** (which dresses the prompt, *before* the
//! command) — kazari dresses the output, *after* the command — over the one
//! ishou/irodori token spine that also feeds egaku-term (the cell-grid) and
//! tela (the web). It CONSUMES `irodori` for the Nord palette (no fork),
//! WRAPS `anstyle` for typed SGR emission, and OWNS exactly the three
//! net-new algebras nothing in the fleet supplied: the [`caps::ColorLevel`]
//! capability wall, the RGB→256→16 linear-space quantizer, and the
//! line-block layout AST.
//!
//! ## The wall as a type
//!
//! A [`ir::Block`]-shaped value carries **no** color depth. The only place
//! the wall is consulted is at emit, through a [`caps::Capability`] supplied
//! by the [`render::RenderEnvironment`] — so "assumed truecolor on a pipe"
//! is a compile-shaped absence on the color axis, not a runtime clamp. The
//! same value degrades to every tier through one total morphism.
//!
//! ## 3-line adoption
//!
//! ```no_run
//! use kazari::prelude::*;
//! error("build failed: missing input").print().unwrap();
//! banner("sui — Rust-native Nix").print().unwrap();
//! Panel::titled("store").row("path", "/nix/store/…").row("size", "42 MiB").print().unwrap();
//! ```

pub mod blocks;
pub mod caps;
pub mod ir;
pub mod render;
pub mod style;
pub mod theme;

pub use blocks::{Badge, Banner, Callout, List, ListItem, Panel, Rule};
pub use caps::{Background, Capability, ColorLevel, Rgb};
pub use ir::{CalloutSeverity, Fragment, StyledLine};
pub use render::{render, BlockRender, MockEnvironment, RenderEnvironment, TerminalEnvironment};
pub use theme::{Role, Theme};

/// Probe the real terminal's capabilities (sealed precedence fold).
#[must_use]
pub fn caps() -> Capability {
    Capability::probe()
}

// ── ergonomic constructors ────────────────────────────────────────────────

/// A callout at an explicit severity.
#[must_use]
pub fn callout(severity: CalloutSeverity, message: impl Into<String>) -> Callout {
    Callout::new(severity, message)
}

/// `✗ error` — Aurora red.
#[must_use]
pub fn error(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Error, message)
}

/// `⚠ warn` — Aurora orange.
#[must_use]
pub fn warn(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Warn, message)
}

/// `✓ ok` — Aurora green.
#[must_use]
pub fn ok(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Ok, message)
}

/// `ℹ info` — Frost blue.
#[must_use]
pub fn info(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Info, message)
}

/// `• note` — muted.
#[must_use]
pub fn note(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Note, message)
}

/// `» debug` — dim.
#[must_use]
pub fn debug(message: impl Into<String>) -> Callout {
    Callout::new(CalloutSeverity::Debug, message)
}

/// A framed title banner.
#[must_use]
pub fn banner(title: impl Into<String>) -> Banner {
    Banner::new(title)
}

/// A plain full-width divider.
#[must_use]
pub fn rule() -> Rule {
    Rule::new()
}

/// A labelled divider (`── label ────`).
#[must_use]
pub fn rule_labelled(label: impl Into<String>) -> Rule {
    Rule::labelled(label)
}

/// An aligned key→value panel builder.
#[must_use]
pub fn panel() -> Panel {
    Panel::new()
}

/// A list builder.
#[must_use]
pub fn list() -> List {
    List::new()
}

/// An inline `[ text ]` badge.
#[must_use]
pub fn badge(text: impl Into<String>, role: Role) -> Badge {
    Badge::new(text, role)
}

// ── inline painting (the adapter path — style a run of text, not a block) ──

/// Style `text` in a semantic [`Role`] at the given capability, returning a
/// `String`. The low-level path for tools composing their own lines.
#[must_use]
pub fn paint_at(text: &str, role: Role, caps: &Capability) -> String {
    style::StyleAtom::resolve(role, Theme::default(), caps, false, false).paint(text)
}

/// Style `text` in a semantic [`Role`] at the *probed* terminal capability.
#[must_use]
pub fn paint(text: &str, role: Role) -> String {
    paint_at(text, role, &Capability::probe())
}

/// Paint `text` from a concrete [`Rgb`] (bold/dim) at the given capability —
/// the migration path for a tool carrying its own palette onto kazari.
/// Degrades truecolor → 256 → 16 → plain by construction.
#[must_use]
pub fn paint_rgb_at(rgb: Rgb, text: &str, bold: bool, dim: bool, caps: &Capability) -> String {
    style::StyleAtom::from_rgb(rgb, caps, bold, dim).paint(text)
}

/// Paint `text` from a concrete [`Rgb`] at the *probed* terminal capability.
#[must_use]
pub fn paint_rgb(rgb: Rgb, text: &str, bold: bool, dim: bool) -> String {
    paint_rgb_at(rgb, text, bold, dim, &Capability::probe())
}

// ── the print convenience ─────────────────────────────────────────────────

/// Render-to-terminal sugar for every block. Probes capabilities from the
/// real environment and emits.
pub trait Print: BlockRender {
    /// Render to stdout with probed capabilities.
    fn print(&self) -> std::io::Result<()> {
        let mut env = TerminalEnvironment::stdout();
        render(self, &mut env)
    }

    /// Render to stderr with probed capabilities.
    fn eprint(&self) -> std::io::Result<()> {
        let mut env = TerminalEnvironment::stderr();
        render(self, &mut env)
    }

    /// Render into an explicit environment (the test/compose path).
    fn render_to<E: RenderEnvironment>(&self, env: &mut E) -> std::io::Result<()> {
        render(self, env)
    }

    /// Render to a `String` at a fixed capability (used by tests + snapshots).
    fn to_string_at(&self, caps: Capability) -> String {
        let mut env = MockEnvironment::new(caps);
        // Infallible: the writer is an in-memory Vec.
        let _ = render(self, &mut env);
        env.rendered()
    }
}

impl<T: BlockRender + ?Sized> Print for T {}

/// The glob-import surface.
pub mod prelude {
    pub use crate::{
        badge, banner, callout, caps, debug, error, info, list, note, ok, panel, rule,
        rule_labelled, warn, Print,
    };
    pub use crate::blocks::{Badge, Banner, Callout, List, Panel, Rule};
    pub use crate::caps::{Capability, ColorLevel};
    pub use crate::ir::CalloutSeverity;
    pub use crate::theme::{Role, Theme};
}
