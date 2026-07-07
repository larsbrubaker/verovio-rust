//! The engraving pass: positions every element of the score in **y-down
//! logical pixels** from the top-left of the score box. Rendering converts
//! to agg-gui's y-up world at draw time (`render.rs`).
//!
//! Ports the v0 slice of Verovio's layout (`src/view_element.cpp`,
//! `src/note.cpp`, `src/beam.cpp` for the pair rule, `src/keysig.cpp` for
//! signature positions). Simplifications are marked `// v0:` — most
//! notably linear duration spacing instead of Verovio's non-linear
//! spacing algorithm.

use std::collections::HashMap;

use crate::score::{Clef, Duration, Event, Pitch, Score};
use crate::smufl;

/// What an element is (feedback code keys off this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    StaffLine,
    LedgerLine,
    Barline,
    Brace,
    Clef,
    KeySignature,
    TimeSignature,
    Notehead,
    Stem,
    Flag,
    Beam,
    Dot,
    Accidental,
    Rest,
    Tie,
}

/// A positioned drawable.
#[derive(Debug, Clone)]
pub enum Primitive {
    /// Stroked segment with the given thickness.
    Line { x1: f64, y1: f64, x2: f64, y2: f64, thickness: f64 },
    /// A SMuFL glyph drawn at a baseline point.
    Glyph { ch: char, x: f64, y: f64, size: f64 },
    /// Filled tie/slur: a cubic arc from (x1,y1) to (x2,y2) bulging toward
    /// `bulge` pixels (positive = downward in y-down space), drawn as two
    /// stacked cubics for the classic tapered look.
    Tie { x1: f64, y1: f64, x2: f64, y2: f64, bulge: f64 },
}

/// One laid-out element. `id` is present for addressable elements
/// (noteheads, rests) and shared by a note's stem/flag/dot/accidental so a
/// recolor covers the whole note.
#[derive(Debug, Clone)]
pub struct LaidOutElement {
    pub kind: ElementKind,
    pub id: Option<String>,
    pub primitive: Primitive,
    /// Axis-aligned bounds in y-down layout space.
    pub bounds: (f64, f64, f64, f64),
}

/// A timemap moment: every note id sounding at an onset (document order —
/// treble voice then bass), mirroring Verovio's timemap API.
#[derive(Debug, Clone, PartialEq)]
pub struct TimemapEntry {
    pub onset_units: i32,
    pub note_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Distance between adjacent staff lines, px. Everything scales off this.
    pub staff_space: f64,
    /// Left/right/top margins around the system, px.
    pub margin: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            staff_space: 10.0,
            margin: 24.0,
        }
    }
}

/// The engraved result.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub elements: Vec<LaidOutElement>,
    pub timemap: Vec<TimemapEntry>,
    pub width: f64,
    pub height: f64,
    /// Notehead bounds by id (cursor placement, hit tests).
    pub bounds_by_id: HashMap<String, (f64, f64, f64, f64)>,
}

/// Staff-position bookkeeping for one staff of the system.
struct StaffGeometry {
    /// y of the top line.
    top: f64,
    space: f64,
    clef: Clef,
}

impl StaffGeometry {
    fn line_y(&self, line_from_top: i32) -> f64 {
        self.top + line_from_top as f64 * self.space
    }

    fn bottom(&self) -> f64 {
        self.line_y(4)
    }

    fn middle(&self) -> f64 {
        self.line_y(2)
    }

    /// y of a pitch: half a space per diatonic step from the bottom line's
    /// reference pitch (treble bottom line = E4, bass = G2 — see
    /// Verovio `src/clef.cpp` line positions).
    fn note_y(&self, pitch: Pitch) -> f64 {
        let bottom_ref = match self.clef {
            Clef::Treble => Pitch { step: 2, alter: 0, octave: 4 }, // E4
            Clef::Bass => Pitch { step: 4, alter: 0, octave: 2 },   // G2
        };
        let steps = pitch.diatonic_index() - bottom_ref.diatonic_index();
        self.bottom() - steps as f64 * self.space * 0.5
    }
}

