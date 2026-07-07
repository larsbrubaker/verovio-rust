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

/// Diagnostic: render to a PNG for eyeballing glyph extents
/// (`cargo test dump_render -- --ignored --nocapture`).
#[test]
#[ignore]
fn dump_render() {
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

    // Minimal PPM dump (framebuffer is RGBA, y-up rows).
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    let px = framebuffer.pixels();
    for y in (0..height).rev() {
        for x in 0..width {
            let i = (y * width + x) * 4;
            out.extend_from_slice(&px[i..i + 3]);
        }
    }
    let path = std::env::temp_dir().join("verovio_dump.ppm");
    std::fs::write(&path, out).unwrap();
    println!("wrote {}", path.display());
}

/// The LCD text rasterizer must size its mask from real glyph outline
/// extents, not the font's ascender/descender — SMuFL glyphs like the
/// treble clef deliberately overflow the em box and were cropped flat
/// on GPU backends (the "clipped clef" bug).
#[test]
fn lcd_mask_covers_full_clef_extent() {
    let font = verovio_rust::leipzig_font();
    let size = 40.0;
    let clef = "\u{E050}"; // gClef
    let (y_min, y_max) = font
        .glyph_visual_bounds('\u{E050}', size)
        .expect("clef outline");
    let mask = agg_gui::lcd_coverage::rasterize_text_lcd_cached(&font, clef, size);
    let ink_height = y_max - y_min;
    assert!(
        (mask.height as f64) >= ink_height,
        "mask height {} must cover the clef outline height {ink_height}",
        mask.height
    );
    // And the baseline must sit deep enough that the tail isn't cut.
    assert!(
        mask.baseline_y_in_mask >= -y_min,
        "baseline offset {} must clear the clef descender {}",
        mask.baseline_y_in_mask,
        -y_min
    );
}

/// Build a many-measure single-voice score (`count` measures of four
/// quarters each).
fn long_xml(count: usize) -> String {
    let mut measures = String::new();
    for number in 1..=count {
        let attributes = if number == 1 {
            r#"<attributes>
        <divisions>2</divisions>
        <key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>"#
        } else {
            ""
        };
        measures.push_str(&format!(
            r#"<measure number="{number}">{attributes}
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
    </measure>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
  <part id="P1">{measures}</part>
</score-partwise>"#
    )
}

/// `system_width` wraps measures into rows: the layout stays inside the
/// width, grows in height, keeps every note addressable, and puts later
/// measures on lower systems.
#[test]
fn system_width_wraps_long_scores_into_rows() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&long_xml(12)).unwrap();

    let single_height = toolkit.layout(&LayoutOptions::default()).height;
    let single_width = toolkit.layout(&LayoutOptions::default()).width;
    assert!(single_width > 800.0, "12 measures overflow one system");

    let options = LayoutOptions {
        system_width: Some(800.0),
        ..LayoutOptions::default()
    };
    let layout = toolkit.layout(&options);
    assert!(
        layout.width <= 800.0,
        "wrapped width {} stays inside the system width",
        layout.width
    );
    assert!(
        layout.height > single_height * 2.0,
        "wrapping stacks systems ({} vs single {single_height})",
        layout.height
    );

    // Every note keeps queryable bounds, and the last measure's notes sit
    // on a lower system than the first measure's.
    let ids: Vec<String> = layout
        .timemap
        .iter()
        .flat_map(|m| m.note_ids.clone())
        .collect();
    assert_eq!(ids.len(), 48);
    let first = layout.bounds_by_id[&ids[0]];
    let last = layout.bounds_by_id[ids.last().unwrap()];
    assert!(
        last.1 > first.1 + 100.0,
        "later measures wrap to lower rows ({} vs {})",
        last.1,
        first.1
    );

    // Clefs repeat on every system; the time signature only opens the first.
    let systems = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Clef)
        .count();
    assert!(systems > 1, "clef repeats per system");
    let time_sigs = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::TimeSignature)
        .count();
    assert_eq!(time_sigs, 2, "time signature (two digits) only on system 1");
}

/// Without `system_width` the engraving is byte-for-byte the single
/// endless system it always was.
#[test]
fn no_system_width_keeps_one_system() {
    let mut toolkit = Toolkit::new();
    toolkit.load_music_xml(&long_xml(12)).unwrap();
    let layout = toolkit.layout(&LayoutOptions::default());
    let clefs = layout
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Clef)
        .count();
    assert_eq!(clefs, 1, "single system, single clef");
}
