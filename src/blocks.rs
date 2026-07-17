//! The line-output block set.
//!
//! M0 ships `Callout` (the severity spine) + the kazari-*owned* structural
//! blocks (`Banner`, `Rule`, `Panel`, `List`, `Badge`) — no external engine,
//! so the whole set is self-contained and matrix-testable. The WRAP layer
//! (`Table`→comfy-table, `Progress`/`Spinner`→indicatif, `Tree`→termtree,
//! `Diagnostic`→miette) lands at M2.
//!
//! Every block lays out into `Vec<StyledLine>` from a [`Capability`] +
//! [`Theme`] alone. Restraint is the house style: at most one accent role
//! per block, Aurora is state-only, hierarchy is weight/dim not hue.

use crate::caps::Capability;
use crate::ir::{CalloutSeverity, Fragment, StyledLine};
use crate::render::BlockRender;
use crate::theme::{Role, Theme};

// ── glyph tables (unicode ⇄ ascii fallback) ───────────────────────────────

struct Glyphs {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    h: &'static str,
    v: &'static str,
    snow: &'static str,
    bullet: &'static str,
}

const UNICODE_GLYPHS: Glyphs = Glyphs {
    tl: "╭", tr: "╮", bl: "╰", br: "╯", h: "─", v: "│", snow: "❄", bullet: "•",
};
const ASCII_GLYPHS: Glyphs = Glyphs {
    tl: "+", tr: "+", bl: "+", br: "+", h: "-", v: "|", snow: "*", bullet: "-",
};

const fn glyphs(caps: &Capability) -> &'static Glyphs {
    if caps.unicode { &UNICODE_GLYPHS } else { &ASCII_GLYPHS }
}

fn repeat(s: &str, n: usize) -> String {
    s.repeat(n)
}

// ── Callout — the severity spine ──────────────────────────────────────────

/// A severity-tagged one-line admonition. The load-bearing block: its
/// [`CalloutSeverity`] is shared by list task markers, badges, status.
#[derive(Clone, Debug)]
pub struct Callout {
    pub severity: CalloutSeverity,
    pub message: String,
    pub title: Option<String>,
}

impl Callout {
    #[must_use]
    pub fn new(severity: CalloutSeverity, message: impl Into<String>) -> Self {
        Self { severity, message: message.into(), title: None }
    }

    /// Give the callout a bold lead-in title (`title: message`).
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl BlockRender for Callout {
    fn layout(&self, caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let role = self.severity.role();
        let mut line: StyledLine = Vec::new();
        if caps.unicode {
            line.push(Fragment::styled(self.severity.glyph(), role));
            line.push(Fragment::plain(" "));
        } else {
            // The NO_COLOR / non-unicode projection: [TAG] message.
            line.push(Fragment::plain("["));
            line.push(Fragment::styled(self.severity.tag(), role));
            line.push(Fragment::plain("] "));
        }
        if let Some(title) = &self.title {
            line.push(Fragment::accent(title.clone(), role));
            line.push(Fragment::plain(": "));
        }
        line.push(Fragment::styled(self.message.clone(), Role::Text));
        vec![line]
    }
}

// ── Banner — a framed title ───────────────────────────────────────────────

/// A boxed title header. Rounded box on a capable terminal, `+`-box on ASCII.
#[derive(Clone, Debug)]
pub struct Banner {
    pub title: String,
    pub snowflake: bool,
}

impl Banner {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), snowflake: true }
    }

    /// Drop the leading ❄ mark.
    #[must_use]
    pub fn bare(mut self) -> Self {
        self.snowflake = false;
        self
    }
}

impl BlockRender for Banner {
    fn layout(&self, caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let g = glyphs(caps);
        let mut content = String::new();
        if self.snowflake {
            content.push_str(g.snow);
            content.push(' ');
        }
        content.push_str(&self.title);
        let content_w = unicode_width::UnicodeWidthStr::width(content.as_str());
        let inner = content_w + 2;

        let top = {
            let mut s = String::with_capacity(inner + 2);
            s.push_str(g.tl);
            s.push_str(&repeat(g.h, inner));
            s.push_str(g.tr);
            s
        };
        let bottom = {
            let mut s = String::with_capacity(inner + 2);
            s.push_str(g.bl);
            s.push_str(&repeat(g.h, inner));
            s.push_str(g.br);
            s
        };

        vec![
            vec![Fragment::styled(top, Role::Border)],
            vec![
                Fragment::styled(g.v, Role::Border),
                Fragment::plain(" "),
                Fragment::accent(content, Role::Primary),
                Fragment::plain(" "),
                Fragment::styled(g.v, Role::Border),
            ],
            vec![Fragment::styled(bottom, Role::Border)],
        ]
    }
}

// ── Rule — a section divider ──────────────────────────────────────────────

/// A full-width horizontal divider, optionally labelled.
#[derive(Clone, Debug)]
pub struct Rule {
    pub label: Option<String>,
}