/// Per-key accidental columns: (letter step 0=C…6=B, staff line position)
/// for sharps and flats in signature order, per clef. Values are
/// "half-space steps below the top line" (Verovio `src/keysig.cpp`).
fn signature_positions(clef: Clef, sharp: bool) -> [(u8, i32); 7] {
    match (clef, sharp) {
        // F# C# G# D# A# E# B#
        (Clef::Treble, true) => [(3, 0), (0, 3), (4, -1), (1, 2), (5, 5), (2, 1), (6, 4)],
        (Clef::Bass, true) => [(3, 2), (0, 5), (4, 1), (1, 4), (5, 7), (2, 3), (6, 6)],
        // Bb Eb Ab Db Gb Cb Fb
        (Clef::Treble, false) => [(6, 4), (2, 1), (5, 5), (1, 2), (4, 6), (0, 3), (3, 7)],
        (Clef::Bass, false) => [(6, 6), (2, 3), (5, 7), (1, 4), (4, 8), (0, 5), (3, 9)],
    }
}

/// Key-signature alteration for a step letter (sharp order F C G D A E B).
pub fn key_alteration(step: u8, fifths: i32) -> i32 {
    const SHARP_ORDER: [u8; 7] = [3, 0, 4, 1, 5, 2, 6];
    const FLAT_ORDER: [u8; 7] = [6, 2, 5, 1, 4, 0, 3];
    if fifths > 0 && SHARP_ORDER[..fifths.min(7) as usize].contains(&step) {
        return 1;
    }
    if fifths < 0 && FLAT_ORDER[..(-fifths).min(7) as usize].contains(&step) {
        return -1;
    }
    0
}

pub fn layout_score(score: &Score, options: &LayoutOptions) -> Layout {
    Engraver::new(score, options).run()
}

struct Engraver<'a> {
    score: &'a Score,
    sp: f64,
    margin: f64,
    staves: Vec<StaffGeometry>,
    out: Layout,
    glyph_size: f64,
}

impl<'a> Engraver<'a> {
    fn new(score: &'a Score, options: &LayoutOptions) -> Self {
        let sp = options.staff_space;
        let margin = options.margin;
        // Room for two ledger lines above the treble staff.
        let top = margin + 3.0 * sp;
        let mut staves = vec![StaffGeometry {
            top,
            space: sp,
            clef: Clef::Treble,
        }];
        if score.staves == 2 {
            // v0: fixed staff distance of 9 spaces between staff bottoms/tops.
            staves.push(StaffGeometry {
                top: top + 4.0 * sp + 9.0 * sp,
                space: sp,
                clef: Clef::Bass,
            });
        }
        Self {
            score,
            sp,
            margin,
            staves,
            out: Layout::default(),
            // SMuFL convention: 1 em = 4 staff spaces.
            glyph_size: 4.0 * sp,
        }
    }

    fn push(
        &mut self,
        kind: ElementKind,
        id: Option<String>,
        primitive: Primitive,
        bounds: (f64, f64, f64, f64),
    ) {
        if let Some(id) = &id {
            // First primitive of an id (the notehead / the rest glyph) owns
            // the queryable bounds.
            self.out.bounds_by_id.entry(id.clone()).or_insert(bounds);
        }
        self.out.elements.push(LaidOutElement {
            kind,
            id,
            primitive,
            bounds,
        });
    }

    fn glyph(&mut self, kind: ElementKind, id: Option<String>, ch: char, x: f64, y: f64) {
        // v0: bounds approximate the glyph cell from em fractions (accurate
        // metadata-driven boxes arrive with the font-metadata port).
        let w = self.glyph_width(ch);
        let h = self.glyph_size;
        let size = self.glyph_size;
        self.push(
            kind,
            id,
            Primitive::Glyph { ch, x, y, size },
            (x, y - h * 0.5, w, h),
        );
    }

    /// Advance width of a SMuFL glyph at the current size. Notehead-class
    /// glyphs in Leipzig advance ~1.18 staff spaces; clefs/rests wider.
    /// v0: fixed table (font-metadata port will replace it).
    fn glyph_width(&self, ch: char) -> f64 {
        let spaces = match ch {
            smufl::NOTEHEAD_WHOLE => 1.7,
            smufl::NOTEHEAD_HALF | smufl::NOTEHEAD_BLACK => 1.18,
            smufl::G_CLEF | smufl::F_CLEF => 2.6,
            smufl::ACCIDENTAL_SHARP => 1.0,
            smufl::ACCIDENTAL_FLAT => 0.9,
            smufl::ACCIDENTAL_NATURAL => 0.7,
            smufl::REST_WHOLE | smufl::REST_HALF => 1.3,
            smufl::REST_QUARTER => 1.1,
            smufl::REST_8TH => 1.0,
            smufl::AUGMENTATION_DOT => 0.4,
            _ => 1.8, // time-sig digits, brace column
        };
        spaces * self.sp
    }

