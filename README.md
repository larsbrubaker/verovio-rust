# verovio-rust

A Rust port of [Verovio](https://github.com/rism-digital/verovio) music
engraving, rendering through [agg-gui](https://github.com/larsbrubaker/agg-gui)
instead of SVG. Subset-first: each release engraves a documented MusicXML
subset completely — with per-element ids, bounds lookup, color overrides,
and an onset timemap — and rejects anything it can't engrave faithfully.

Built as the notation engine for
[keyinsight-rust](https://github.com/larsbrubaker/keyinsight-rust); useful
to any agg-gui app that needs staff notation.

## License

**LGPL-3.0-or-later**, matching upstream Verovio (see `COPYING` and
`COPYING.LESSER`). This library deliberately lives in its own repository so
MIT-licensed applications can consume it while its license stays intact.
The bundled Leipzig font is SIL OFL 1.1 (Verovio's own SMuFL font). The
pinned C++ source is the `verovio-cpp-reference/` submodule.

## Usage

```rust
let mut toolkit = verovio_rust::Toolkit::new();
toolkit.load_music_xml(&music_xml)?;
let layout = toolkit.layout(&verovio_rust::LayoutOptions::default());
// inside an agg-gui widget's paint():
toolkit.render(ctx, &verovio_rust::RenderOptions::default());
```

See `docs/rendering.md` for the DrawCtx/SMuFL details and `docs/porting.md`
for the supported subset and the porting rules.

## Building

```powershell
# agg-gui is path-patched to a sibling checkout
git clone https://github.com/larsbrubaker/agg-gui.git ../agg-gui
cargo test
```
