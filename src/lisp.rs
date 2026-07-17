//! The `(defblock…)` authoring vocabulary — a tatara-lisp surface for kazari.
//!
//! An operator writes a block as an S-expression and kazari renders it:
//!
//! ```
//! use kazari::prelude::*;
//! let block = kazari::lisp::block_from_lisp(
//!     r#"(defcallout :severity error :message "build failed")"#,
//! ).unwrap();
//! let s = block.to_string_at(Capability::fixed(ColorLevel::None, 80, false));
//! assert!(s.contains("[ERROR] build failed"));
//! ```
//!
//! **Tier-honest:** this is kazari's *own* minimal S-expr reader (the aldrava
//! precedent), not the shared `tatara_lisp` crate's `#[derive(TataraDomain)]`
//! registration machinery. Swapping to the runtime-consumable `tatara_lisp`
//! reader once it ships is a named, isolated follow-on
//! (`pending-kazari: tatara-lisp-reader`). The forms + keyword grammar match
//! the destination so the swap is drop-in.

use crate::blocks::{Badge, Banner, Callout, List, Panel, Rule, Table};
use crate::ir::CalloutSeverity;
use crate::render::BlockRender;
use crate::theme::Role;

/// A parsed S-expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    Sym(String),
    /// A `:keyword`.
    Kw(String),
    Str(String),
    Num(f64),
    List(Vec<Sexp>),
}

/// A typed authoring error — never a panic, never a `format!()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LispError {
    Unterminated(&'static str),
    Unexpected(char),
    Empty,
    ExpectedList,
    UnknownForm(String),
    BadArg(&'static str),
}

impl std::fmt::Display for LispError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LispError::Unterminated(w) => {
                f.write_str("unterminated ")?;
                f.write_str(w)
            }
            LispError::Unexpected(c) => {
                f.write_str("unexpected char ")?;
                f.write_str(&c.to_string())
            }
            LispError::Empty => f.write_str("empty input"),
            LispError::ExpectedList => f.write_str("expected a (form …) list"),
            LispError::UnknownForm(s) => {
                f.write_str("unknown form: ")?;
                f.write_str(s)
            }
            LispError::BadArg(s) => {
                f.write_str("bad argument: ")?;
                f.write_str(s)
            }
        }
    }
}

impl std::error::Error for LispError {}

// ── reader ────────────────────────────────────────────────────────────────

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Reader<'a> {
    fn new(src: &'a str) -> Self {
        Self { chars: src.chars().peekable() }
    }

    fn skip_ws(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else if c == ';' {
                // line comment
                while let Some(&c) = self.chars.peek() {
                    self.chars.next();
                    if c == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn read(&mut self) -> Result<Sexp, LispError> {
        self.skip_ws();
        match self.chars.peek().copied() {
            None => Err(LispError::Empty),
            Some('(') => self.read_list(),
            Some('"') => self.read_string(),
            Some(')') => Err(LispError::Unexpected(')')),
            Some(_) => self.read_atom(),
        }
    }

    fn read_list(&mut self) -> Result<Sexp, LispError> {
        self.chars.next(); // consume '('
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.chars.peek().copied() {
                None => return Err(LispError::Unterminated("list")),
                Some(')') => {
                    self.chars.next();
                    return Ok(Sexp::List(items));
                }
                Some(_) => items.push(self.read()?),
            }
        }
    }

    fn read_string(&mut self) -> Result<Sexp, LispError> {
        self.chars.next(); // consume '"'
        let mut s = String::new();
        loop {
            match self.chars.next() {
                None => return Err(LispError::Unterminated("string")),
                Some('"') => return Ok(Sexp::Str(s)),
                Some('\\') => match self.chars.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(c) => s.push(c),
                    None => return Err(LispError::Unterminated("string")),
                },
                Some(c) => s.push(c),
            }
        }
    }

    fn read_atom(&mut self) -> Result<Sexp, LispError> {
        let mut tok = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                break;
            }
            tok.push(c);
            self.chars.next();
        }
        if let Some(kw) = tok.strip_prefix(':') {
            Ok(Sexp::Kw(kw.to_string()))
        } else if let Ok(n) = tok.parse::<f64>() {
            Ok(Sexp::Num(n))
        } else {
            Ok(Sexp::Sym(tok))
        }
    }
}

