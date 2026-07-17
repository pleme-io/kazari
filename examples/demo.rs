//! A taste of kazari — the line-output face, dark-Nord/blackmatter.
//!
//! Run it in a truecolor terminal (mado) to see the Nord palette:
//!     cargo run --example demo
//! Pipe it and watch it degrade honestly to plain `[TAG]` text:
//!     cargo run --example demo | cat

use kazari::prelude::*;

fn main() {
    banner("sui — Rust-native Nix").print().unwrap();
    println!();

    ok("realised /nix/store/…-hello-2.12.1 (byte-parity verified)").print().unwrap();
    info("evaluating flake outputs").print().unwrap();
    warn("cache miss — rebuilding from source").print().unwrap();
    error("exit 1 in phase `build`").with_title("hello-2.12.1").eprint().unwrap();
    println!();

    rule_labelled("store").print().unwrap();
    panel()
        .row("path", "/nix/store/abc123-hello-2.12.1")
        .row("closure-size", "42.1 MiB")
        .row("references", "7")
        .print()
        .unwrap();
    println!();

    rule_labelled("realise").print().unwrap();
    list()
        .task(CalloutSeverity::Ok, "fetch      sources")
        .task(CalloutSeverity::Ok, "unpack     tarball")
        .task(CalloutSeverity::Warn, "patch      1 hunk fuzzed")
        .task(CalloutSeverity::Error, "build      compiler error")
        .bullet("configure phase skipped")
        .print()
        .unwrap();
    println!();

    print!("verdict  ");
    badge("BYTE-PARITY", Role::Ok).print().unwrap();
}
