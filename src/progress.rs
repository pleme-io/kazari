//! A live board for a graph of jobs: the plan folded into waves that converge
//! on the root, the jobs running now, and the rate of completion. Pure and
//! clock-free: every call takes the elapsed time, so a frame is a function of
//! the events alone.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::{BuildHasher, Hash};
use std::fmt::Write as _;
use std::time::Duration;

use crate::caps::Capability;
use crate::ir::{Fragment, StyledLine};
use crate::live::{fit, Meter, Pulse, Spark, Spinner};
use crate::theme::Role;

/// Compute that runs here, or bytes that arrive from elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    Work,
    Transfer,
}

/// How a lane ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Transferred,
    Failed,
}

/// One depth of the plan: jobs whose deepest input sits one wave below.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wave {
    pub total: usize,
    pub done: usize,
    pub running: usize,
    pub failed: usize,
    pub last_done: Option<Duration>,
}

/// A job in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    pub name: String,
    pub kind: LaneKind,
    pub phase: Option<String>,
    pub since: Duration,
    pub bytes: (u64, u64),
    wave: Option<usize>,
}

/// A job that ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub name: String,
    pub outcome: Outcome,
    pub took: Duration,
    pub at: Duration,
}

/// The words a board speaks: its owner, its verb and its target, and the
/// past tense of each lane kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labels {
    pub owner: String,
    pub verb: String,
    pub target: String,
    pub work: String,
    pub transfer: String,
}

/// The state of one run.
#[derive(Debug, Clone)]
pub struct Board<K: Eq + Hash + Clone> {
    pub labels: Labels,
    pub waves: Vec<Wave>,
    pub done: usize,
    pub transferred: usize,
    pub failed: usize,
    pub bytes: u64,
    pub planned_work: usize,
    pub planned_transfer: usize,
    pub lanes: BTreeMap<u64, Lane>,
    pub finished: VecDeque<Finished>,
    depth: HashMap<K, usize>,
    completions: Vec<Duration>,
}

const KEEP_FINISHED: usize = 64;
const SHOWN_LANES: usize = 6;
const FLASH: Duration = Duration::from_millis(600);

/// Each node's wave: one above its deepest input inside the graph.
pub fn waves_of<K: Eq + Hash + Clone, S: BuildHasher>(inputs: &HashMap<K, Vec<K>, S>) -> HashMap<K, usize> {
    let mut depth: HashMap<K, usize> = HashMap::new();
    for root in inputs.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((node, expanded)) = stack.pop() {
            if depth.contains_key(&node) {
                continue;
            }
            let deps: Vec<K> = inputs
                .get(&node)
                .map(|d| d.iter().filter(|d| inputs.contains_key(*d)).cloned().collect())
                .unwrap_or_default();
            if expanded {
                let d = deps.iter().filter_map(|x| depth.get(x)).map(|d| d + 1).max().unwrap_or(0);
                depth.insert(node, d);
            } else {
                let pending: Vec<K> = deps.into_iter().filter(|d| !depth.contains_key(d)).collect();
                stack.push((node, true));
                stack.extend(pending.into_iter().map(|d| (d, false)));
            }
        }
    }
    depth
}

impl<K: Eq + Hash + Clone> Board<K> {
    #[must_use]
    pub fn new(labels: Labels) -> Self {
        Self {
            labels,
            waves: Vec::new(),
            done: 0,
            transferred: 0,
            failed: 0,
            bytes: 0,
            planned_work: 0,
            planned_transfer: 0,
            lanes: BTreeMap::new(),
            finished: VecDeque::new(),
            depth: HashMap::new(),
            completions: Vec::new(),
        }
    }

    /// The jobs to run and their inputs, and how many transfers come first.
    pub fn plan<S: BuildHasher>(&mut self, inputs: &HashMap<K, Vec<K>, S>, transfers: usize) {
        self.depth = waves_of(inputs);
        let n = self.depth.values().max().map_or(0, |m| m + 1);
        self.waves = vec![Wave::default(); n];
        for d in self.depth.values() {
            self.waves[*d].total += 1;
        }
        self.planned_work = inputs.len();
        self.planned_transfer = transfers;
    }