/// Parse a single S-expression from `src`.
pub fn parse(src: &str) -> Result<Sexp, LispError> {
    Reader::new(src).read()
}

// ── accessors ───────────────────────────────────────────────────────────────

impl Sexp {
    fn as_list(&self) -> Result<&[Sexp], LispError> {
        match self {
            Sexp::List(v) => Ok(v),
            _ => Err(LispError::ExpectedList),
        }
    }

    fn as_text(&self) -> Result<String, LispError> {
        match self {
            Sexp::Str(s) | Sexp::Sym(s) => Ok(s.clone()),
            _ => Err(LispError::BadArg("expected a string or symbol")),
        }
    }

    fn as_sym(&self) -> Result<&str, LispError> {
        match self {
            Sexp::Sym(s) => Ok(s.as_str()),
            _ => Err(LispError::BadArg("expected a symbol")),
        }
    }

    fn as_num(&self) -> Result<f64, LispError> {
        match self {
            Sexp::Num(n) => Ok(*n),
            _ => Err(LispError::BadArg("expected a number")),
        }
    }
}

/// Collect `:key value` pairs from a form's tail.
fn kwargs(tail: &[Sexp]) -> Vec<(String, &Sexp)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < tail.len() + 1 {
        if let Some(Sexp::Kw(k)) = tail.get(i) {
            if let Some(v) = tail.get(i + 1) {
                out.push((k.clone(), v));
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn get<'a>(args: &'a [(String, &'a Sexp)], key: &str) -> Option<&'a Sexp> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

fn severity_from(sym: &str) -> Result<CalloutSeverity, LispError> {
    Ok(match sym {
        "note" => CalloutSeverity::Note,
        "info" => CalloutSeverity::Info,
        "ok" => CalloutSeverity::Ok,
        "warn" => CalloutSeverity::Warn,
        "error" => CalloutSeverity::Error,
        "debug" => CalloutSeverity::Debug,
        _ => return Err(LispError::BadArg("unknown severity")),
    })
}

fn role_from(sym: &str) -> Result<Role, LispError> {
    Ok(match sym {
        "primary" => Role::Primary,
        "text" => Role::Text,
        "muted" | "text-muted" => Role::TextMuted,
        "dim" | "text-dim" => Role::TextDim,
        "border" => Role::Border,
        "error" => Role::Error,
        "warn" => Role::Warn,
        "pending" => Role::Pending,
        "ok" => Role::Ok,
        "info" => Role::Info,
        "ident" => Role::Ident,
        _ => return Err(LispError::BadArg("unknown role")),
    })
}

/// Parse a `(def<block> …)` form into a renderable block.
pub fn block_from_lisp(src: &str) -> Result<Box<dyn BlockRender>, LispError> {
    let sexp = parse(src)?;
    let list = sexp.as_list()?;
    let head = list.first().ok_or(LispError::Empty)?.as_sym()?;
    let args = kwargs(&list[1..]);

    match head {
        "defcallout" => {
            let sev = severity_from(get(&args, "severity").ok_or(LispError::BadArg("callout needs :severity"))?.as_sym()?)?;
            let msg = get(&args, "message").ok_or(LispError::BadArg("callout needs :message"))?.as_text()?;
            let mut c = Callout::new(sev, msg);
            if let Some(t) = get(&args, "title") {
                c = c.with_title(t.as_text()?);
            }
            Ok(Box::new(c))
        }
        "defbanner" => {
            let title = get(&args, "title").ok_or(LispError::BadArg("banner needs :title"))?.as_text()?;
            let mut b = Banner::new(title);
            if let Some(Sexp::Sym(s)) = get(&args, "snowflake") {
                if s == "false" || s == "no" {
                    b = b.bare();
                }
            }
            Ok(Box::new(b))
        }
        "defrule" => match get(&args, "label") {
            Some(l) => Ok(Box::new(Rule::labelled(l.as_text()?))),
            None => Ok(Box::new(Rule::new())),
        },
        "defbadge" => {
            let text = get(&args, "text").ok_or(LispError::BadArg("badge needs :text"))?.as_text()?;
            let role = match get(&args, "severity") {
                Some(s) => severity_from(s.as_sym()?)?.role(),
                None => match get(&args, "role") {
                    Some(r) => role_from(r.as_sym()?)?,
                    None => Role::Primary,
                },
            };
            Ok(Box::new(Badge::new(text, role)))
        }
        "defpanel" => {
            let mut p = Panel::new();
            if let Some(t) = get(&args, "title") {
                p = Panel::titled(t.as_text()?);
            }
            if let Some(rows) = get(&args, "rows") {
                for pair in rows.as_list()? {
                    let kv = pair.as_list()?;
                    let k = kv.first().ok_or(LispError::BadArg("panel row needs a key"))?.as_text()?;
                    let v = kv.get(1).ok_or(LispError::BadArg("panel row needs a value"))?.as_text()?;
                    p = p.row(k, v);
                }
            }
            Ok(Box::new(p))
        }
        "deflist" => {
            let mut l = List::new();
            if let Some(items) = get(&args, "items") {
                for item in items.as_list()? {
                    match item {
                        // (task <severity> "text")
                        Sexp::List(parts) if parts.first().map(|s| s.as_sym()) == Some(Ok("task")) => {
                            let sev = severity_from(parts.get(1).ok_or(LispError::BadArg("task needs a severity"))?.as_sym()?)?;
                            let text = parts.get(2).ok_or(LispError::BadArg("task needs text"))?.as_text()?;
                            l = l.task(sev, text);
                        }
                        other => l = l.bullet(other.as_text()?),
                    }
                }
            }
            Ok(Box::new(l))
        }
        "deftable" => {
            let headers = get(&args, "headers").ok_or(LispError::BadArg("table needs :headers"))?.as_list()?;
            let mut t = Table::new(headers.iter().map(|h| h.as_text().unwrap_or_default()).collect::<Vec<_>>());
            if let Some(rows) = get(&args, "rows") {
                for row in rows.as_list()? {
                    let cells = row.as_list()?.iter().map(|c| c.as_text().unwrap_or_default()).collect::<Vec<_>>();
                    t = t.row(cells);
                }
            }
            Ok(Box::new(t))
        }
        other => Err(LispError::UnknownForm(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{Capability, ColorLevel};
    use crate::Print;

    fn plain(src: &str) -> String {
        block_from_lisp(src)
            .unwrap()
            .to_string_at(Capability::fixed(ColorLevel::None, 80, false))
    }

    #[test]
    fn parses_callout() {
        assert!(plain(r#"(defcallout :severity error :message "boom")"#).contains("[ERROR] boom"));
        assert!(plain(r#"(defcallout :severity ok :message "done")"#).contains("[OK] done"));
    }

    #[test]
    fn parses_callout_with_title() {
        let s = plain(r#"(defcallout :severity warn :title "cache" :message "miss")"#);
        assert!(s.contains("cache") && s.contains("miss"));
    }

    #[test]
    fn parses_banner_and_bare() {
        assert!(plain(r#"(defbanner :title "sui")"#).contains("sui"));
        let bare = plain(r#"(defbanner :title "sui" :snowflake false)"#);
        assert!(bare.contains("sui") && !bare.contains('*'));
    }

    #[test]
    fn parses_panel_rows() {
        let s = plain(r#"(defpanel :rows (("path" "/nix/store") ("size" "42 MiB")))"#);
        assert!(s.contains("path") && s.contains("/nix/store") && s.contains(" : "));
    }

    #[test]
    fn parses_table() {
        let s = plain(r#"(deftable :headers ("Domain" "Gate") :rows (("derivation" "Working")))"#);
        assert!(s.contains("Domain") && s.contains("derivation") && s.contains("Working"));
    }

    #[test]
    fn parses_list_with_tasks() {
        let s = plain(r#"(deflist :items ((task ok "fetch") (task error "build") "note"))"#);
        assert!(s.contains("[OK] fetch") && s.contains("[ERROR] build") && s.contains("note"));
    }

    #[test]
    fn typed_errors_never_panic() {
        assert!(matches!(block_from_lisp("(defbogus)"), Err(LispError::UnknownForm(_))));
        assert!(matches!(block_from_lisp("(defcallout :severity error)"), Err(LispError::BadArg(_))));
        assert!(matches!(block_from_lisp("(defcallout"), Err(LispError::Unterminated(_))));
        assert!(matches!(block_from_lisp(""), Err(LispError::Empty)));
    }

    #[test]
    fn comments_and_whitespace_ok() {
        let s = plain("; a comment\n  (defcallout :severity info  :message \"spaced\")  ");
        assert!(s.contains("[INFO] spaced"));
    }
}
