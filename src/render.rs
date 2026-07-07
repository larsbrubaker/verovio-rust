//! Paints a [`Layout`](crate::layout::Layout) through agg-gui's `DrawCtx`.
//!
//! The layout is y-down from the top-left of the score box; agg-gui is
//! y-up. The conversion happens here, per primitive, so glyphs render
//! upright (a transform flip would mirror them).
//!
//! Music renders on a light page: the defaults are black ink and no
//! background fill (the host widget paints paper white). Per-element color
//! overrides serve feedback (correct/wrong/cursor coloring by element id),
//! never dark theming — see `docs/rendering.md`.

use std::collections::HashMap;
use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::text::Font;

use crate::layout::{Layout, Primitive};

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Default ink for every element.
    pub ink: Color,
    /// Per-element-id ink overrides (feedback coloring).
    pub overrides: HashMap<String, Color>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            // Not-quite-black engraving ink, as printed music uses.
            ink: Color::from_rgb8(20, 20, 24),
            overrides: HashMap::new(),
        }
    }
}

/// Draw the layout with its top-left at `(origin_x, origin_y_top)` in the
/// host widget's y-up coordinates. `origin_y_top` is the y of the score
/// box's TOP edge (so callers pass `widget_top`, not bottom).
pub fn render(
    ctx: &mut dyn DrawCtx,
    font: &Arc<Font>,
    layout: &Layout,
    origin_x: f64,
    origin_y_top: f64,
    options: &RenderOptions,
) {
    ctx.set_font(Arc::clone(font));
    // y-down layout → y-up canvas.
    let flip = |y: f64| origin_y_top - y;

    for element in &layout.elements {
        let color = element
            .id
            .as_ref()
            .and_then(|id| options.overrides.get(id))
            .copied()
            .unwrap_or(options.ink);
        ctx.set_fill_color(color);
        ctx.set_stroke_color(color);
        match &element.primitive {
            Primitive::Line { x1, y1, x2, y2, thickness } => {
                ctx.set_line_width(*thickness);
                ctx.begin_path();
                ctx.move_to(origin_x + x1, flip(*y1));
                ctx.line_to(origin_x + x2, flip(*y2));
                ctx.stroke();
            }
            Primitive::Glyph { ch, x, y, size } => {
                ctx.set_font_size(*size);
                let mut buf = [0u8; 4];
                ctx.fill_text(ch.encode_utf8(&mut buf), origin_x + x, flip(*y));
            }
            Primitive::Tie { x1, y1, x2, y2, bulge } => {
                // Classic tapered tie: outer arc out, inner arc back with a
                // slightly smaller bulge, filled.
                let (bx1, by1) = (origin_x + x1, flip(*y1));
                let (bx2, by2) = (origin_x + x2, flip(*y2));
                // Layout bulge is y-down positive; canvas is y-up.
                let b_outer = -bulge;
                let b_inner = b_outer * 0.72;
                let cx_a = bx1 + (bx2 - bx1) * 0.3;
                let cx_b = bx1 + (bx2 - bx1) * 0.7;
                ctx.begin_path();
                ctx.move_to(bx1, by1);
                ctx.cubic_to(cx_a, by1 + b_outer, cx_b, by2 + b_outer, bx2, by2);
                ctx.cubic_to(cx_b, by2 + b_inner, cx_a, by1 + b_inner, bx1, by1);
                ctx.close_path();
                ctx.fill();
            }
        }
    }
}