    pub fn begin(&mut self, id: u64, key: Option<&K>, name: impl Into<String>, kind: LaneKind, now: Duration) {
        let wave = key.and_then(|k| self.depth.get(k)).copied();
        if let Some(w) = wave {
            self.waves[w].running += 1;
        }
        self.lanes.insert(id, Lane { name: name.into(), kind, phase: None, since: now, bytes: (0, 0), wave });
    }

    pub fn phase(&mut self, id: u64, phase: impl Into<String>) {
        if let Some(lane) = self.lanes.get_mut(&id) {
            lane.phase = Some(phase.into());
        }
    }

    pub fn progress(&mut self, id: u64, done: u64, expected: u64) {
        if let Some(lane) = self.lanes.get_mut(&id) {
            lane.bytes = (done, expected);
        }
    }

    /// End a lane; returns what ended so the caller can commit it above.
    pub fn end(&mut self, id: u64, ok: bool, now: Duration) -> Option<Finished> {
        let lane = self.lanes.remove(&id)?;
        if let Some(w) = lane.wave {
            let wave = &mut self.waves[w];
            wave.running = wave.running.saturating_sub(1);
            if ok {
                wave.done += 1;
                wave.last_done = Some(now);
            } else {
                wave.failed += 1;
            }
        }
        let outcome = match (ok, lane.kind) {
            (false, _) => Outcome::Failed,
            (true, LaneKind::Work) => Outcome::Done,
            (true, LaneKind::Transfer) => Outcome::Transferred,
        };
        match outcome {
            Outcome::Done => self.done += 1,
            Outcome::Transferred => {
                self.transferred += 1;
                self.bytes += lane.bytes.0.max(lane.bytes.1);
            }
            Outcome::Failed => self.failed += 1,
        }
        self.completions.push(now);
        let finished = Finished { name: lane.name, outcome, took: now.saturating_sub(lane.since), at: now };
        self.finished.push_back(finished.clone());
        while self.finished.len() > KEEP_FINISHED {
            self.finished.pop_front();
        }
        Some(finished)
    }

    #[must_use]
    pub fn left(&self) -> usize {
        (self.planned_work + self.planned_transfer).saturating_sub(self.done + self.transferred + self.failed)
    }

    #[must_use]
    pub fn ratio(&self) -> f64 {
        let total = self.planned_work + self.planned_transfer;
        if total == 0 {
            return 0.0;
        }
        (self.done + self.transferred + self.failed) as f64 / total as f64
    }

    #[must_use]
    pub fn current_wave(&self) -> usize {
        self.waves.iter().position(|w| w.done + w.failed < w.total).unwrap_or(self.waves.len())
    }

    /// Completions per bucket, oldest first, the newest bucket ending `now`.
    #[must_use]
    pub fn rate(&self, now: Duration, buckets: usize, width: Duration) -> Vec<f64> {
        let mut out = vec![0.0; buckets];
        for at in &self.completions {
            let i = (now.saturating_sub(*at).as_secs_f64() / width.as_secs_f64()) as usize;
            if i < buckets {
                out[buckets - 1 - i] += 1.0;
            }
        }
        out
    }

    /// The board at `now`, fitted to `width` cells.
    #[must_use]
    pub fn frame(&self, now: Duration, caps: &Capability, width: usize) -> Vec<StyledLine> {
        let mut lines = vec![self.header(now, caps, width), self.sprawl(now, caps, width), self.counts(caps)];
        let mut running: Vec<&Lane> = self.lanes.values().collect();
        running.sort_by_key(|l| l.since);
        let name_width = running.iter().take(SHOWN_LANES).map(|l| l.name.chars().count()).max().unwrap_or(0).min(40);
        for job in running.iter().take(SHOWN_LANES) {
            lines.push(Self::lane(job, now, caps, name_width));
        }
        if running.len() > SHOWN_LANES {
            lines.push(vec![Fragment::faint(fmt(format_args!("  + {} more", running.len() - SHOWN_LANES)), Role::TextDim)]);
        }
        lines.push(self.pace(now, caps));
        lines.into_iter().map(|l| fit(&l, width)).collect()
    }