    fn run(mut self) -> Layout {
        let mut x = self.margin;

        if self.score.staves == 2 {
            x += self.draw_brace(x);
        }
        let system_left = x;
        x += self.draw_clefs(x);
        x += self.draw_key_signature(x);
        x += self.draw_time_signature(x);
        x += self.sp; // breathing room before the first note

        let x = self.draw_measures(x);

        self.draw_staff_lines(system_left - if self.score.staves == 2 { 1.2 * self.sp } else { 0.0 }, x);
        self.finish(x)
    }

    fn finish(mut self, right: f64) -> Layout {
        self.out.width = right + self.margin;
        let bottom = self
            .staves
            .last()
            .map(|s| s.bottom())
            .unwrap_or(self.margin)
            // Room for two ledger lines below.
            + 3.0 * self.sp;
        self.out.height = bottom + self.margin;
        self.out
    }

    fn draw_staff_lines(&mut self, left: f64, right: f64) {
        let thickness = (self.sp * 0.08).max(1.0);
        for staff in 0..self.staves.len() {
            for line in 0..5 {
                let y = self.staves[staff].line_y(line);
                self.push(
                    ElementKind::StaffLine,
                    None,
                    Primitive::Line { x1: left, y1: y, x2: right, y2: y, thickness },
                    (left, y - thickness / 2.0, right - left, thickness),
                );
            }
        }
        // Opening barline connecting the system.
        let top = self.staves[0].top;
        let bottom = self.staves.last().unwrap().bottom();
        let bar_thickness = (self.sp * 0.12).max(1.0);
        self.push(
            ElementKind::Barline,
            None,
            Primitive::Line { x1: left, y1: top, x2: left, y2: bottom, thickness: bar_thickness },
            (left - bar_thickness / 2.0, top, bar_thickness, bottom - top),
        );
    }

    fn draw_brace(&mut self, x: f64) -> f64 {
        let top = self.staves[0].top;
        let bottom = self.staves[1].bottom();
        // The brace glyph spans one em per staff; scale to the system.
        let size = bottom - top;
        self.push(
            ElementKind::Brace,
            None,
            Primitive::Glyph { ch: smufl::BRACE, x, y: bottom, size },
            (x, top, 1.0 * self.sp, bottom - top),
        );
        1.6 * self.sp
    }

    fn draw_clefs(&mut self, x: f64) -> f64 {
        for i in 0..self.staves.len() {
            let (ch, line) = match self.staves[i].clef {
                Clef::Treble => (smufl::G_CLEF, 3), // G line, second from bottom
                Clef::Bass => (smufl::F_CLEF, 1),   // F line, second from top
            };
            let y = self.staves[i].line_y(line);
            self.glyph(ElementKind::Clef, None, ch, x + 0.3 * self.sp, y);
        }
        3.4 * self.sp
    }

    fn draw_key_signature(&mut self, x: f64) -> f64 {
        let fifths = self.score.fifths;
        if fifths == 0 {
            return 0.0;
        }
        let count = fifths.unsigned_abs() as usize;
        let sharp = fifths > 0;
        let ch = if sharp {
            smufl::ACCIDENTAL_SHARP
        } else {
            smufl::ACCIDENTAL_FLAT
        };
        let step_width = self.glyph_width(ch) + 0.1 * self.sp;
        for i in 0..self.staves.len() {
            let positions = signature_positions(self.staves[i].clef, sharp);
            for (k, (_step, half_steps_below_top)) in positions.iter().take(count).enumerate() {
                let y = self.staves[i].top + *half_steps_below_top as f64 * self.sp * 0.5;
                self.glyph(
                    ElementKind::KeySignature,
                    None,
                    ch,
                    x + k as f64 * step_width,
                    y,
                );
            }
        }
        count as f64 * step_width + 0.6 * self.sp
    }

