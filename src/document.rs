
use crate::caps::Capability;
use crate::ir::{CalloutSeverity, Fragment, StyledLine};
use crate::render::BlockRender;
use crate::theme::{Role, Theme};

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Map(Vec<(String, Node)>),
    Seq(Vec<Node>),
    Str(String),
    Num(String),
    Bool(bool),
    Null,
    Status(String, CalloutSeverity),
}

impl Node {
    #[must_use]
    pub fn map<I, K>(entries: I) -> Self
    where
        I: IntoIterator<Item = (K, Node)>,
        K: Into<String>,
    {
        Node::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    #[must_use]
    pub fn seq<I: IntoIterator<Item = Node>>(items: I) -> Self {
        Node::Seq(items.into_iter().collect())
    }

    #[must_use]
    pub fn str(s: impl Into<String>) -> Self {
        Node::Str(s.into())
    }

    #[must_use]
    pub fn num(n: impl ToString) -> Self {
        Node::Num(n.to_string())
    }

    #[must_use]
    pub fn status(s: impl Into<String>, severity: CalloutSeverity) -> Self {
        Node::Status(s.into(), severity)
    }

    const fn is_scalar(&self) -> bool {
        !matches!(self, Node::Map(_) | Node::Seq(_))
    }

    #[must_use]
    pub fn classify<F>(self, classify: &F) -> Self
    where
        F: Fn(&[&str], &str) -> Option<CalloutSeverity>,
    {
        let mut path: Vec<String> = Vec::new();
        self.classify_at(&mut path, classify)
    }

    fn classify_at<F>(self, path: &mut Vec<String>, classify: &F) -> Self
    where
        F: Fn(&[&str], &str) -> Option<CalloutSeverity>,
    {
        match self {
            Node::Map(entries) => Node::Map(
                entries
                    .into_iter()
                    .map(|(k, v)| {
                        path.push(k);
                        let v = v.classify_at(path, classify);
                        let k = path.pop().unwrap_or_default();
                        (k, v)
                    })
                    .collect(),
            ),
            Node::Seq(items) => {
                Node::Seq(items.into_iter().map(|v| v.classify_at(path, classify)).collect())
            }
            scalar => {
                let text = match &scalar {
                    Node::Str(s) | Node::Num(s) => s.clone(),
                    Node::Bool(b) => b.to_string(),
                    _ => return scalar,
                };
                let keys: Vec<&str> = path.iter().map(String::as_str).collect();
                match classify(&keys, &text) {
                    Some(severity) => Node::Status(text, severity),
                    None => scalar,
                }
            }
        }
    }
}

#[cfg(feature = "json")]
impl From<&serde_json::Value> for Node {
    fn from(v: &serde_json::Value) -> Self {
        use serde_json::Value;
        match v {
            Value::Null => Node::Null,
            Value::Bool(b) => Node::Bool(*b),
            Value::Number(n) => Node::Num(n.to_string()),
            Value::String(s) => Node::Str(s.clone()),
            Value::Array(items) => Node::Seq(items.iter().map(Node::from).collect()),
            Value::Object(map) => {
                Node::Map(map.iter().map(|(k, v)| (k.clone(), Node::from(v))).collect())
            }
        }
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Value> for Node {
    fn from(v: serde_json::Value) -> Self {
        Node::from(&v)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub root: Node,
}

impl Document {
    #[must_use]
    pub fn new(root: Node) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn classify<F>(self, classify: F) -> Self
    where
        F: Fn(&[&str], &str) -> Option<CalloutSeverity>,
    {
        Self { root: self.root.classify(&classify) }
    }
}

impl From<Node> for Document {
    fn from(root: Node) -> Self {
        Self::new(root)
    }
}

impl BlockRender for Document {
    fn layout(&self, _caps: &Capability, _theme: Theme) -> Vec<StyledLine> {
        let mut out = Vec::new();
        match &self.root {
            Node::Map(entries) if entries.is_empty() => out.push(vec![dim("{}")]),
            Node::Seq(items) if items.is_empty() => out.push(vec![dim("[]")]),
            Node::Map(entries) => layout_map(entries, 0, None, &mut out),
            Node::Seq(items) => layout_seq(items, 0, None, &mut out),
            scalar => out.push(scalar_fragments(scalar)),
        }
        out
    }
}

fn dim(text: &str) -> Fragment {
    Fragment::styled(text, Role::TextDim)
}

fn indent(n: usize) -> StyledLine {
    if n == 0 { Vec::new() } else { vec![Fragment::plain(" ".repeat(n))] }
}

fn layout_map(entries: &[(String, Node)], depth: usize, mut first: Option<StyledLine>, out: &mut Vec<StyledLine>) {
    for (key, value) in entries {
        let mut line = first.take().unwrap_or_else(|| indent(depth));
        let key = yaml_text(key);
        line.push(if depth == 0 { Fragment::accent(key, Role::Primary) } else { Fragment::styled(key, Role::Primary) });
        line.push(dim(":"));
        match value {
            Node::Map(m) if m.is_empty() => {
                line.push(Fragment::plain(" "));
                line.push(dim("{}"));
                out.push(line);
            }
            Node::Seq(s) if s.is_empty() => {
                line.push(Fragment::plain(" "));
                line.push(dim("[]"));
                out.push(line);
            }
            Node::Map(m) => {
                out.push(line);
                layout_map(m, depth + 2, None, out);
            }
            Node::Seq(s) => {
                out.push(line);
                layout_seq(s, depth + 2, None, out);
            }
            scalar => {
                line.push(Fragment::plain(" "));
                line.extend(scalar_fragments(scalar));
                out.push(line);
            }
        }
    }
}

fn layout_seq(items: &[Node], depth: usize, mut first: Option<StyledLine>, out: &mut Vec<StyledLine>) {
    for item in items {
        let mut line = first.take().unwrap_or_else(|| indent(depth));
        line.push(Fragment::styled("- ", Role::Border));
        match item {
            Node::Map(m) if m.is_empty() => {
                line.push(dim("{}"));
                out.push(line);
            }
            Node::Seq(s) if s.is_empty() => {
                line.push(dim("[]"));
                out.push(line);
            }
            Node::Map(m) => layout_map(m, depth + 2, Some(line), out),
            Node::Seq(s) => layout_seq(s, depth + 2, Some(line), out),
            scalar if scalar.is_scalar() => {
                line.extend(scalar_fragments(scalar));
                out.push(line);
            }
            _ => out.push(line),
        }
    }
}

fn scalar_fragments(node: &Node) -> StyledLine {
    match node {
        Node::Str(s) => vec![Fragment::styled(yaml_text(s), Role::Text)],
        Node::Num(n) => vec![Fragment::styled(n.clone(), Role::Ident)],
        Node::Bool(b) => vec![Fragment::styled(b.to_string(), Role::Ident)],
        Node::Null => vec![dim("null")],
        Node::Status(s, severity) => vec![Fragment::styled(yaml_text(s), severity.role())],
        Node::Map(_) | Node::Seq(_) => Vec::new(),
    }
}

fn yaml_text(s: &str) -> String {
    if needs_quotes(s) { quote(s) } else { s.to_string() }
}

fn needs_quotes(s: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "null", "Null", "NULL", "~", "true", "True", "TRUE", "false", "False", "FALSE", "yes", "Yes",
        "YES", "no", "No", "NO", "on", "On", "ON", "off", "Off", "OFF",
    ];
    const LEADING: &[char] = &[
        '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`',
    ];
    let Some(first) = s.chars().next() else { return true };
    RESERVED.contains(&s)
        || LEADING.contains(&first)
        || first.is_whitespace()
        || s.ends_with(char::is_whitespace)
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.chars().any(char::is_control)
        || looks_numeric(s)
}

fn looks_numeric(s: &str) -> bool {
    let t = s.strip_prefix(['+', '-']).unwrap_or(s);
    t.parse::<f64>().is_ok()
        || t.starts_with("0x")
        || t.starts_with("0o")
        || matches!(t, ".inf" | ".Inf" | ".INF" | ".nan" | ".NaN" | ".NAN")
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str("\\u");
                let code = c as u32;
                for shift in [12, 8, 4, 0] {
                    let nibble = (code >> shift) & 0xF;
                    out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::ColorLevel;
    use crate::Print;

    fn plain(doc: &Document) -> String {
        doc.to_string_at(Capability::fixed(ColorLevel::None, 80, false))
    }

    fn store_show() -> Document {
        Document::new(Node::map([
            (
                "facts",
                Node::map([
                    ("lock", Node::str("held_by_this_daemon")),
                    (
                        "state",
                        Node::map([
                            ("kind", Node::str("resume")),
                            ("leader", Node::Bool(true)),
                            ("revision", Node::num(1_385_997)),
                            ("store", Node::str("live")),
                        ]),
                    ),
                ]),
            ),
            ("path", Node::str("/Users/x/.local/share/engenho/store")),
        ]))
    }

    #[test]
    fn renders_block_yaml() {
        assert_eq!(
            plain(&store_show()),
            "facts:\n  lock: held_by_this_daemon\n  state:\n    kind: resume\n    leader: true\n    revision: 1385997\n    store: live\npath: /Users/x/.local/share/engenho/store\n"
        );
    }

    #[test]
    fn sequences_of_maps_hang_off_the_dash() {
        let doc = Document::new(Node::map([(
            "items",
            Node::seq([
                Node::map([("name", Node::str("a")), ("ready", Node::Bool(false))]),
                Node::str("b"),
                Node::seq([]),
            ]),
        )]));
        assert_eq!(plain(&doc), "items:\n  - name: a\n    ready: false\n  - b\n  - []\n");
    }

    #[test]
    fn ambiguous_strings_are_quoted() {
        let doc = Document::new(Node::map([
            ("a", Node::str("true")),
            ("b", Node::str("42")),
            ("c", Node::str("")),
            ("d", Node::str("k: v")),
            ("e", Node::str("line\nbreak")),
            ("f", Node::str("- dash")),
            ("g", Node::str("plain words")),
        ]));
        assert_eq!(
            plain(&doc),
            "a: \"true\"\nb: \"42\"\nc: \"\"\nd: \"k: v\"\ne: \"line\\nbreak\"\nf: \"- dash\"\ng: plain words\n"
        );
    }

    #[test]
    fn classify_paints_status_and_keeps_text() {
        let doc = store_show().classify(|path, text| match (path.last().copied(), text) {
            (Some("store"), "live") => Some(CalloutSeverity::Ok),
            _ => None,
        });
        assert_eq!(plain(&doc), plain(&store_show()));
        let colored = doc.to_string_at(Capability::fixed(ColorLevel::Truecolor, 80, true));
        assert!(colored.contains("38;2;163;190;140mlive"), "store: live is nord14: {colored:?}");
    }

    #[test]
    fn colour_is_an_overlay_on_identical_text() {
        let colored = store_show().to_string_at(Capability::fixed(ColorLevel::Truecolor, 80, true));
        assert!(colored.contains('\u{1b}'));
        let mut stripped = String::new();
        let mut chars = colored.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                stripped.push(c);
            }
        }
        assert_eq!(stripped, plain(&store_show()));
    }

    #[test]
    fn empty_and_scalar_roots() {
        assert_eq!(plain(&Document::new(Node::map::<_, String>([]))), "{}\n");
        assert_eq!(plain(&Document::new(Node::Null)), "null\n");
    }
}
