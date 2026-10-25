//! Live, in-place output: a region of lines redrawn on a stream, and the
//! small fragment builders that fill it.
//!
//! [`Live`] is capability-honest through the same wall as every block. On a
//! colored tty it repaints its region in place inside DEC synchronized
//! output (mode 2026), hides the cursor while live and always restores it.
//! Anywhere else (a pipe, `NO_COLOR`, a dumb terminal) it never moves the
//! cursor: frames are held, [`Live::finish`] prints the last one once, and
//! [`Live::commit`] appends plain lines, so piped bytes stay append-only and
//! deterministic.
//!
//! [`Spinner`], [`Meter`], [`Spark`] and [`Pulse`] return [`Fragment`]s that
//! name a [`Role`]. None of them reads a clock: time is an injected tick or
//! elapsed duration.

use std::fmt;
use std::io::{self, Stderr, Stdout, Write};
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthChar;

use crate::caps::Capability;
use crate::ir::{Fragment, StyledLine};
use crate::render::{emit_line, RenderEnvironment, TerminalEnvironment};
use crate::theme::Role;

/// The width a live region assumes when no terminal or `COLUMNS` answers.
pub const DEFAULT_WIDTH: u16 = 100;

/// The default minimum interval between two painted frames.
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_millis(80);

/// The cursor and screen controls a live region emits. The one place these
/// bytes are spelled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    /// Begin DEC synchronized output (mode 2026).
    SyncBegin,
    /// End DEC synchronized output (mode 2026).
    SyncEnd,
    /// Hide the cursor.
    HideCursor,
    /// Show the cursor.
    ShowCursor,
    /// Return to column 0.
    CarriageReturn,
    /// Move the cursor up `n` lines; `Up(0)` emits nothing.
    Up(usize),
    /// Clear the whole current line.
    ClearLine,
    /// Clear from the cursor to the end of the screen.
    ClearBelow,
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Control::SyncBegin => f.write_str("\x1b[?2026h"),
            Control::SyncEnd => f.write_str("\x1b[?2026l"),
            Control::HideCursor => f.write_str("\x1b[?25l"),
            Control::ShowCursor => f.write_str("\x1b[?25h"),
            Control::CarriageReturn => f.write_str("\r"),
            Control::Up(0) => Ok(()),
            Control::Up(n) => write!(f, "\x1b[{n}A"),
            Control::ClearLine => f.write_str("\x1b[2K"),
            Control::ClearBelow => f.write_str("\x1b[J"),
        }
    }
}

/// Truncate `line` to at most `width` display cells. A wide character that
/// would straddle the edge is dropped whole.
#[must_use]
pub fn fit(line: &StyledLine, width: usize) -> StyledLine {
    let mut out = Vec::with_capacity(line.len());
    let mut used = 0usize;
    for frag in line {
        let w = frag.width();
        if used + w <= width {
            out.push(frag.clone());
            used += w;
            continue;
        }
        let mut text = String::new();
        for ch in frag.text.chars() {
            let cw = ch.width().unwrap_or(0);
            if used + cw > width {
                break;
            }
            text.push(ch);
            used += cw;
        }
        if !text.is_empty() {
            out.push(Fragment { text, role: frag.role, bold: frag.bold, dim: frag.dim });
        }
        break;
    }
    out
}

/// A region of lines redrawn in place on a stream.
pub struct Live<E: RenderEnvironment> {
    env: E,
    width: Option<fn() -> u16>,
    min_interval: Duration,
    last_paint: Option<Instant>,
    frame: Vec<StyledLine>,
    dirty: bool,
    height: usize,
    cursor_hidden: bool,
    finished: bool,
}

impl Live<TerminalEnvironment<Stderr>> {
    /// A live region on stderr, capabilities and width probed from stderr.
    #[must_use]
    pub fn stderr() -> Self {
        let mut live = Self::new(TerminalEnvironment::stderr());
        live.width = Some(stderr_width);
        live
    }
}

impl Live<TerminalEnvironment<Stdout>> {
    /// A live region on stdout, capabilities and width probed from stdout.
    #[must_use]
    pub fn stdout() -> Self {
        let mut live = Self::new(TerminalEnvironment::stdout());
        live.width = Some(stdout_width);
        live
    }
}

