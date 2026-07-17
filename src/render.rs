//! The interpreter: lay a block out under a capability, emit it.
//!
//! [`RenderEnvironment`] is the mockable seam — the testability contract.
//! Tests inject a [`MockEnvironment`] with a fixed [`Capability`] and a byte
//! buffer, so the whole degrade ladder is exercised at every
//! [`ColorLevel`](crate::caps::ColorLevel) **without a real terminal**. That
//! is what makes the (block × depth) matrix a mechanical forcing function.

use std::io::{self, Stderr, Stdout, Write};

use crate::caps::Capability;
use crate::ir::StyledLine;
use crate::style::StyleAtom;
use crate::theme::Theme;

/// A block is a pure value that lays itself out given only the capability +
/// theme. No block can render without a `Capability` — the wall as a type.
pub trait BlockRender {
    fn layout(&self, caps: &Capability, theme: Theme) -> Vec<StyledLine>;
}

/// The seam: where the caps come from and where the bytes go.
pub trait RenderEnvironment {
    fn caps(&self) -> Capability;
    fn theme(&self) -> Theme;
    fn writer(&mut self) -> &mut dyn Write;
}

/// Lay `block` out under `env`'s capability and emit it, one line per row.
pub fn render<B, E>(block: &B, env: &mut E) -> io::Result<()>
where
    B: BlockRender + ?Sized,
    E: RenderEnvironment + ?Sized,
{
    let caps = env.caps();
    let theme = env.theme();
    let lines = block.layout(&caps, theme);
    let w = env.writer();
    for line in &lines {
        emit_line(line, &caps, theme, w)?;
        writeln!(w)?;
    }
    Ok(())
}

fn emit_line(line: &StyledLine, caps: &Capability, theme: Theme, w: &mut dyn Write) -> io::Result<()> {
    for frag in line {
        match frag.role {
            Some(role) => {
                StyleAtom::resolve(role, theme, caps, frag.bold, frag.dim).write_into(w, &frag.text)?;
            }
            None => w.write_all(frag.text.as_bytes())?,
        }
    }
    Ok(())
}

/// A real terminal environment over an arbitrary writer.
pub struct TerminalEnvironment<W: Write> {
    caps: Capability,
    theme: Theme,
    out: W,
}

impl TerminalEnvironment<Stdout> {
    /// stdout, capabilities probed from the real environment.
    #[must_use]
    pub fn stdout() -> Self {
        Self { caps: Capability::probe(), theme: Theme::default(), out: io::stdout() }
    }
}

impl TerminalEnvironment<Stderr> {
    /// stderr, capabilities probed from the real environment.
    #[must_use]
    pub fn stderr() -> Self {
        Self { caps: Capability::probe(), theme: Theme::default(), out: io::stderr() }
    }
}

impl<W: Write> TerminalEnvironment<W> {
    /// An environment with an explicit capability + writer.
    pub fn new(caps: Capability, theme: Theme, out: W) -> Self {
        Self { caps, theme, out }
    }

    /// Override the theme (morphability — default is dark-Nord).
    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
}

impl<W: Write> RenderEnvironment for TerminalEnvironment<W> {
    fn caps(&self) -> Capability {
        self.caps
    }
    fn theme(&self) -> Theme {
        self.theme
    }
    fn writer(&mut self) -> &mut dyn Write {
        &mut self.out
    }
}

/// A test/mock environment: a fixed capability + an in-memory buffer. No
/// real terminal touched — this is the seam the verification matrix drives.
pub struct MockEnvironment {
    caps: Capability,
    theme: Theme,
    buf: Vec<u8>,
}

impl MockEnvironment {
    /// A mock at a fixed capability (dark-Nord theme).
    #[must_use]
    pub fn new(caps: Capability) -> Self {
        Self { caps, theme: Theme::default(), buf: Vec::new() }
    }

    /// The bytes emitted so far, as a string.
    #[must_use]
    pub fn rendered(&self) -> String {
        String::from_utf8_lossy(&self.buf).into_owned()
    }

    /// `true` if the output contains no ESC byte (the NO_COLOR proof).
    #[must_use]
    pub fn has_no_escapes(&self) -> bool {
        !self.buf.contains(&0x1b)
    }
}

impl RenderEnvironment for MockEnvironment {
    fn caps(&self) -> Capability {
        self.caps
    }
    fn theme(&self) -> Theme {
        self.theme
    }
    fn writer(&mut self) -> &mut dyn Write {
        &mut self.buf
    }
}
