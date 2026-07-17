# kazari (飾り)

**The line-output face of the fleet.** Typed, capability-honest, dark-Nord /
blackmatter beautiful CLI output — banners, callouts, panels, rules, lists,
badges (tables / trees / progress / diagnostics at M2) — as a pure-data,
`format!()`-free Block IR.

kazari is the **sibling of [seki](https://github.com/pleme-io/seki)**: seki
dresses the *prompt* (before the command), kazari dresses the *output* (after
the command), over the one [ishou](https://github.com/pleme-io/ishou) /
[irodori](https://github.com/pleme-io/irodori) token spine that also feeds
egaku-term (the cell-grid) and tela (the web). Four faces, one spine.

## The wall is a type

A `Block` carries **no** color depth. `render()` requires a `Capability`
supplied by the environment — so *"assumed truecolor on a pipe"* is a
compile-shaped absence, not a runtime clamp. The same value degrades
`truecolor → 256 → 16 → NO_COLOR` through one total morphism.

```rust
use kazari::prelude::*;

banner("sui — Rust-native Nix").print()?;          // rounded box, Frost accent
ok("realised /nix/store/…-hello (byte-parity)").print()?;   // ✓ Aurora green
warn("cache miss — rebuilding").print()?;          // ⚠ Aurora orange
error("exit 1").with_title("build").eprint()?;     // ✗ Aurora red

Panel::titled("store")
    .row("path", "/nix/store/…")
    .row("closure-size", "42 MiB")
    .print()?;
```

Piped or under `NO_COLOR`, the same code emits **zero escape bytes** and a
plain `[TAG]` / ASCII-glyph projection.

## What it owns vs reuses

- **OWNS** (the ~15% genuinely-new, 0-hit fleet gap): the `ColorLevel`
  capability wall, the `RGB→256→16` nearest-in-**linear** quantizer, the
  line-block AST.
- **CONSUMES** `irodori` (Nord palette, no fork) · **WRAPS** `anstyle` (typed
  SGR — the escape bytes are anstyle's, never hand-spelled).

Dark-Nord by default (fully morphable). Design + tier-honest ledger:
[`theory/KAZARI.md`](https://github.com/pleme-io/theory/blob/main/KAZARI.md).

## License

MIT.
