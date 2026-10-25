use std::collections::HashMap;
use std::time::{Duration, Instant};

use kazari::progress::{Board, Labels, LaneKind};
use kazari::{Capability, Live, Stream};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n.max(1)
    }
}

const PHASES: [&str; 4] = ["unpackPhase", "configurePhase", "buildPhase", "installPhase"];

fn main() -> std::io::Result<()> {
    let mut rng = Lcg(7);
    let jobs = 400usize;
    let names: Vec<String> = (0..jobs).map(|i| ["rust_crate-", &i.to_string(), "-0.1.0"].concat()).collect();
    let mut inputs: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..jobs - 1 {
        let deps = (0..rng.next(4)).filter(|_| i > 0).map(|_| rng.next(i as u64) as usize).collect();
        inputs.insert(i, deps);
    }
    inputs.insert(jobs - 1, (jobs - 40..jobs - 1).collect());

    let mut board = Board::new(Labels {
        owner: "sui".into(),
        verb: "realize".into(),
        target: "darwin-system-25.11".into(),
        work: "built".into(),
        transfer: "fetched".into(),
    });
    board.plan(&inputs, 12);
    let caps = Capability::cached(Stream::Stderr);
    let mut live = Live::stderr();
    let start = Instant::now();
    let mut done: Vec<bool> = vec![false; jobs];
    let mut running: Vec<(u64, usize, Duration)> = Vec::new();
    let mut next_id = 0u64;
    let mut fetches = 12;

    loop {
        let now = start.elapsed();
        running.retain(|&(id, job, until)| {
            if now >= until {
                if let Some(f) = board.end(id, true, now) {
                    let _ = live.commit(&Board::<usize>::finished_line(&f, &caps));
                }
                if job < jobs {
                    done[job] = true;
                }
                false
            } else {
                true
            }
        });
        if fetches > 0 && running.len() < 8 {
            let size = 1 + rng.next(80_000_000);
            board.begin(next_id, None, "source", LaneKind::Transfer, now);
            board.progress(next_id, size, size);
            running.push((next_id, usize::MAX, now + Duration::from_millis(300 + rng.next(900))));
            next_id += 1;
            fetches -= 1;
        }
        for job in 0..jobs {
            if running.len() >= 8 {
                break;
            }
            let busy = running.iter().any(|&(_, j, _)| j == job);
            if !done[job] && !busy && inputs[&job].iter().all(|d| done[*d]) {
                board.begin(next_id, Some(&job), names[job].clone(), LaneKind::Work, now);
                board.phase(next_id, PHASES[rng.next(4) as usize]);
                running.push((next_id, job, now + Duration::from_millis(150 + rng.next(1_400))));
                next_id += 1;
            }
        }
        let width = usize::from(caps.width);
        live.frame(&board.frame(now, &caps, width))?;
        if done.iter().all(|d| *d) && running.is_empty() {
            live.frame(&board.frame(now, &caps, width))?;
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    live.finish()
}