impl<E: RenderEnvironment> Live<E> {
    /// A live region over an explicit environment; width is the capability's.
    pub fn new(env: E) -> Self {
        Self {
            env,
            width: None,
            min_interval: DEFAULT_MIN_INTERVAL,
            last_paint: None,
            frame: Vec::new(),
            dirty: false,
            height: 0,
            cursor_hidden: false,
            finished: false,
        }
    }

    /// Set the minimum interval between painted frames.
    #[must_use]
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    /// `true` when frames repaint in place (a colored tty).
    pub fn is_live(&self) -> bool {
        let caps = self.env.caps();
        caps.is_tty && caps.level.is_colored()
    }

    /// The environment the region writes into.
    pub fn env(&self) -> &E {
        &self.env
    }

    /// Show `lines` as the region, at the current instant.
    pub fn frame(&mut self, lines: &[StyledLine]) -> io::Result<bool> {
        self.frame_at(lines, Instant::now())
    }

    /// Show `lines` as the region at `now`. Returns `true` when painted; a
    /// call inside the minimum interval is held for the next paint.
    pub fn frame_at(&mut self, lines: &[StyledLine], now: Instant) -> io::Result<bool> {
        if self.finished {
            return Ok(false);
        }
        self.frame = lines.to_vec();
        self.dirty = true;
        if !self.is_live() {
            return Ok(false);
        }
        if self
            .last_paint
            .is_some_and(|at| now.saturating_duration_since(at) < self.min_interval)
        {
            return Ok(false);
        }
        self.last_paint = Some(now);
        self.paint(None)?;
        Ok(true)
    }

    /// Print a permanent line above the region, then repaint the region.
    pub fn commit(&mut self, line: &StyledLine) -> io::Result<()> {
        if self.is_live() && !self.finished {
            return self.paint(Some(line));
        }
        let caps = self.env.caps();
        let theme = self.env.theme();
        let w = self.env.writer();
        emit_line(line, &caps, theme, w)?;
        writeln!(w)?;
        w.flush()
    }

    /// Paint the last frame once, leave it as normal lines, and restore the
    /// cursor. Idempotent; also run on drop.
    pub fn finish(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        if !self.is_live() {
            let caps = self.env.caps();
            let theme = self.env.theme();
            let w = self.env.writer();
            for line in &self.frame {
                emit_line(line, &caps, theme, w)?;
                writeln!(w)?;
            }
            return w.flush();
        }
        let painted = if self.dirty { self.paint(None) } else { Ok(()) };
        self.height = 0;
        if self.cursor_hidden {
            self.cursor_hidden = false;
            let w = self.env.writer();
            write!(w, "{}", Control::ShowCursor)?;
            w.flush()?;
        }
        painted
    }

    fn width(&self) -> usize {
        usize::from(self.width.map_or(self.env.caps().width, |probe| probe()))
    }

    fn paint(&mut self, committed: Option<&StyledLine>) -> io::Result<()> {
        let caps = self.env.caps();
        let theme = self.env.theme();
        let width = self.width();
        let mut buf: Vec<u8> = Vec::new();
        write!(buf, "{}", Control::SyncBegin)?;
        if !self.cursor_hidden {
            write!(buf, "{}", Control::HideCursor)?;
            self.cursor_hidden = true;
        }
        write!(buf, "{}{}", Control::CarriageReturn, Control::Up(self.height))?;
        if let Some(line) = committed {
            write!(buf, "{}", Control::ClearLine)?;
            emit_line(line, &caps, theme, &mut buf)?;
            writeln!(buf)?;
        }
        for line in &self.frame {
            write!(buf, "{}", Control::ClearLine)?;
            emit_line(&fit(line, width), &caps, theme, &mut buf)?;
            writeln!(buf)?;
        }
        write!(buf, "{}{}", Control::ClearBelow, Control::SyncEnd)?;
        self.height = self.frame.len();
        self.dirty = false;
        let w = self.env.writer();
        w.write_all(&buf)?;
        w.flush()
    }
}

impl<E: RenderEnvironment> Drop for Live<E> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn columns() -> Option<u16> {
    std::env::var("COLUMNS").ok()?.trim().parse().ok().filter(|w| *w > 0)
}