    /// A finished job as one settled line, for committing above the board.
    #[must_use]
    pub fn finished_line(f: &Finished, caps: &Capability) -> StyledLine {
        let (mark, role) = match (f.outcome, caps.unicode) {
            (Outcome::Failed, true) => ("✗", Role::Error),
            (Outcome::Failed, false) => ("x", Role::Error),
            (Outcome::Transferred, true) => ("↓", Role::Info),
            (Outcome::Transferred, false) => ("v", Role::Info),
            (Outcome::Done, true) => ("✓", Role::Ok),
            (Outcome::Done, false) => ("+", Role::Ok),
        };
        vec![
            Fragment::styled(mark, role),
            Fragment::plain(" "),
            Fragment::styled(f.name.clone(), if f.outcome == Outcome::Failed { Role::Error } else { Role::TextMuted }),
            Fragment::faint(fmt(format_args!("  {}", clock(f.took))), Role::TextDim),
        ]
    }

    fn header(&self, now: Duration, caps: &Capability, width: usize) -> StyledLine {
        let left = vec![
            Fragment::accent(self.labels.owner.clone(), Role::Primary),
            Fragment::styled(fmt(format_args!(" {} {} ", dot(caps), self.labels.verb)), Role::TextMuted),
            Fragment::styled(self.labels.target.clone(), Role::Text),
        ];
        let time = clock(now);
        let used: usize = left.iter().map(Fragment::width).sum::<usize>() + time.chars().count();
        let mut line = left;
        line.push(Fragment::plain(" ".repeat(width.saturating_sub(used + 1).max(2))));
        line.push(Fragment::faint(time, Role::TextDim));
        line
    }

    fn sprawl(&self, now: Duration, caps: &Capability, width: usize) -> StyledLine {
        let columns = (width / 3).clamp(8, 48);
        let mut line = Vec::new();
        if self.waves.is_empty() {
            line.push(Fragment::faint("planning", Role::TextDim));
            return line;
        }
        let per = self.waves.len().div_ceil(columns);
        let groups: Vec<&[Wave]> = self.waves.chunks(per).collect();
        let widest = groups.iter().map(|g| g.iter().map(|w| w.total).sum::<usize>()).max().unwrap_or(1).max(1);
        let levels: &[&str] = if caps.unicode { &["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"] } else { &["."] };
        for (i, group) in groups.iter().enumerate() {
            let total: usize = group.iter().map(|w| w.total).sum();
            let settled: usize = group.iter().map(|w| w.done + w.failed).sum();
            let running: usize = group.iter().map(|w| w.running).sum();
            let failed: usize = group.iter().map(|w| w.failed).sum();
            let last = group.iter().filter_map(|w| w.last_done).max();
            let root = i == groups.len() - 1 && total == 1;
            let glyph = if root {
                if caps.unicode { "◆" } else { "*" }
            } else {
                let scaled = ((total as f64).ln_1p() / (widest as f64).ln_1p() * (levels.len() - 1) as f64).round() as usize;
                levels[scaled.min(levels.len() - 1)]
            };
            let fragment = if failed > 0 {
                Fragment::styled(glyph, Role::Error)
            } else if settled == total {
                match last {
                    Some(at) => Pulse::settle(Role::Primary, Role::Ok, 1).fragment(glyph, u64::from(now.saturating_sub(at) >= FLASH)),
                    None => Fragment::styled(glyph, Role::Ok),
                }
            } else if running > 0 {
                Fragment::accent(glyph, Role::Primary)
            } else if settled > 0 {
                Fragment::styled(glyph, Role::TextMuted)
            } else {
                Fragment::faint(glyph, Role::TextDim)
            };
            line.push(fragment);
        }
        let waves = self.waves.len();
        line.push(Fragment::faint(
            fmt(format_args!("  wave {}/{}  ", (self.current_wave() + 1).min(waves), waves)),
            Role::TextDim,
        ));
        let total = (self.planned_work + self.planned_transfer).max(1) as f64;
        let running = self.lanes.len() as f64;
        line.extend(Meter::for_caps(24, caps).segments(&[
            (self.done as f64 / total, Role::Ok),
            (self.transferred as f64 / total, Role::Info),
            (self.failed as f64 / total, Role::Error),
            (running / total, Role::Primary),
        ]));
        line.push(Fragment::styled(fmt(format_args!(" {:>3}%", (self.ratio() * 100.0).floor())), Role::Text));
        line
    }

