//! Semantic roles + the theme that resolves them to color.
//!
//! A [`Fragment`](crate::ir::Fragment) names a [`Role`], never a color. The
//! theme is the one place a role becomes an [`Rgb`], and the palette is
//! **CONSUMED from `irodori::NORD`** — never a re-inlined hex (a 4th Nord
//! fork would be the exact regression the fleet is killing).
//!
//! The default is **dark-Nord blackmatter** — cool Polar Night, Frost
//! accents, Aurora signals — because that is the operator's stated opinion
//! and matches sui + mado. (The fleet's *other* prescribed theme is Vellum,
//! warm parchment; it and PolarVeil arrive at M1 as the morphable
//! alternatives, resolved through `ishou_tokens::FleetTheme` +
//! `SemanticRoles`. M0 owns a Nord role map sourced from irodori.)

use crate::caps::Rgb;

/// The kazari CLI-block role vocabulary. Restraint is typed: exactly one
/// *accent* role (`Primary`), Aurora is state-only (Error/Warn/Pending/Ok),
/// hierarchy is weight + dim, not hue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Role {
    /// The single accent — Frost ice-cyan (nord8). At most one per block.
    Primary,
    /// Body text — brightest Snow Storm (nord6).
    Text,
    /// Secondary text — dim Snow Storm (nord4).
    TextMuted,
    /// Tertiary / guide lines — Polar Night comment gray (nord3).
    TextDim,
    /// Frames, dividers, table rules — Polar Night gray (nord3).
    Border,
    /// State: error — Aurora red (nord11).
    Error,
    /// State: warning — Aurora orange (nord12).
    Warn,
    /// State: pending — Aurora yellow (nord13).
    Pending,
    /// State: ok / success — Aurora green (nord14).
    Ok,
    /// Informational — Frost light blue (nord9).
    Info,
    /// Identifiers / values — Aurora purple (nord15).
    Ident,
}

/// A resolved palette. M0 ships the opinionated dark-Nord default; M1 adds
/// Vellum / PolarVeil / a fully custom map via `ishou_tokens::FleetTheme`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    /// The default opinion: cool dark Nord, sourced from `irodori::NORD`.
    PlemeDark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::PlemeDark
    }
}

impl Theme {
    /// Resolve a role to a concrete color. The ONLY hex source is
    /// `irodori::NORD` — kazari re-inlines nothing.
    #[must_use]
    pub fn color(self, role: Role) -> Rgb {
        match self {
            Theme::PlemeDark => nord_role(role),
        }
    }
}

fn nord_role(role: Role) -> Rgb {
    let n = irodori::NORD;
    let c = match role {
        Role::Primary => n.frost[1],        // nord8  — ice cyan accent
        Role::Text => n.snow_storm[2],      // nord6  — brightest text
        Role::TextMuted => n.snow_storm[0], // nord4  — dim text
        Role::TextDim => n.polar_night[3],  // nord3  — comment gray
        Role::Border => n.polar_night[3],   // nord3  — frames/rules
        Role::Error => n.aurora[0],         // nord11 — red
        Role::Warn => n.aurora[1],          // nord12 — orange
        Role::Pending => n.aurora[2],       // nord13 — yellow
        Role::Ok => n.aurora[3],            // nord14 — green
        Role::Info => n.frost[2],           // nord9  — light blue
        Role::Ident => n.aurora[4],         // nord15 — purple
    };
    Rgb::from_color(c)
}