fn width_of(size: Option<(terminal_size::Width, terminal_size::Height)>) -> u16 {
    size.map(|(w, _)| w.0)
        .filter(|w| *w > 0)
        .or_else(columns)
        .unwrap_or(DEFAULT_WIDTH)
}

fn stderr_width() -> u16 {
    width_of(terminal_size::terminal_size_of(io::stderr()))
}

fn stdout_width() -> u16 {
    width_of(terminal_size::terminal_size_of(io::stdout()))
}

/// The braille spinner frames.
pub const BRAILLE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const ASCII_SPIN: [&str; 4] = ["|", "/", "-", "\\"];

/// A spinner whose frame is chosen by an injected tick or elapsed time.
#[derive(Clone, Copy, Debug)]
pub struct Spinner {
    frames: &'static [&'static str],
    role: Role,
    period: Duration,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    /// The braille spinner in [`Role::Primary`], 80 ms per frame.
    #[must_use]
    pub const fn new() -> Self {
        Self { frames: &BRAILLE, role: Role::Primary, period: DEFAULT_MIN_INTERVAL }
    }

    /// The `| / - \` spinner for terminals without unicode.
    #[must_use]
    pub const fn ascii() -> Self {
        Self { frames: &ASCII_SPIN, role: Role::Primary, period: DEFAULT_MIN_INTERVAL }
    }

    /// Braille on a unicode terminal, ASCII otherwise.
    #[must_use]
    pub const fn for_caps(caps: &Capability) -> Self {
        if caps.unicode { Self::new() } else { Self::ascii() }
    }

    /// Paint the spinner in `role`.
    #[must_use]
    pub const fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// Set the time one frame is shown, for [`Spinner::at`].
    #[must_use]
    pub const fn period(mut self, period: Duration) -> Self {
        self.period = period;
        self
    }

    /// The glyph for `tick`.
    #[must_use]
    pub fn glyph(&self, tick: u64) -> &'static str {
        self.frames[(tick % self.frames.len() as u64) as usize]
    }

    /// The fragment for `tick`.
    #[must_use]
    pub fn fragment(&self, tick: u64) -> Fragment {
        Fragment::styled(self.glyph(tick), self.role)
    }

    /// The fragment for an elapsed duration.
    #[must_use]
    pub fn at(&self, elapsed: Duration) -> Fragment {
        let period = self.period.as_millis().max(1);
        self.fragment((elapsed.as_millis() / period) as u64)
    }
}

const EIGHTHS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// A fixed-width bar drawn in eighth blocks, one or many segments.
#[derive(Clone, Copy, Debug)]
pub struct Meter {
    width: usize,
    unicode: bool,
    track: &'static str,
    track_role: Role,
}

impl Meter {
    /// A unicode meter `width` cells wide with a `─` track in [`Role::Border`].
    #[must_use]
    pub const fn new(width: usize) -> Self {
        Self { width, unicode: true, track: "─", track_role: Role::Border }
    }

    /// A meter in the glyphs `caps` can show (`#` and `-` without unicode).
    #[must_use]
    pub const fn for_caps(width: usize, caps: &Capability) -> Self {
        if caps.unicode {
            Self::new(width)
        } else {
            Self { width, unicode: false, track: "-", track_role: Role::Border }
        }
    }

    /// Draw the unfilled part with `glyph` (one cell wide) in `role`.
    #[must_use]
    pub const fn track(mut self, glyph: &'static str, role: Role) -> Self {
        self.track = glyph;
        self.track_role = role;
        self
    }

    /// A single bar filled to `ratio` (clamped to `0..=1`) in `role`.
    #[must_use]
    pub fn fragments(&self, ratio: f64, role: Role) -> Vec<Fragment> {
        self.segments(&[(ratio, role)])
    }

