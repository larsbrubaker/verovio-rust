//! SMuFL glyph code points used by the v0 engraving subset, named per the
//! SMuFL specification (the same glyph set Verovio's `data/` tables map).
//! All glyphs live in the SMuFL private-use area and are present in the
//! bundled Leipzig font.

pub const BRACE: char = '\u{E000}';

pub const G_CLEF: char = '\u{E050}';
pub const F_CLEF: char = '\u{E062}';

/// Time-signature digits 0-9 (`timeSig0` … `timeSig9`).
pub fn time_sig_digit(digit: u32) -> char {
    debug_assert!(digit <= 9);
    char::from_u32(0xE080 + digit).expect("SMuFL timeSig digits are valid chars")
}

pub const NOTEHEAD_WHOLE: char = '\u{E0A2}';
pub const NOTEHEAD_HALF: char = '\u{E0A3}';
pub const NOTEHEAD_BLACK: char = '\u{E0A4}';

pub const ACCIDENTAL_FLAT: char = '\u{E260}';
pub const ACCIDENTAL_NATURAL: char = '\u{E261}';
pub const ACCIDENTAL_SHARP: char = '\u{E262}';

pub const FLAG_8TH_UP: char = '\u{E240}';
pub const FLAG_8TH_DOWN: char = '\u{E241}';

pub const REST_WHOLE: char = '\u{E4E3}';
pub const REST_HALF: char = '\u{E4E4}';
pub const REST_QUARTER: char = '\u{E4E5}';
pub const REST_8TH: char = '\u{E4E6}';

pub const AUGMENTATION_DOT: char = '\u{E1E7}';