    fn draw_time_signature(&mut self, x: f64) -> f64 {
        let beats = self.score.beats_per_measure;
        for i in 0..self.staves.len() {
            // Numerator sits on the second line from the top, denominator on
            // the fourth (SMuFL digits are centered on the baseline point).
            let num_y = self.staves[i].line_y(1);
            let den_y = self.staves[i].line_y(3);
            for (value, y) in [(beats as u32, num_y), (4u32, den_y)] {
                // v0 subset: beats ≤ 9 (single digit).
                self.glyph(
                    ElementKind::TimeSignature,
                    None,
                    smufl::time_sig_digit(value.min(9)),
                    x,
                    y,
                );
            }
        }
        2.4 * self.sp
    }

    fn draw_measures(&mut self, mut x: f64) -> f64 {
        let measures = self.score.measures.clone();
        for measure in &measures {
            x = self.draw_measure(measure, x);
            // Measure barline across the system.
            let top = self.staves[0].top;
            let bottom = self.staves.last().unwrap().bottom();
            let thickness = (self.sp * 0.12).max(1.0);
            self.push(
                ElementKind::Barline,
                None,
                Primitive::Line { x1: x, y1: top, x2: x, y2: bottom, thickness },
                (x - thickness / 2.0, top, thickness, bottom - top),
            );
            x += 0.4 * self.sp;
        }
        x
    }

    /// Engrave one measure: merge both voices' onsets into shared columns
    /// (chords across staves align), then place each voice's events.
    fn draw_measure(&mut self, measure: &crate::score::Measure, x_start: f64) -> f64 {
        // Column x per onset: linear duration spacing.
        // v0: Verovio uses non-linear spacing (`src/alignfunctor.cpp`);
        // linear reads fine for short exercises.
        let mut onsets: Vec<i32> = measure
            .voices
            .iter()
            .flat_map(|v| v.iter().map(|e| e.onset_units))
            .collect();
        onsets.sort_unstable();
        onsets.dedup();

        let mut column_x: HashMap<i32, f64> = HashMap::new();
        let mut x = x_start + 1.0 * self.sp;
        for (i, onset) in onsets.iter().enumerate() {
            if i > 0 {
                // Advance by the shortest sounding duration entering this
                // column boundary — approximated by the onset delta.
                let delta_units = onset - onsets[i - 1];
                x += (1.4 + 0.85 * delta_units as f64) * self.sp;
            }
            column_x.insert(*onset, x);
        }
        // Measure right edge: last column + its longest duration.
        let last_onset = onsets.last().copied().unwrap_or(0);
        let longest_last: i32 = measure
            .voices
            .iter()
            .flat_map(|v| v.iter())
            .filter(|e| e.onset_units == last_onset)
            .map(|e| e.duration.total_units())
            .max()
            .unwrap_or(2);
        let right = column_x.get(&last_onset).copied().unwrap_or(x_start)
            + (1.4 + 0.85 * longest_last as f64) * self.sp;

        for (staff_index, voice) in measure.voices.iter().enumerate() {
            if staff_index >= self.staves.len() {
                continue;
            }
            self.draw_voice(voice, staff_index, &column_x);
        }
        right
    }

    fn draw_voice(&mut self, voice: &[Event], staff_index: usize, column_x: &HashMap<i32, f64>) {
        let mut i = 0;
        while i < voice.len() {
            let event = &voice[i];
            let x = column_x[&event.onset_units];
            if event.is_rest() {
                self.draw_rest(event, staff_index, x);
                i += 1;
                continue;
            }
            // Simple pair beaming (`src/beam.cpp` reduced to the exercise
            // rule): two adjacent eighths beam when the first starts the
            // pair (even eighth index).
            let beam_pair = event.duration.base_units == 1
                && !event.duration.dotted
                && event.onset_units % 2 == 0
                && i + 1 < voice.len()
                && !voice[i + 1].is_rest()
                && voice[i + 1].duration.base_units == 1
                && !voice[i + 1].duration.dotted
                && voice[i + 1].onset_units == event.onset_units + 1;
            if beam_pair {
                let next_x = column_x[&voice[i + 1].onset_units];
                self.draw_beamed_pair(event, &voice[i + 1], staff_index, x, next_x);
                i += 2;
            } else {
                self.draw_event(event, staff_index, x, true);
                i += 1;
            }
        }
        self.draw_voice_ties(voice, staff_index, column_x);
    }

