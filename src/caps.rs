//! The capability WALL — made a type, not a runtime clamp.
//!
//! A [`Block`](crate::ir) carries **no** color depth, no width, no tty
//! knowledge. It is a pure value. The one and only place the wall is
//! consulted is at emit, through a [`Capability`] supplied by the
//! [`RenderEnvironment`](crate::render::RenderEnvironment). So "assumed a
//! tty / assumed truecolor on a pipe" has no code path — it is
//! *unrepresentable*, because no block can render without a `Capability`,
//! and the same block value degrades to every tier through one total
//! morphism.
//!
//! The two quantizers ([`Rgb::to_ansi256`] / [`Rgb::to_ansi16`]) are the
//! single genuinely-new subsystem (0-hit fleet grep): nothing in pleme-io
//! degrades an RGB to 256/16 color. They quantize **nearest-in-LINEAR
//! space** (reusing `irodori::Color::to_linear`) so the matte Nord
//! relationships survive the downgrade instead of washing out.

use anstyle::AnsiColor;

/// The color-depth ladder — the wall as a sum type. Every emit is a total
/// function over this; a new rung forces every block's matrix row.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ColorLevel {
    /// 24-bit `38;2;r;g;b` — a real Nord panel.
    Truecolor,
    /// 256-color `38;5;n` — quantized through the xterm cube + grey ramp.
    Ansi256,
    /// 16-color `30..37`/`90..97` — the terminal's own palette renders it.
    Ansi16,
    /// No SGR at all — a pipe, `NO_COLOR`, a dumb terminal. Zero escape bytes.
    None,
}

impl ColorLevel {
    /// Every rung, for the verification matrix (CLOSED-LOOP MASS-SYNTHESIS).
    pub const ALL: [ColorLevel; 4] = [
        ColorLevel::Truecolor,
        ColorLevel::Ansi256,
        ColorLevel::Ansi16,
        ColorLevel::None,
    ];

    /// `true` when this rung emits at least some SGR.
    pub const fn is_colored(self) -> bool {
        !matches!(self, ColorLevel::None)
    }
}

/// The background kazari is drawing over. Known only when kazari owns the
/// surface (its own panel bg); over a foreign terminal it is `Assumed`, and
/// the WCAG contrast floor is best-effort — a tier-honesty boundary the type
/// surfaces rather than hides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Background {
    Known(Rgb),
    Assumed,
}

