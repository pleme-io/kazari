//! CLOSED-LOOP MASS-SYNTHESIS — the (block × ColorLevel) verification matrix.
//!
//! Proves the WALL-AS-TYPE mechanically: the SAME block value renders
//! correctly at every color depth through a `MockEnvironment` (no real
//! terminal). This is the forcing function — a new depth or a broken degrade
//! surfaces here, not in a user's pipe.

use kazari::prelude::*;
use kazari::{Capability, ColorLevel, Print};

fn at(level: ColorLevel, unicode: bool) -> Capability {
    Capability::fixed(level, 80, unicode)
}

fn level_name(level: ColorLevel) -> &'static str {
    match level {
        ColorLevel::Truecolor => "truecolor",
        ColorLevel::Ansi256 => "ansi256",
        ColorLevel::Ansi16 => "ansi16",
        ColorLevel::None => "none",
    }
}

#[test]
fn callout_truecolor_emits_nord_rgb() {
    let out = kazari::error("boom").to_string_at(at(ColorLevel::Truecolor, true));
    // nord11 (Aurora red) = #BF616A = 191;97;106 — proves the palette is
    // CONSUMED from irodori, not a fork.
    assert!(out.contains("38;2;191;97;106"), "truecolor error must be nord11 rgb: {out:?}");
    assert!(out.contains("boom"));
}

#[test]
fn callout_ansi256_emits_indexed_not_truecolor() {
    let out = kazari::error("boom").to_string_at(at(ColorLevel::Ansi256, true));
    assert!(out.contains("38;5;"), "256 must emit indexed sgr: {out:?}");
    assert!(!out.contains("38;2;"), "256 must NOT emit truecolor: {out:?}");
}

#[test]
fn callout_ansi16_emits_basic_not_extended() {
    let out = kazari::error("boom").to_string_at(at(ColorLevel::Ansi16, true));
    assert!(out.contains('\u{1b}'), "16 must emit sgr: {out:?}");
    assert!(
        !out.contains("38;2;") && !out.contains("38;5;"),
        "16 must be basic sgr, not extended: {out:?}"
    );
}

#[test]
fn callout_none_zero_escapes_tag_form() {
    let out = kazari::error("boom").to_string_at(at(ColorLevel::None, false));
    assert!(!out.contains('\u{1b}'), "None must emit ZERO escapes: {out:?}");
    assert!(out.contains("[ERROR] boom"), "None must be [TAG] form: {out:?}");
}

/// The matrix: every severity × every depth. None ⇒ zero escapes + a tag;
/// colored ⇒ at least one escape. A new severity or depth without the
/// invariant fails the build.
#[test]
fn callout_matrix_every_severity_every_depth() {
    let mut failures: Vec<String> = Vec::new();
    for sev in CalloutSeverity::ALL {
        for level in ColorLevel::ALL {
            let unicode = level.is_colored();
            let out = kazari::callout(sev, "msg").to_string_at(at(level, unicode));
            let has_esc = out.contains('\u{1b}');
            let mut fail = |why: &str| {
                let mut m = String::new();
                m.push_str(sev.tag());
                m.push('/');
                m.push_str(level_name(level));
                m.push_str(": ");
                m.push_str(why);
                failures.push(m);
            };
            match level {
                ColorLevel::None => {
                    if has_esc {
                        fail("leaked an escape byte on the None rung");
                    }
                    if !out.contains(sev.tag()) {
                        fail("dropped the [TAG] projection");
                    }
                }
                _ => {
                    if !has_esc {
                        fail("emitted no SGR on a colored rung");
                    }
                    if out.contains("38;2;") && level != ColorLevel::Truecolor {
                        fail("emitted truecolor below truecolor");
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} matrix rows failed:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}

#[test]
fn banner_is_three_lines_and_boxed() {
    let out = kazari::banner("sui").to_string_at(at(ColorLevel::Truecolor, true));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "banner is a 3-line box: {out:?}");
    assert!(out.contains('╭') && out.contains('╯'), "rounded box corners: {out:?}");
    assert!(out.contains("sui"));
}

#[test]
fn banner_ascii_fallback_when_no_unicode() {
    let out = kazari::banner("sui").to_string_at(at(ColorLevel::None, false));
    assert!(!out.contains('╭'), "ascii fallback drops rounded corners: {out:?}");
    assert!(out.contains('+') && out.contains('-'), "ascii box: {out:?}");
    assert!(!out.contains('\u{1b}'), "None ⇒ zero escapes: {out:?}");
}

#[test]
fn panel_aligns_keys() {
    let out = kazari::panel()
        .row("path", "/nix/store/abc")
        .row("closure-size", "42 MiB")
        .to_string_at(at(ColorLevel::None, false));
    // Both separators must line up — the short key is padded to the long key.
    let sep_cols: Vec<usize> = out.lines().filter_map(|l| l.find(" : ")).collect();
    assert_eq!(sep_cols.len(), 2, "two rows: {out:?}");
    assert_eq!(sep_cols[0], sep_cols[1], "separators must align: {out:?}");
}

#[test]
fn list_tasks_carry_severity() {
    let out = kazari::list()
        .task(CalloutSeverity::Ok, "compiled")
        .task(CalloutSeverity::Error, "linked")
        .bullet("plain note")
        .to_string_at(at(ColorLevel::None, false));
    assert!(out.contains("[OK] compiled"), "task ok tag: {out:?}");
    assert!(out.contains("[ERROR] linked"), "task error tag: {out:?}");
    assert!(!out.contains('\u{1b}'), "None ⇒ zero escapes: {out:?}");
}
