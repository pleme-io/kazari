use std::collections::HashMap;
use std::time::Duration;

use kazari::caps::ColorLevel;
use kazari::ir::line_width;
use kazari::progress::{bytes, clock, grouped, waves_of, Board, Labels, LaneKind, Outcome};
use kazari::{Capability, Role, StyledLine};

fn graph(edges: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
    edges.iter().map(|(n, d)| ((*n).to_string(), d.iter().map(|s| (*s).to_string()).collect())).collect()
}

fn labels() -> Labels {
    Labels {
        owner: "sui".into(),
        verb: "realize".into(),
        target: "system".into(),
        work: "built".into(),
        transfer: "fetched".into(),
    }
}

fn text(line: &StyledLine) -> String {
    line.iter().map(|f| f.text.as_str()).collect()
}

fn s(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn a_node_sits_one_wave_above_its_deepest_input() {
    let w = waves_of(&graph(&[("top", &["a", "b"]), ("a", &["c"]), ("b", &[]), ("c", &["outside"])]));
    assert_eq!((w["c"], w["b"], w["a"], w["top"]), (0, 0, 1, 2));
}

#[test]
fn the_board_counts_each_wave_and_converges_on_the_root() {
    let mut b = Board::new(labels());
    b.plan(&graph(&[("top", &["l1", "l2"]), ("l1", &[]), ("l2", &[])]), 1);
    assert_eq!(b.waves.iter().map(|w| w.total).collect::<Vec<_>>(), vec![2, 1]);
    b.begin(1, Some(&"l1".to_string()), "l1", LaneKind::Work, s(0));
    b.begin(2, None, "src", LaneKind::Transfer, s(0));
    b.progress(2, 512, 1024);
    assert_eq!(b.waves[0].running, 1);
    assert_eq!(b.end(1, true, s(3)).map(|f| f.outcome), Some(Outcome::Done));
    b.end(2, true, s(4));
    assert_eq!((b.done, b.transferred, b.bytes, b.left()), (1, 1, 1024, 2));
    assert_eq!(b.current_wave(), 0);
    b.begin(3, Some(&"l2".to_string()), "l2", LaneKind::Work, s(4));
    b.end(3, true, s(5));
    assert_eq!(b.current_wave(), 1);
}

#[test]
fn the_rate_puts_recent_completions_in_the_last_bucket() {
    let mut b: Board<String> = Board::new(labels());
    for i in 0..3u64 {
        b.begin(i, None, "x", LaneKind::Work, Duration::ZERO);
        b.end(i, true, s(10 - i));
    }
    assert_eq!(b.rate(s(10), 4, s(1)), vec![0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn a_frame_names_the_owner_and_never_exceeds_the_width() {
    let mut b = Board::new(labels());
    b.plan(&graph(&[("top", &["a"]), ("a", &[])]), 0);
    b.begin(1, Some(&"a".to_string()), "a-very-long-derivation-name-that-goes-on-and-on-and-on", LaneKind::Work, s(0));
    b.phase(1, "buildPhase");
    let caps = Capability::fixed(ColorLevel::Truecolor, 60, true);
    let frame = b.frame(s(75), &caps, 60);
    assert!(text(&frame[0]).starts_with("sui · realize system"));
    assert!(text(&frame[0]).ends_with("1:15"));
    assert!(frame.iter().all(|l| line_width(l) <= 60), "{frame:?}");
    assert!(frame.iter().any(|l| text(l).contains("buildPhase")));
    assert!(text(&frame[1]).contains("wave 1/2"));
    assert!(text(&frame[1]).contains('◆'));
}

#[test]
fn a_wave_that_just_finished_flashes_then_settles() {
    let mut b = Board::new(labels());
    b.plan(&graph(&[("top", &["a"]), ("a", &[])]), 0);
    b.begin(1, Some(&"a".to_string()), "a", LaneKind::Work, s(0));
    b.end(1, true, s(10));
    let caps = Capability::fixed(ColorLevel::Truecolor, 80, true);
    let role_at = |now: Duration| b.frame(now, &caps, 80)[1][0].role;
    assert_eq!(role_at(Duration::from_millis(10_100)), Some(Role::Primary));
    assert_eq!(role_at(s(12)), Some(Role::Ok));
}

#[test]
fn a_plain_terminal_gets_ascii_only() {
    let mut b = Board::new(labels());
    b.plan(&graph(&[("top", &["a"]), ("a", &[])]), 0);
    b.begin(1, Some(&"a".to_string()), "a", LaneKind::Work, s(0));
    let frame = b.frame(s(1), &Capability::plain(), 80);
    assert!(frame.iter().all(|l| text(l).is_ascii()), "{frame:?}");
}

#[test]
fn numbers_read_at_a_glance() {
    assert_eq!(grouped(1_234_567), "1 234 567");
    assert_eq!(grouped(12), "12");
    assert_eq!(bytes(1_288_490_189), "1.2 GiB");
    assert_eq!(bytes(512), "512 B");
    assert_eq!(clock(s(3725)), "1:02:05");
}

#[test]
fn a_full_bar_is_full_however_its_segments_split_a_cell() {
    let m = kazari::Meter::new(24);
    let line: StyledLine = m.segments(&[(400.0 / 412.0, Role::Ok), (12.0 / 412.0, Role::Info)]);
    assert_eq!(text(&line), "█".repeat(24));
}