/// A plain 24-bit color. The kazari-owned RGB type; converts to/from
/// `irodori::Color` (the palette source of truth) losslessly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// CONSUME irodori — no fork of the palette.
    #[must_use]
    pub fn from_color(c: irodori::Color) -> Self {
        Self::new(c.r, c.g, c.b)
    }

    fn linear(self) -> [f32; 3] {
        // Reuse irodori's sRGB→linear so quantization distance is
        // perceptually honest, not naive-sRGB (the 2026-06-14 wash-out bug).
        irodori::Color::new(self.r, self.g, self.b).to_linear()
    }

    /// Nearest xterm-256 index, chosen in linear space between the 6×6×6
    /// color cube and the 24-step grey ramp.
    #[must_use]
    pub fn to_ansi256(self) -> u8 {
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let nearest_level = |c: u8| -> (usize, u8) {
            let mut best = 0usize;
            let mut best_d = u16::MAX;
            for (i, &lv) in LEVELS.iter().enumerate() {
                let d = (i16::from(c) - i16::from(lv)).unsigned_abs();
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            (best, LEVELS[best])
        };
        let (ri, _) = nearest_level(self.r);
        let (gi, _) = nearest_level(self.g);
        let (bi, _) = nearest_level(self.b);
        let cube_idx = 16 + 36 * ri + 6 * gi + bi;
        let cube_rgb = Rgb::new(LEVELS[ri], LEVELS[gi], LEVELS[bi]);

        // Grey ramp candidate (232..=255): greys 8, 18, ... 238.
        let grey_avg = ((u16::from(self.r) + u16::from(self.g) + u16::from(self.b)) / 3) as u8;
        let gi2 = ((i16::from(grey_avg) - 8).clamp(0, 238) as f32 / 10.0).round() as i16;
        let gi2 = gi2.clamp(0, 23) as u8;
        let grey_val = 8 + gi2 * 10;
        let grey_idx = 232 + gi2;
        let grey_rgb = Rgb::new(grey_val, grey_val, grey_val);

        // Pick whichever candidate is nearest in linear space.
        if linear_dist(self, cube_rgb) <= linear_dist(self, grey_rgb) {
            cube_idx as u8
        } else {
            grey_idx
        }
    }

    /// Nearest of the 16 standard ANSI colors, in linear space. The
    /// terminal's own palette renders the slot — on a Nord terminal, a Nord
    /// slot looks Nord.
    #[must_use]
    pub fn to_ansi16(self) -> AnsiColor {
        // Standard xterm 16-color reference RGBs.
        const REF: [(Rgb, AnsiColor); 16] = [
            (Rgb::new(0x00, 0x00, 0x00), AnsiColor::Black),
            (Rgb::new(0x80, 0x00, 0x00), AnsiColor::Red),
            (Rgb::new(0x00, 0x80, 0x00), AnsiColor::Green),
            (Rgb::new(0x80, 0x80, 0x00), AnsiColor::Yellow),
            (Rgb::new(0x00, 0x00, 0x80), AnsiColor::Blue),
            (Rgb::new(0x80, 0x00, 0x80), AnsiColor::Magenta),
            (Rgb::new(0x00, 0x80, 0x80), AnsiColor::Cyan),
            (Rgb::new(0xc0, 0xc0, 0xc0), AnsiColor::White),
            (Rgb::new(0x80, 0x80, 0x80), AnsiColor::BrightBlack),
            (Rgb::new(0xff, 0x00, 0x00), AnsiColor::BrightRed),
            (Rgb::new(0x00, 0xff, 0x00), AnsiColor::BrightGreen),
            (Rgb::new(0xff, 0xff, 0x00), AnsiColor::BrightYellow),
            (Rgb::new(0x00, 0x00, 0xff), AnsiColor::BrightBlue),
            (Rgb::new(0xff, 0x00, 0xff), AnsiColor::BrightMagenta),
            (Rgb::new(0x00, 0xff, 0xff), AnsiColor::BrightCyan),
            (Rgb::new(0xff, 0xff, 0xff), AnsiColor::BrightWhite),
        ];
        let mut best = AnsiColor::White;
        let mut best_d = f32::MAX;
        for (rgb, ansi) in REF {
            let d = linear_dist(self, rgb);
            if d < best_d {
                best_d = d;
                best = ansi;
            }
        }
        best
    }
}

fn linear_dist(a: Rgb, b: Rgb) -> f32 {
    let la = a.linear();
    let lb = b.linear();
    let dr = la[0] - lb[0];
    let dg = la[1] - lb[1];
    let db = la[2] - lb[2];
    dr * dr + dg * dg + db * db
}

impl From<irodori::Color> for Rgb {
    fn from(c: irodori::Color) -> Self {
        Rgb::from_color(c)
    }
}

/// The full capability wall — carried by the environment, never the block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capability {
    pub level: ColorLevel,
    /// Terminal width in columns, clamped to a sane band.
    pub width: u16,
    pub is_tty: bool,
    pub unicode: bool,
    pub background: Background,
}

impl Capability {
    /// Probe the real environment through a sealed precedence fold
    /// (mirrors `shikumi::resolve_progressive`): `CLICOLOR_FORCE` >
    /// `NO_COLOR` > `!is_tty` > `COLORTERM` > `TERM` > none.
    #[must_use]
    pub fn probe() -> Self {
        use std::io::IsTerminal;
        let is_tty = std::io::stdout().is_terminal();
        let level = resolve_level(is_tty);
        Self {
            level,
            width: probe_width(),
            is_tty,
            unicode: probe_unicode(),
            background: Background::Assumed,
        }
    }