    fn draw_rest(&mut self, event: &Event, staff_index: usize, x: f64) {
        let ch = match (event.duration.base_units, event.duration.dotted) {
            (1, _) => smufl::REST_8TH,
            (2, _) => smufl::REST_QUARTER,
            (4, _) => smufl::REST_HALF,
            _ => smufl::REST_WHOLE,
        };
        // Whole rests hang under the 2nd line; half rests sit on the middle
        // line; others center on the middle line.
        let y = match ch {
            smufl::REST_WHOLE => self.staves[staff_index].line_y(1),
            _ => self.staves[staff_index].middle(),
        };
        let id = event.rest_id.clone();
        self.glyph(ElementKind::Rest, id.clone(), ch, x, y);
        if event.duration.dotted {
            let w = self.glyph_width(ch);
            self.glyph(
                ElementKind::Dot,
                id,
                smufl::AUGMENTATION_DOT,
                x + w + 0.3 * self.sp,
                y - 0.5 * self.sp,
            );
        }
    }

    /// Notehead glyph for a duration.
    fn notehead_char(duration: Duration) -> char {
        match duration.base_units {
            8 => smufl::NOTEHEAD_WHOLE,
            4 => smufl::NOTEHEAD_HALF,
            _ => smufl::NOTEHEAD_BLACK,
        }
    }

    /// Engrave one note/chord event; returns (stem_x, stem_tip_y, stem_up)
    /// so beaming can attach.
    fn draw_event(
        &mut self,
        event: &Event,
        staff_index: usize,
        x: f64,
        with_flag: bool,
    ) -> (f64, f64, bool) {
        let staff_middle = self.staves[staff_index].middle();
        let head = Self::notehead_char(event.duration);
        let head_w = self.glyph_width(head);

        // Stem direction from the note farthest from the middle line
        // (Verovio `src/note.cpp::CalcStemDirection`).
        let ys: Vec<f64> = event
            .notes
            .iter()
            .map(|n| self.staves[staff_index].note_y(n.pitch))
            .collect();
        let farthest = ys
            .iter()
            .cloned()
            .max_by(|a, b| {
                (a - staff_middle)
                    .abs()
                    .partial_cmp(&(b - staff_middle).abs())
                    .unwrap()
            })
            .unwrap_or(staff_middle);
        let stem_up = farthest > staff_middle;

        // Ledger lines, noteheads, dots, accidentals.
        for (note, &y) in event.notes.iter().zip(&ys) {
            self.draw_ledger_lines(staff_index, x, y, head_w);
            self.glyph(ElementKind::Notehead, Some(note.id.clone()), head, x, y);
            if event.duration.dotted {
                self.glyph(
                    ElementKind::Dot,
                    Some(note.id.clone()),
                    smufl::AUGMENTATION_DOT,
                    x + head_w + 0.35 * self.sp,
                    y - 0.5 * self.sp,
                );
            }
            self.draw_accidental(note, staff_index, x, y);
        }

        // Stem + flag (whole notes have none).
        let mut stem_x = x;
        let mut tip_y = farthest;
        if event.duration.base_units < 8 {
            let top_y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
            let bottom_y = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let stem_len = 3.5 * self.sp;
            let thickness = (self.sp * 0.12).max(1.0);
            let (x1, y1, y2) = if stem_up {
                (x + head_w - thickness / 2.0, bottom_y, top_y - stem_len)
            } else {
                (x + thickness / 2.0, top_y, bottom_y + stem_len)
            };
            stem_x = x1;
            tip_y = y2;
            let note_id = event.notes.first().map(|n| n.id.clone());
            self.push(
                ElementKind::Stem,
                note_id.clone(),
                Primitive::Line { x1, y1, x2: x1, y2, thickness },
                (x1 - thickness / 2.0, y1.min(y2), thickness, (y2 - y1).abs()),
            );
            if with_flag && event.duration.base_units == 1 {
                let flag = if stem_up {
                    smufl::FLAG_8TH_UP
                } else {
                    smufl::FLAG_8TH_DOWN
                };
                self.glyph(ElementKind::Flag, note_id, flag, x1, y2);
            }
        }
        (stem_x, tip_y, stem_up)
    }