    fn counts(&self, caps: &Capability) -> StyledLine {
        let sep = fmt(format_args!(" {} ", dot(caps)));
        let mut line = Vec::new();
        let mut push = |n: String, word: &str, role: Role| {
            if !line.is_empty() {
                line.push(Fragment::faint(sep.clone(), Role::TextDim));
            }
            line.push(Fragment::styled(word.to_string(), Role::TextMuted));
            line.push(Fragment::plain(" "));
            line.push(Fragment::styled(n, role));
        };
        push(grouped(self.done), &self.labels.work.clone(), Role::Text);
        let fetched = if self.bytes > 0 { fmt(format_args!("{} ({})", grouped(self.transferred), bytes(self.bytes))) } else { grouped(self.transferred) };
        push(fetched, &self.labels.transfer.clone(), Role::Text);
        push(grouped(self.lanes.len()), "running", Role::Text);
        if self.failed > 0 {
            push(grouped(self.failed), "failed", Role::Error);
        }
        push(grouped(self.left()), "left", Role::Text);
        line
    }

    fn lane(job: &Lane, now: Duration, caps: &Capability, name_width: usize) -> StyledLine {
        let elapsed = now.saturating_sub(job.since);
        let mark = match job.kind {
            LaneKind::Work => Spinner::for_caps(caps).role(Role::Primary).at(elapsed),
            LaneKind::Transfer => Fragment::styled(if caps.unicode { "↓" } else { "v" }, Role::Info),
        };
        let name: String = job.name.chars().take(name_width).collect();
        let pad = name_width.saturating_sub(name.chars().count());
        let mut line = vec![mark, Fragment::plain(" "), Fragment::styled(name, Role::Text), Fragment::plain(" ".repeat(pad + 2))];
        match (job.kind, job.bytes) {
            (LaneKind::Transfer, (done, expected)) if expected > 0 => {
                line.extend(Meter::for_caps(12, caps).fragments(done as f64 / expected as f64, Role::Info));
                line.push(Fragment::faint(fmt(format_args!(" {} / {}", bytes(done), bytes(expected))), Role::TextDim));
            }
            _ => {
                if let Some(phase) = &job.phase {
                    line.push(Fragment::styled(fmt(format_args!("{phase:<14}")), Role::TextMuted));
                }
            }
        }
        line.push(Fragment::faint(fmt(format_args!(" {}", clock(elapsed))), Role::TextDim));
        line
    }

    fn pace(&self, now: Duration, caps: &Capability) -> StyledLine {
        let window = 30;
        let rate = self.rate(now, window, Duration::from_secs(2));
        let recent: f64 = rate.iter().sum::<f64>() / (window as f64 * 2.0) * 60.0;
        vec![
            Spark::for_caps(window, caps).role(Role::Info).fragment(&rate),
            Fragment::faint(fmt(format_args!("  {recent:.0}/min")), Role::TextDim),
        ]
    }
}

fn fmt(args: std::fmt::Arguments<'_>) -> String {
    let mut out = String::new();
    let _ = out.write_fmt(args);
    out
}

fn dot(caps: &Capability) -> &'static str {
    if caps.unicode { "·" } else { "-" }
}

/// `m:ss`, or `h:mm:ss` past an hour.
#[must_use]
pub fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 { fmt(format_args!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)) } else { fmt(format_args!("{}:{:02}", s / 60, s % 60)) }
}

/// A count with thin-space thousands groups: `12 345`.
#[must_use]
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// A byte count in binary units: `1.2 GiB`.
#[must_use]
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { fmt(format_args!("{n} B")) } else { fmt(format_args!("{v:.1} {}", UNITS[u])) }
}
