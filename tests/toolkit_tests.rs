//! Toolkit acceptance tests for the v0 subset: import validation, element
//! ids/timemap, layout geometry invariants, and the render smoke test
//! against agg-gui's software rasterizer.

use verovio_rust::{ElementKind, ImportError, LayoutOptions, Toolkit};

fn simple_xml(notes: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>2</divisions>
        <key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>{notes}
    </measure>
  </part>
</score-partwise>"#
    )
}

const FOUR_QUARTERS: &str = r#"
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>"#;

#[test]
fn loads_the_subset() {
    let mut toolkit = Toolkit::new();
    toolkit
        .load_music_xml(&simple_xml(FOUR_QUARTERS))
        .expect("subset score loads");
    let score = toolkit.score().unwrap();
    assert_eq!(score.measures.len(), 1);
    assert_eq!(score.measures[0].voices[0].len(), 4);
    assert_eq!(score.staves, 1);
}

#[test]
fn rejects_multiple_parts() {
    let xml = r#"<?xml version="1.0"?>
<score-partwise><part-list/><part id="P1"><measure number="1"/></part><part id="P2"><measure number="1"/></part></score-partwise>"#;
    let mut toolkit = Toolkit::new();
    assert_eq!(
        toolkit.load_music_xml(xml),
        Err(ImportError::MultipleParts)
    );
}

#[test]
fn rejects_tuplets_specifically() {
    let xml = simple_xml(
        r#"
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration>
        <time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>
      </note>"#,
    );
    let mut toolkit = Toolkit::new();
    match toolkit.load_music_xml(&xml) {
        Err(ImportError::Unsupported(what)) => assert!(what.contains("tuplet"), "{what}"),
        other => panic!("expected tuplet rejection, got {other:?}"),
    }
}

#[test]
fn note_ids_are_stable_document_order() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&simple_xml(FOUR_QUARTERS)).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let ids: Vec<&str> = layout
        .timemap
        .iter()
        .flat_map(|m| m.note_ids.iter().map(|s| s.as_str()))
        .collect();
    assert_eq!(ids, ["note-0", "note-1", "note-2", "note-3"]);
    assert_eq!(
        layout.timemap.iter().map(|m| m.onset_units).collect::<Vec<_>>(),
        [0, 2, 4, 6]
    );
}

#[test]
fn note_midi_reports_the_encoded_pitch() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&simple_xml(FOUR_QUARTERS)).unwrap();
    assert_eq!(toolkit.note_midi("note-0"), Some(60));
    assert_eq!(toolkit.note_midi("note-3"), Some(65));
    assert_eq!(toolkit.note_midi("note-99"), None);
}

#[test]
fn noteheads_have_queryable_bounds_in_reading_order() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&simple_xml(FOUR_QUARTERS)).unwrap();
    toolkit.layout(&LayoutOptions::default());
    let mut last_x = f64::NEG_INFINITY;
    for id in ["note-0", "note-1", "note-2", "note-3"] {
        let (x, _y, w, h) = toolkit.element_bounds(id).expect("note bounds exist");
        assert!(x > last_x, "{id} advances rightward");
        assert!(w > 0.0 && h > 0.0);
        last_x = x;
    }
    // C4 sits below D4 on the staff (y-down layout space: larger y).
    let c4 = toolkit.element_bounds("note-0").unwrap();
    let d4 = toolkit.element_bounds("note-1").unwrap();
    assert!(c4.1 > d4.1, "C4 renders lower than D4");
}

#[test]
fn grand_staff_gets_brace_and_two_staves() {
    let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"/></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>2</divisions>
        <key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <staves>2</staves>
        <clef number="1"><sign>G</sign><line>2</line></clef>
        <clef number="2"><sign>F</sign><line>4</line></clef>
      </attributes>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration><type>whole</type><staff>1</staff></note>
      <backup><duration>8</duration></backup>
      <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration><type>whole</type><staff>2</staff></note>
    </measure>
  </part>
</score-partwise>"#;
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(xml).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let braces = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Brace)
        .count();
    assert_eq!(braces, 1);
    let staff_lines = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::StaffLine)
        .count();
    assert_eq!(staff_lines, 10, "two five-line staves");
    // Grand-staff timemap merges the simultaneous onsets treble-first.
    assert_eq!(layout.timemap.len(), 1);
    assert_eq!(layout.timemap[0].note_ids, ["note-0", "note-1"]);
}

#[test]
fn eighth_pairs_beam_and_lone_eighths_flag() {
    let xml = simple_xml(
        r#"
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>eighth</type></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>1</duration><type>eighth</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>4</duration><type>half</type></note>"#,
    );
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&xml).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let beams = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Beam)
        .count();
    let flags = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Flag)
        .count();
    assert_eq!(beams, 1, "the adjacent eighth pair beams");
    assert_eq!(flags, 0, "beamed eighths carry no flags");
}

#[test]
fn ties_link_and_suppress_new_onsets() {
    let xml = simple_xml(
        r#"
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>4</duration><tie type="start"/><type>half</type></note>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>4</duration><tie type="stop"/><type>half</type></note>"#,
    );
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&xml).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let ties = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Tie)
        .count();
    assert_eq!(ties, 1);
    // The tied continuation is not a new onset.
    assert_eq!(layout.timemap.len(), 1);
    assert_eq!(layout.timemap[0].note_ids, ["note-0"]);
}

#[test]
fn key_signature_engraves_and_suppresses_in_key_accidentals() {
    // F#4 in G major (fifths=1): the signature shows the sharp; the note
    // itself carries no accidental glyph.
    let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"/></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>2</divisions>
        <key><fifths>1</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>
      <note><pitch><step>F</step><alter>1</alter><octave>4</octave></pitch><duration>8</duration><type>whole</type></note>
    </measure>
  </part>
</score-partwise>"#;
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(xml).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let signature_glyphs = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::KeySignature)
        .count();
    assert_eq!(signature_glyphs, 1, "one sharp in the G-major signature");
    let accidentals = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Accidental)
        .count();
    assert_eq!(accidentals, 0, "in-key F# needs no accidental glyph");
}

#[test]
fn renders_through_software_gfx_ctx() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&simple_xml(FOUR_QUARTERS)).unwrap();
    let (width, height) = {
        let layout = toolkit.layout(&LayoutOptions::default());
        (layout.width.ceil() as usize, layout.height.ceil() as usize)
    };

    let mut framebuffer =
        agg_gui::framebuffer::Framebuffer::new(width as u32, height as u32);
    let mut ctx = agg_gui::gfx_ctx::GfxCtx::new(&mut framebuffer);
    agg_gui::draw_ctx::DrawCtx::clear(&mut ctx, agg_gui::color::Color::white());
    let font = verovio_rust::leipzig_font();
    toolkit.render(
        &mut ctx,
        &font,
        0.0,
        height as f64,
        &verovio_rust::RenderOptions::default(),
    );
    drop(ctx);

    // Ink landed: some pixels are no longer white.
    let inked = framebuffer
        .pixels()
        .chunks_exact(4)
        .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
        .count();
    assert!(
        inked > 100,
        "engraving should ink a meaningful number of pixels, got {inked}"
    );
}