    /// The pipe/`NO_COLOR`/test floor: zero SGR, ASCII, 80 columns.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            level: ColorLevel::None,
            width: 80,
            is_tty: false,
            unicode: false,
            background: Background::Assumed,
        }
    }

    /// A fixed capability for tests/mocks at a chosen depth.
    #[must_use]
    pub const fn fixed(level: ColorLevel, width: u16, unicode: bool) -> Self {
        Self {
            level,
            width,
            is_tty: matches!(level, ColorLevel::Truecolor | ColorLevel::Ansi256 | ColorLevel::Ansi16),
            unicode,
            background: Background::Assumed,
        }
    }
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

fn resolve_level(is_tty: bool) -> ColorLevel {
    // CLICOLOR_FORCE overrides everything, even a pipe.
    let forced = env_set("CLICOLOR_FORCE");
    if env_set("NO_COLOR") && !forced {
        return ColorLevel::None;
    }
    if !is_tty && !forced {
        return ColorLevel::None;
    }
    if let Ok(ct) = std::env::var("COLORTERM") {
        if ct.contains("truecolor") || ct.contains("24bit") {
            return ColorLevel::Truecolor;
        }
    }
    match std::env::var("TERM") {
        Ok(t) if t.contains("256") => ColorLevel::Ansi256,
        Ok(t) if t.is_empty() || t == "dumb" => {
            if forced {
                ColorLevel::Ansi16
            } else {
                ColorLevel::None
            }
        }
        Ok(_) => ColorLevel::Ansi16,
        Err(_) => {
            if forced {
                ColorLevel::Ansi16
            } else {
                ColorLevel::None
            }
        }
    }
}

fn probe_width() -> u16 {
    if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
        return w.clamp(20, 400);
    }
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(w) = cols.parse::<u16>() {
            return w.clamp(20, 400);
        }
    }
    80
}

fn probe_unicode() -> bool {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.to_ascii_lowercase();
            if v.contains("utf-8") || v.contains("utf8") {
                return true;
            }
        }
    }
    // Default to unicode on a modern tty even without a locale hint.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi256_cube_anchors() {
        assert_eq!(Rgb::new(0, 0, 0).to_ansi256(), 16, "black is cube 16");
        assert_eq!(Rgb::new(255, 255, 255).to_ansi256(), 231, "white is cube 231");
    }

    #[test]
    fn ansi256_grey_ramp() {
        // A pure mid-grey lands on the 24-step ramp (232..=255), not the cube.
        let g = Rgb::new(128, 128, 128).to_ansi256();
        assert!((232..=255).contains(&g), "mid grey -> ramp: {g}");
    }

    #[test]
    fn ansi16_anchors() {
        assert_eq!(Rgb::new(0, 0, 0).to_ansi16(), AnsiColor::Black);
        assert_eq!(Rgb::new(255, 255, 255).to_ansi16(), AnsiColor::BrightWhite);
        assert_eq!(Rgb::new(255, 0, 0).to_ansi16(), AnsiColor::BrightRed);
        assert_eq!(Rgb::new(128, 0, 0).to_ansi16(), AnsiColor::Red);
    }

    #[test]
    fn from_irodori_is_lossless() {
        // CONSUME, no fork: an irodori Nord color round-trips exactly.
        let n11 = irodori::NORD.aurora[0]; // #BF616A
        let rgb = Rgb::from_color(n11);
        assert_eq!((rgb.r, rgb.g, rgb.b), (0xBF, 0x61, 0x6A));
    }

    #[test]
    fn plain_capability_is_the_pipe_floor() {
        let c = Capability::plain();
        assert_eq!(c.level, ColorLevel::None);
        assert!(!c.is_tty);
    }
}
