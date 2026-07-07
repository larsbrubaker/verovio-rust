//! # verovio-rust
//!
//! A Rust port of Verovio music engraving, rendering through agg-gui's
//! `DrawCtx` instead of SVG. Subset-first — see `docs/porting.md` for the
//! supported input subset and the porting rules, `docs/rendering.md` for
//! the DrawCtx/SMuFL mapping.
//!
//! The public shape mirrors the C++ `Toolkit` (`tools/main.cpp`,
//! `src/toolkit.cpp`): load, lay out, render, and query elements by id.
//!
//! LGPL-3.0-or-later, matching upstream Verovio.

mod import;
mod layout;
mod render;
pub mod score;
pub mod smufl;

use std::sync::Arc;

use agg_gui::text::Font;

pub use import::ImportError;
pub use layout::{
    ElementKind, LaidOutElement, Layout, LayoutOptions, Primitive, TimemapEntry,
};
pub use render::{render as render_layout, RenderOptions};
pub use score::Score;

/// Verovio's own SMuFL font (SIL OFL 1.1), pinned from the reference
/// submodule at `fonts/Leipzig/Leipzig.ttf`.
pub const LEIPZIG_FONT_BYTES: &[u8] = include_bytes!("../assets/Leipzig.ttf");

/// Load the bundled Leipzig music font.
pub fn leipzig_font() -> Arc<Font> {
    Arc::new(Font::from_slice(LEIPZIG_FONT_BYTES).expect("bundled Leipzig font parses"))
}

/// The engraving toolkit: load → layout → render / query.
#[derive(Default)]
pub struct Toolkit {
    score: Option<Score>,
    layout: Option<Layout>,
}

impl Toolkit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse and validate MusicXML (the v0 subset). Replaces any previously
    /// loaded score and invalidates the layout.
    pub fn load_music_xml(&mut self, xml: &str) -> Result<(), ImportError> {
        self.score = Some(import::parse_music_xml(xml.as_bytes())?);
        self.layout = None;
        Ok(())
    }

    pub fn score(&self) -> Option<&Score> {
        self.score.as_ref()
    }

    /// Engrave the loaded score. Returns the layout (also cached for
    /// rendering and queries).
    ///
    /// # Panics
    /// Panics if no score is loaded — mirroring the C++ toolkit, where
    /// rendering without a document is a programming error.
    pub fn layout(&mut self, options: &LayoutOptions) -> &Layout {
        let score = self
            .score
            .as_ref()
            .expect("load_music_xml before layout");
        let mut layout = layout::layout_score(score, options);
        layout.timemap = Self::build_timemap(score);
        self.layout = Some(layout);
        self.layout.as_ref().unwrap()
    }

    /// The current layout, if `layout` has run since the last load.
    pub fn current_layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// Paint the engraved score through agg-gui. `origin_y_top` is the y of
    /// the score box's top edge in the host's y-up coordinates.
    ///
    /// # Panics
    /// Panics if `layout` has not run.
    pub fn render(
        &self,
        ctx: &mut dyn agg_gui::draw_ctx::DrawCtx,
        font: &Arc<Font>,
        origin_x: f64,
        origin_y_top: f64,
        options: &RenderOptions,
    ) {
        let layout = self.layout.as_ref().expect("layout before render");
        render::render(ctx, font, layout, origin_x, origin_y_top, options);
    }

    /// Bounds of an element id in layout space (x, y_top_down, w, h).
    pub fn element_bounds(&self, id: &str) -> Option<(f64, f64, f64, f64)> {
        self.layout.as_ref()?.bounds_by_id.get(id).copied()
    }

    /// Timemap: every onset moment with its sounding note ids, document
    /// order (treble voice then bass) — Verovio's `renderToTimemap`
    /// reduced to onset units.
    fn build_timemap(score: &Score) -> Vec<TimemapEntry> {
        use std::collections::BTreeMap;
        let mut moments: BTreeMap<i32, (Vec<String>, Vec<String>)> = BTreeMap::new();
        for measure in &score.measures {
            for (voice_index, voice) in measure.voices.iter().enumerate() {
                for event in voice {
                    if event.is_rest() {
                        continue;
                    }
                    let slot = moments.entry(event.onset_units).or_default();
                    let ids: Vec<String> = event
                        .notes
                        .iter()
                        .filter(|n| !n.tie_stop) // continuations aren't new onsets
                        .map(|n| n.id.clone())
                        .collect();
                    if voice_index == 0 {
                        slot.0.extend(ids);
                    } else {
                        slot.1.extend(ids);
                    }
                }
            }
        }
        moments
            .into_iter()
            .filter(|(_, (treble, bass))| !treble.is_empty() || !bass.is_empty())
            .map(|(onset_units, (mut treble, bass))| {
                treble.extend(bass);
                TimemapEntry {
                    onset_units,
                    note_ids: treble,
                }
            })
            .collect()
    }
}
