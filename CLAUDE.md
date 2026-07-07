# Claude Code Guidelines — verovio-rust

Port of **Verovio** (C++ music engraving library, LGPL-3.0) to **Rust**,
rendering through **agg-gui** instead of SVG. Lives in its own repository so
the LGPL license is preserved intact — apps (MIT) consume it as a library.
The pinned C++ source is the submodule at `verovio-cpp-reference/`
(revision `8d42439`, release 6.2.1 — the same revision the KeyInSight Swift
app pinned).

## Read before working

| When you are… | Read |
|---|---|
| Porting engraving behavior from C++ | `docs/porting.md` — reference layout, the subset roadmap, fidelity rules |
| Rendering or fonts | `docs/rendering.md` — agg-gui DrawCtx mapping, SMuFL/Leipzig font, light-page default |

## Non-negotiable rules

- **License:** this repo is LGPL-3.0-or-later (`COPYING`, `COPYING.LESSER`),
  matching upstream Verovio. Never move this code into an MIT repo; apps
  depend on it as a library.
- **All rendering through agg-gui `DrawCtx`.** Where C++ Verovio emits SVG,
  this port paints paths and SMuFL glyphs. No SVG string pipeline.
- **Music renders light.** The engraving defaults assume a light page
  (black ink on white); callers may recolor per element but the library
  never inverts for dark UI themes.
- **Per-element addressability is the point.** Every notehead/rest/etc.
  carries a stable id with queryable bounds — consumers recolor noteheads,
  place cursors, and hit-test. Never lose ids in a refactor.
- **No stubs.** Features are either complete for the documented subset or
  absent; unsupported input is rejected with a specific error.
- **Test-first bug fixing**; never weaken a test to make it pass.
- **800-line file limit** (`tests/file_line_count.rs`) — refactor into
  modules, never compress or bump.

## Quick commands

```powershell
cargo test          # requires sibling ../agg-gui checkout (path patch)
```

## Shell

Windows / **PowerShell**. Heredocs don't work.
