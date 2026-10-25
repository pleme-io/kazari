use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

use kazari::live::{fit, Control};
use kazari::prelude::*;
use kazari::{MockEnvironment, TerminalEnvironment};

fn tty(width: u16) -> Live<MockEnvironment> {
    Live::new(MockEnvironment::new(Capability::fixed(ColorLevel::Truecolor, width, true)))
}

fn pipe() -> Live<MockEnvironment> {
    Live::new(MockEnvironment::new(Capability::plain()))
}

fn line(text: &str) -> StyledLine {
    vec![Fragment::styled(text, Role::Text)]
}

fn plain_text(line: &StyledLine) -> String {
    line.iter().map(|f| f.text.as_str()).collect()
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[derive(Clone, Default)]
struct Shared(Rc<RefCell<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn tty_frame_wraps_in_sync_mode_and_hides_cursor() {
    let mut live = tty(40);
    assert!(live.is_live());
    assert!(live.frame_at(&[line("a"), line("b")], Instant::now()).unwrap());
    let out = live.env().rendered();
    let sync_begin = Control::SyncBegin.to_string();
    let sync_end = Control::SyncEnd.to_string();
    assert!(out.starts_with(&sync_begin), "{out:?}");
    assert!(out.ends_with(&sync_end), "{out:?}");
    assert_eq!(count(&out, &Control::HideCursor.to_string()), 1);
    assert_eq!(count(&out, &Control::ClearLine.to_string()), 2);
    assert!(!out.contains(&Control::ShowCursor.to_string()));
}

#[test]
fn tty_second_frame_moves_up_by_region_height() {
    let t0 = Instant::now();
    let mut live = tty(40);
    live.frame_at(&[line("a"), line("b"), line("c")], t0).unwrap();
    let first = live.env().rendered();
    assert!(!first.contains("\x1b[3A"));
    live.frame_at(&[line("d"), line("e")], t0 + Duration::from_millis(100)).unwrap();
    let all = live.env().rendered();
    let second = &all[first.len()..];
    let up = Control::Up(3).to_string();
    assert_eq!(up, "\x1b[3A");
    assert_eq!(count(second, &up), 1);
    assert_eq!(count(second, &Control::HideCursor.to_string()), 0);
    assert_eq!(count(second, &Control::ClearLine.to_string()), 2);
    assert!(second.contains(&Control::ClearBelow.to_string()));
}

#[test]
fn tty_frames_inside_min_interval_coalesce() {
    let t0 = Instant::now();
    let mut live = tty(40).with_min_interval(Duration::from_millis(80));
    assert!(live.frame_at(&[line("one")], t0).unwrap());
    assert!(!live.frame_at(&[line("two")], t0 + Duration::from_millis(10)).unwrap());
    assert!(!live.frame_at(&[line("three")], t0 + Duration::from_millis(79)).unwrap());
    let out = live.env().rendered();
    assert!(!out.contains("two") && !out.contains("three"));
    assert!(live.frame_at(&[line("four")], t0 + Duration::from_millis(80)).unwrap());
    assert!(live.env().rendered().contains("four"));
}

#[test]
fn tty_finish_paints_last_frame_and_restores_cursor() {
    let t0 = Instant::now();
    let mut live = tty(40);
    live.frame_at(&[line("one")], t0).unwrap();
    live.frame_at(&[line("last")], t0 + Duration::from_millis(1)).unwrap();
    live.finish().unwrap();
    live.finish().unwrap();
    let out = live.env().rendered();
    assert_eq!(count(&out, "last"), 1);
    let show = Control::ShowCursor.to_string();
    assert_eq!(count(&out, &show), 1);
    assert!(out.ends_with(&show), "{out:?}");
    assert!(!live.frame_at(&[line("after")], t0 + Duration::from_secs(1)).unwrap());
}

#[test]
fn tty_drop_restores_cursor() {
    let sink = Shared::default();
    let caps = Capability::fixed(ColorLevel::Ansi256, 40, true);
    {
        let mut live = Live::new(TerminalEnvironment::new(caps, Theme::default(), sink.clone()));
        live.frame_at(&[line("building")], Instant::now()).unwrap();
    }
    let out = String::from_utf8(sink.0.borrow().clone()).unwrap();
    let hide = out.find(&Control::HideCursor.to_string()).unwrap();
    let show = out.rfind(&Control::ShowCursor.to_string()).unwrap();
    assert!(show > hide);
}

#[test]
fn tty_commit_prints_above_and_repaints_region() {
    let t0 = Instant::now();
    let mut live = tty(40);
    live.frame_at(&[line("running x"), line("running y")], t0).unwrap();
    let before = live.env().rendered().len();
    live.commit(&line("built x")).unwrap();
    let all = live.env().rendered();
    let out = &all[before..];
    assert!(out.starts_with(&Control::SyncBegin.to_string()));
    assert_eq!(count(out, &Control::Up(2).to_string()), 1);
    let built = out.find("built x").unwrap();
    let region = out.find("running x").unwrap();
    assert!(built < region);
    assert_eq!(count(out, &Control::ClearLine.to_string()), 3);
    live.frame_at(&[line("running y")], t0 + Duration::from_millis(100)).unwrap();
    let tail = &live.env().rendered()[all.len()..];
    assert_eq!(count(tail, &Control::Up(2).to_string()), 1);
}

#[test]
fn pipe_frames_write_nothing_until_finish() {
    let t0 = Instant::now();
    let mut live = pipe();
    assert!(!live.is_live());
    for i in 0..5u64 {
        live.frame_at(&[line("tick"), vec![Spinner::new().fragment(i)]], t0 + Duration::from_secs(i)).unwrap();
    }
    assert_eq!(live.env().rendered(), "");
    live.frame_at(&[line("done 3/3")], t0 + Duration::from_secs(9)).unwrap();
    live.finish().unwrap();
    live.finish().unwrap();
    assert_eq!(live.env().rendered(), "done 3/3\n");
    assert!(live.env().has_no_escapes());
}

#[test]
fn pipe_commit_prints_plain_lines() {
    let mut live = pipe();
    live.frame_at(&[line("region")], Instant::now()).unwrap();
    live.commit(&vec![Fragment::styled("✓ ", Role::Ok), Fragment::styled("hello", Role::Ident)]).unwrap();
    live.commit(&line("world")).unwrap();
    live.finish().unwrap();
    assert_eq!(live.env().rendered(), "✓ hello\nworld\nregion\n");
    assert!(live.env().has_no_escapes());
}

#[test]
fn pipe_is_deterministic() {
    let run = || {
        let t0 = Instant::now();
        let mut live = pipe();
        live.commit(&line("a")).unwrap();
        live.frame_at(&[Meter::new(4).fragments(0.5, Role::Ok)], t0).unwrap();
        live.finish().unwrap();
        live.env().rendered()
    };
    assert_eq!(run(), run());
}

#[test]
fn fit_truncates_cjk_by_display_width() {
    let l = vec![Fragment::plain("ab"), Fragment::styled("漢字漢字", Role::Ident)];
    let cut = fit(&l, 5);
    assert_eq!(plain_text(&cut), "ab漢");
    assert_eq!(kazari::ir::line_width(&cut), 4);
    assert_eq!(cut[1].role, Some(Role::Ident));
    let cut = fit(&l, 6);
    assert_eq!(plain_text(&cut), "ab漢字");
    assert_eq!(plain_text(&fit(&l, 100)), "ab漢字漢字");
    assert!(fit(&l, 0).is_empty());
}

#[test]
fn tty_lines_truncate_to_terminal_width() {
    let mut live = tty(20);
    let long = "x".repeat(50);
    live.frame_at(&[line(&long), line("漢字漢字漢字漢字漢字漢字")], Instant::now()).unwrap();
    let out = live.env().rendered();
    assert!(out.contains(&"x".repeat(20)));
    assert!(!out.contains(&"x".repeat(21)));
    assert!(out.contains("漢字漢字漢字漢字漢字") && !out.contains("漢字漢字漢字漢字漢字漢"));
}

#[test]
fn meter_eighth_block_boundaries() {
    let cell = |ratio: f64| plain_text(&Meter::new(1).fragments(ratio, Role::Ok));
    assert_eq!(cell(0.0), "─");
    assert_eq!(cell(0.125), "▏");
    assert_eq!(cell(0.25), "▎");
    assert_eq!(cell(0.375), "▍");
    assert_eq!(cell(0.5), "▌");
    assert_eq!(cell(0.625), "▋");
    assert_eq!(cell(0.75), "▊");
    assert_eq!(cell(0.875), "▉");
    assert_eq!(cell(1.0), "█");
    let bar = Meter::new(10).fragments(0.3, Role::Ok);
    assert_eq!(plain_text(&bar), "███───────");
    assert_eq!(plain_text(&Meter::new(10).fragments(0.35, Role::Ok)), "███▌──────");
    assert_eq!(bar[0].role, Some(Role::Ok));
    assert_eq!(bar[1].role, Some(Role::Border));
    assert_eq!(plain_text(&Meter::new(4).fragments(0.5, Role::Ok)), "██──");
}

#[test]
fn meter_clamps_out_of_range() {
    let m = Meter::new(3);
    assert_eq!(plain_text(&m.fragments(-1.0, Role::Ok)), "───");
    assert_eq!(plain_text(&m.fragments(f64::NAN, Role::Ok)), "───");
    assert_eq!(plain_text(&m.fragments(7.0, Role::Ok)), "███");
    let over = m.segments(&[(0.8, Role::Ok), (0.8, Role::Info)]);
    assert_eq!(kazari::ir::line_width(&over), 3);
}

#[test]
fn meter_segments_share_one_bar() {
    let bar = Meter::new(8).segments(&[
        (0.25, Role::Info),
        (0.25, Role::Ok),
        (0.125, Role::Pending),
    ]);
    assert_eq!(plain_text(&bar), "█████───");
    let roles: Vec<_> = bar.iter().map(|f| (f.text.chars().count(), f.role)).collect();
    assert_eq!(
        roles,
        vec![
            (2, Some(Role::Info)),
            (2, Some(Role::Ok)),
            (1, Some(Role::Pending)),
            (3, Some(Role::Border)),
        ]
    );
}

#[test]
fn meter_ascii_without_unicode() {
    let m = Meter::for_caps(4, &Capability::plain());
    assert_eq!(plain_text(&m.fragments(0.5, Role::Ok)), "##--");
}

#[test]
fn spark_scales_to_max_and_pads() {
    let s = Spark::new(6);
    assert_eq!(s.fragment(&[0.0, 1.0, 2.0, 7.0]).text, "  ▁▂▃█");
    assert_eq!(s.fragment(&[9.0, 1.0, 1.0]).text, "   █▂▂");
    assert_eq!(s.fragment(&[0.0; 10]).text, "▁▁▁▁▁▁");
    assert_eq!(s.fragment(&[]).text, "      ");
    assert_eq!(s.fragment(&[5.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]).text.chars().count(), 6);
    assert_eq!(s.fragment(&[1.0]).role, Some(Role::TextMuted));
}

#[test]
fn spinner_frame_follows_injected_tick() {
    let s = Spinner::new();
    assert_eq!(s.glyph(0), "⠋");
    assert_eq!(s.glyph(9), "⠏");
    assert_eq!(s.glyph(10), "⠋");
    assert_eq!(s.at(Duration::from_millis(0)).text, "⠋");
    assert_eq!(s.at(Duration::from_millis(160)).text, "⠹");
    assert_eq!(s.fragment(3).role, Some(Role::Primary));
    assert_eq!(Spinner::for_caps(&Capability::plain()).glyph(1), "/");
}

#[test]
fn pulse_alternates_and_settles() {
    let p = Pulse::alternate(Role::Text, Role::TextMuted, 2);
    let roles: Vec<_> = (0..6).map(|t| p.role(t)).collect();
    assert_eq!(roles, vec![Role::Text, Role::Text, Role::TextMuted, Role::TextMuted, Role::Text, Role::Text]);
    let s = Pulse::settle(Role::Ok, Role::TextMuted, 3);
    assert_eq!(s.role(0), Role::Ok);
    assert_eq!(s.role(2), Role::Ok);
    assert_eq!(s.role(3), Role::TextMuted);
    assert_eq!(s.role(1000), Role::TextMuted);
    assert_eq!(s.fragment("done", 0).role, Some(Role::Ok));
}
