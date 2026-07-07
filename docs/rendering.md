# Rendering — agg-gui and SMuFL

## DrawCtx mapping

C++ Verovio renders to SVG through its `DeviceContext` abstraction. This
port's equivalent is agg-gui's `DrawCtx`:

| Verovio SVG | This port |
|---|---|
| `<path>` staff lines, stems, barlines, beams | `begin_path` + `move_to`/`line_to` + `fill`/`stroke` |
| SMuFL glyphs via `<text>`/`<use>` | `set_font(leipzig)` + `set_font_size` + `fill_text` with the glyph's code point |
| Ties/slurs as cubic beziers | `cubic_to` filled shapes |
| per-element `id` attributes | `Element::id` + `element_bounds(id)` |

Coordinates are the caller's logical pixels, **y-down from the top-left of
the score box** internally; the widget hosting the score flips into
agg-gui's y-up world when painting (one flip, at the boundary — mirrors how
agg-gui apps handle y).

## Font

`assets/Leipzig.ttf` — Verovio's own SMuFL font (SIL OFL 1.1), pinned from
the reference submodule. Load with `leipzig_font()`; glyph code points are
in `smufl.rs` (named per the SMuFL spec, e.g. `G_CLEF = '\u{E050}'`).
Glyph sizing follows the SMuFL convention: a font size of `4 × staff_space`
in pixels makes one em span the five-line staff.

## Light page

Music engraving assumes a light page: the default render options are black
ink; the host widget should paint a white/paper background. Per-element
color overrides exist for feedback (correct/wrong noteheads, cursor) — not
for dark-theming the page. Consumers that run a dark UI still present the
score panel light.
