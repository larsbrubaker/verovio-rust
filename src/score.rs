//! The loaded score model the layout pass engraves: the validated result of
//! MusicXML import. Mirrors the slice of Verovio's object model
//! (`Score → Measure → Staff/Layer → Note/Rest/Chord`) the v0 subset needs.
//!
//! Pitches keep the **spelling from the source file** (step/alter/octave) —
//! engraving must never respell (`src/note.cpp` renders what the encoding
//! says; only the accidental *glyph* is key-signature dependent).

/// Duration of one event, in eighth-note units (1 = eighth … 8 = whole).
/// Dotted values carry the dot separately so the glyph choice stays simple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration {
    /// Base type in eighth units: 1 = eighth, 2 = quarter, 4 = half, 8 = whole.
    pub base_units: i32,
    pub dotted: bool,
}

impl Duration {
    /// Total length in eighth units (dot adds half the base).
    pub fn total_units(self) -> i32 {
        if self.dotted {
            self.base_units + self.base_units / 2
        } else {
            self.base_units
        }
    }

    /// Classify a raw eighth-unit length into base + dot, if representable
    /// in the subset (eighth … whole incl. dotted quarter/half).
    pub fn from_units(units: i32) -> Option<Duration> {
        match units {
            1 => Some(Duration { base_units: 1, dotted: false }),
            2 => Some(Duration { base_units: 2, dotted: false }),
            3 => Some(Duration { base_units: 2, dotted: true }),
            4 => Some(Duration { base_units: 4, dotted: false }),
            6 => Some(Duration { base_units: 4, dotted: true }),
            8 => Some(Duration { base_units: 8, dotted: false }),
            _ => None,
        }
    }
}

/// A spelled pitch, exactly as encoded in the source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pitch {
    /// Diatonic step letter 0=C … 6=B.
    pub step: u8,
    /// −1 flat, 0 natural, +1 sharp.
    pub alter: i32,
    /// Scientific octave (C4 = middle C).
    pub octave: i32,
}

impl Pitch {
    /// MIDI note number of this spelling.
    pub fn midi(self) -> u8 {
        let semitones = [0, 2, 4, 5, 7, 9, 11][self.step as usize] + self.alter;
        ((self.octave + 1) * 12 + semitones).clamp(0, 127) as u8
    }

    /// Absolute diatonic index (C-1 = 0) — one per staff position.
    pub fn diatonic_index(self) -> i32 {
        (self.octave + 1) * 7 + self.step as i32
    }
}

/// One note of an event (an event with several notes is a chord).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub pitch: Pitch,
    /// Stable element id, `note-<n>` in document order.
    pub id: String,
    /// This note is tied to the same pitch in the next event.
    pub tie_start: bool,
    /// This note continues a tie from the previous event.
    pub tie_stop: bool,
    /// An explicit accidental glyph was encoded (importer computes it from
    /// `<accidental>`; layout falls back to key-signature logic when absent).
    pub explicit_accidental: bool,
}

/// One rhythmic moment in a voice: a rest, a note, or a chord.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Onset in eighth units from the start of the score.
    pub onset_units: i32,
    pub duration: Duration,
    /// Empty = rest.
    pub notes: Vec<Note>,
    /// Rest id (rests are addressable too, like Verovio's `rest-<n>`).
    pub rest_id: Option<String>,
}

impl Event {
    pub fn is_rest(&self) -> bool {
        self.notes.is_empty()
    }
}

/// One measure: up to two voices (voice 0 = staff 1/treble, voice 1 =
/// staff 2/bass in the v0 subset's one-voice-per-staff world).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measure {
    pub voices: [Vec<Event>; 2],
}

/// Clef of a staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clef {
    Treble,
    Bass,
}

/// The validated score the layout pass consumes.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// 1 = single treble staff, 2 = grand staff.
    pub staves: usize,
    /// Key signature (−7 … +7).
    pub fifths: i32,
    /// Quarter-note beats per measure (`beat-type` is always 4 in the subset).
    pub beats_per_measure: i32,
    pub measures: Vec<Measure>,
    pub title: Option<String>,
}

impl Score {
    pub fn units_per_measure(&self) -> i32 {
        self.beats_per_measure * 2
    }
}