    fn draw_beamed_pair(&mut self, a: &Event, b: &Event, staff_index: usize, xa: f64, xb: f64) {
        let (sxa, tya, up_a) = self.draw_event(a, staff_index, xa, false);
        let (sxb, tyb, _up_b) = self.draw_event(b, staff_index, xb, false);
        // v0: the pair beams at the first stem's direction; a level beam at
        // the more extreme tip keeps both stems >= minimum length.
        let beam_y = if up_a { tya.min(tyb) } else { tya.max(tyb) };
        let thickness = 0.5 * self.sp;
        let id = a.notes.first().map(|n| n.id.clone());
        self.push(
            ElementKind::Beam,
            id,
            Primitive::Line { x1: sxa, y1: beam_y, x2: sxb, y2: beam_y, thickness },
            (sxa, beam_y - thickness / 2.0, sxb - sxa, thickness),
        );
    }

    fn draw_ledger_lines(&mut self, staff_index: usize, x: f64, y: f64, head_w: f64) {
        let staff = &self.staves[staff_index];
        let top = staff.top;
        let bottom = staff.bottom();
        let thickness = (self.sp * 0.1).max(1.0);
        let overhang = 0.35 * self.sp;
        let mut line_y = top - self.sp;
        while line_y > y - 0.25 * self.sp {
            self.push(
                ElementKind::LedgerLine,
                None,
                Primitive::Line {
                    x1: x - overhang,
                    y1: line_y,
                    x2: x + head_w + overhang,
                    y2: line_y,
                    thickness,
                },
                (x - overhang, line_y - thickness / 2.0, head_w + 2.0 * overhang, thickness),
            );
            line_y -= self.sp;
        }
        let mut line_y = bottom + self.sp;
        while line_y < y + 0.25 * self.sp {
            self.push(
                ElementKind::LedgerLine,
                None,
                Primitive::Line {
                    x1: x - overhang,
                    y1: line_y,
                    x2: x + head_w + overhang,
                    y2: line_y,
                    thickness,
                },
                (x - overhang, line_y - thickness / 2.0, head_w + 2.0 * overhang, thickness),
            );
            line_y += self.sp;
        }
    }

    fn draw_accidental(&mut self, note: &crate::score::Note, staff_index: usize, x: f64, y: f64) {
        // A glyph appears when the encoding says so, or when the pitch
        // contradicts the key signature (ties never re-print — matches
        // the importer subset where tied notes carry no accidental).
        let needed = note.explicit_accidental
            || (!note.tie_stop && note.pitch.alter != key_alteration(note.pitch.step, self.score.fifths));
        if !needed {
            return;
        }
        let ch = match note.pitch.alter {
            -1 => smufl::ACCIDENTAL_FLAT,
            0 => smufl::ACCIDENTAL_NATURAL,
            _ => smufl::ACCIDENTAL_SHARP,
        };
        let w = self.glyph_width(ch);
        self.glyph(
            ElementKind::Accidental,
            Some(note.id.clone()),
            ch,
            x - w - 0.25 * self.sp,
            y,
        );
        let _ = staff_index;
    }

    fn draw_voice_ties(&mut self, voice: &[Event], staff_index: usize, column_x: &HashMap<i32, f64>) {
        for (i, event) in voice.iter().enumerate() {
            for note in &event.notes {
                if !note.tie_start {
                    continue;
                }
                // Find the same pitch in the next event (the tie stop).
                let Some(next) = voice.get(i + 1) else { continue };
                if !next
                    .notes
                    .iter()
                    .any(|n| n.tie_stop && n.pitch.midi() == note.pitch.midi())
                {
                    continue;
                }
                let y = self.staves[staff_index].note_y(note.pitch);
                let head_w = self.glyph_width(Self::notehead_char(event.duration));
                let x1 = column_x[&event.onset_units] + head_w + 0.15 * self.sp;
                let x2 = column_x[&next.onset_units] - 0.15 * self.sp;
                // Bulge away from the staff middle.
                let bulge = if y > self.staves[staff_index].middle() {
                    0.9 * self.sp
                } else {
                    -0.9 * self.sp
                };
                let y_edge = y + bulge.signum() * 0.55 * self.sp;
                self.push(
                    ElementKind::Tie,
                    Some(note.id.clone()),
                    Primitive::Tie { x1, y1: y_edge, x2, y2: y_edge, bulge },
                    (x1, y_edge.min(y_edge + bulge), x2 - x1, bulge.abs()),
                );
            }
        }
    }
}