impl Rule {
    #[must_use]
    pub fn new() -> Self {
        Self { label: None }
    }

    #[must_use]
    pub fn labelled(label: impl Into<String>) -> Self {
        Self { label: Some(label.into()) }
    }
}

impl Default for Rule {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockRender for Rule {
    fn layout(&self, caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let g = glyphs(caps);
        let width = caps.width as usize;
        match &self.label {
            None => vec![vec![Fragment::styled(repeat(g.h, width), Role::Border)]],
            Some(label) => {
                let lead = 2usize;
                let label_w = unicode_width::UnicodeWidthStr::width(label.as_str());
                // lead ── + space + label + space + rest ──
                let used = lead + 1 + label_w + 1;
                let rest = width.saturating_sub(used);
                let mut leadstr = String::new();
                leadstr.push_str(&repeat(g.h, lead));
                leadstr.push(' ');
                vec![vec![
                    Fragment::styled(leadstr, Role::Border),
                    Fragment::styled(label.clone(), Role::TextMuted),
                    Fragment::plain(" "),
                    Fragment::styled(repeat(g.h, rest), Role::Border),
                ]]
            }
        }
    }
}

// ── Panel — aligned key → value ───────────────────────────────────────────

/// An aligned key→value panel.
#[derive(Clone, Debug, Default)]
pub struct Panel {
    pub title: Option<String>,
    pub rows: Vec<(String, String)>,
}

impl Panel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn titled(title: impl Into<String>) -> Self {
        Self { title: Some(title.into()), rows: Vec::new() }
    }

    #[must_use]
    pub fn row(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.rows.push((key.into(), value.into()));
        self
    }
}

impl BlockRender for Panel {
    fn layout(&self, _caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let key_w = self
            .rows
            .iter()
            .map(|(k, _)| unicode_width::UnicodeWidthStr::width(k.as_str()))
            .max()
            .unwrap_or(0);
        let mut lines: Vec<StyledLine> = Vec::new();
        if let Some(title) = &self.title {
            lines.push(vec![Fragment::accent(title.clone(), Role::Primary)]);
        }
        for (k, v) in &self.rows {
            let pad = key_w.saturating_sub(unicode_width::UnicodeWidthStr::width(k.as_str()));
            let mut key = String::new();
            key.push_str("  ");
            key.push_str(k);
            key.push_str(&repeat(" ", pad));
            lines.push(vec![
                Fragment::styled(key, Role::TextMuted),
                Fragment::styled(" : ", Role::TextDim),
                Fragment::styled(v.clone(), Role::Text),
            ]);
        }
        lines
    }
}

// ── List — bullets, tasks, checklists ─────────────────────────────────────

/// One list item — a bullet, or a task carrying a status severity.
#[derive(Clone, Debug)]
pub struct ListItem {
    pub text: String,
    pub status: Option<CalloutSeverity>,
}

/// A bullet / task list.
#[derive(Clone, Debug, Default)]
pub struct List {
    pub items: Vec<ListItem>,
}

impl List {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bullet(mut self, text: impl Into<String>) -> Self {
        self.items.push(ListItem { text: text.into(), status: None });
        self
    }

    #[must_use]
    pub fn task(mut self, status: CalloutSeverity, text: impl Into<String>) -> Self {
        self.items.push(ListItem { text: text.into(), status: Some(status) });
        self
    }
}

impl BlockRender for List {
    fn layout(&self, caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let g = glyphs(caps);
        let mut lines: Vec<StyledLine> = Vec::new();
        for item in &self.items {
            let (marker, role) = match item.status {
                Some(sev) if caps.unicode => (sev.glyph().to_string(), sev.role()),
                Some(sev) => {
                    let mut m = String::new();
                    m.push('[');
                    m.push_str(sev.tag());
                    m.push(']');
                    (m, sev.role())
                }
                None => (g.bullet.to_string(), Role::Primary),
            };
            lines.push(vec![
                Fragment::plain("  "),
                Fragment::styled(marker, role),
                Fragment::plain(" "),
                Fragment::styled(item.text.clone(), Role::Text),
            ]);
        }
        lines
    }
}

// ── Badge — an inline pill ────────────────────────────────────────────────

/// An inline `[ TEXT ]` pill, coloured by severity or a role.
#[derive(Clone, Debug)]
pub struct Badge {
    pub text: String,
    pub role: Role,
}

impl Badge {
    #[must_use]
    pub fn new(text: impl Into<String>, role: Role) -> Self {
        Self { text: text.into(), role }
    }

    #[must_use]
    pub fn severity(text: impl Into<String>, sev: CalloutSeverity) -> Self {
        Self { text: text.into(), role: sev.role() }
    }
}

impl BlockRender for Badge {
    fn layout(&self, _caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        vec![vec![
            Fragment::styled("[ ", Role::Border),
            Fragment::accent(self.text.clone(), self.role),
            Fragment::styled(" ]", Role::Border),
        ]]
    }
}