    /// One bar of consecutive segments, each `(ratio, role)`. The ratios add
    /// up from the left and are clamped so the bar never overflows.
    #[must_use]
    pub fn segments(&self, segments: &[(f64, Role)]) -> Vec<Fragment> {
        let total = self.width * 8;
        let mut ends = Vec::with_capacity(segments.len());
        let mut sum = 0.0;
        for &(ratio, role) in segments {
            sum += clamp_unit(ratio);
            let end = (sum.min(1.0) * total as f64).round() as usize;
            ends.push((end.min(total), role));
        }
        let mut out: Vec<Fragment> = Vec::new();
        for cell in 0..self.width {
            let start = cell * 8;
            let (glyph, role) = match ends.iter().find(|(end, _)| *end > start) {
                None => (self.track, self.track_role),
                Some(&(end, role)) => self.cell(end - start, role),
            };
            push_merged(&mut out, glyph, role);
        }
        out
    }

    fn cell(&self, eighths: usize, role: Role) -> (&'static str, Role) {
        let eighths = eighths.min(8);
        if self.unicode {
            (EIGHTHS[eighths], role)
        } else if eighths >= 4 {
            ("#", role)
        } else {
            (self.track, self.track_role)
        }
    }
}

fn clamp_unit(v: f64) -> f64 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

fn push_merged(out: &mut Vec<Fragment>, glyph: &str, role: Role) {
    if let Some(last) = out.last_mut()
        && last.role == Some(role)
        && !last.bold
        && !last.dim
    {
        last.text.push_str(glyph);
        return;
    }
    out.push(Fragment::styled(glyph, role));
}

const SPARK_LEVELS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
const SPARK_ASCII: [&str; 8] = ["_", ".", "-", "~", "=", "+", "*", "#"];

/// A fixed-width sparkline of the most recent values, scaled to their max.
#[derive(Clone, Copy, Debug)]
pub struct Spark {
    width: usize,
    role: Role,
    unicode: bool,
}

impl Spark {
    /// A block sparkline `width` cells wide in [`Role::TextMuted`].
    #[must_use]
    pub const fn new(width: usize) -> Self {
        Self { width, role: Role::TextMuted, unicode: true }
    }

    /// A sparkline in the glyphs `caps` can show.
    #[must_use]
    pub const fn for_caps(width: usize, caps: &Capability) -> Self {
        Self { width, role: Role::TextMuted, unicode: caps.unicode }
    }

    /// Paint the sparkline in `role`.
    #[must_use]
    pub const fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// The last `width` values, left-padded with spaces when fewer.
    #[must_use]
    pub fn fragment(&self, values: &[f64]) -> Fragment {
        let levels = if self.unicode { &SPARK_LEVELS } else { &SPARK_ASCII };
        let tail = &values[values.len().saturating_sub(self.width)..];
        let max = tail.iter().copied().filter(|v| v.is_finite()).fold(0.0, f64::max);
        let mut text = String::with_capacity(self.width * 3);
        for _ in tail.len()..self.width {
            text.push(' ');
        }
        for &v in tail {
            let level = if max > 0.0 && v.is_finite() {
                ((v.max(0.0) / max) * 7.0).round() as usize
            } else {
                0
            };
            text.push_str(levels[level.min(7)]);
        }
        Fragment::styled(text, self.role)
    }
}

/// A role that changes with an injected tick: alternating, or bright for a
/// moment and then settled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pulse {
    from: Role,
    to: Role,
    period: u64,
    settles: bool,
}

impl Pulse {
    /// Alternate `a` and `b`, each held for `period` ticks.
    #[must_use]
    pub const fn alternate(a: Role, b: Role, period: u64) -> Self {
        Self { from: a, to: b, period, settles: false }
    }

    /// Show `bright` for `hold` ticks, then `settled` for good.
    #[must_use]
    pub const fn settle(bright: Role, settled: Role, hold: u64) -> Self {
        Self { from: bright, to: settled, period: hold, settles: true }
    }

    /// The role at `tick`.
    #[must_use]
    pub const fn role(&self, tick: u64) -> Role {
        let phase = tick / if self.period == 0 { 1 } else { self.period };
        let first = if self.settles { phase == 0 } else { phase.is_multiple_of(2) };
        if first { self.from } else { self.to }
    }

    /// `text` in the role at `tick`.
    #[must_use]
    pub fn fragment(&self, text: impl Into<String>, tick: u64) -> Fragment {
        Fragment::styled(text, self.role(tick))
    }
}
