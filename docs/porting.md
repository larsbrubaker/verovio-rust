# Porting Verovio to Rust

Upstream Verovio is ~200k lines of C++ engraving covering the full breadth
of MEI. This port grows **subset-first**: each release supports a documented
input subset completely (layout + rendering + element ids), rejecting
everything else with a specific error — never silently mis-engraving.

## The C++ reference

Pinned submodule `verovio-cpp-reference/` at `8d42439` (release 6.2.1).
Layout:

- `src/` — the engraving core (layout classes per element type)
- `include/vrv/` — headers / class docs
- `data/` — SMuFL support tables
- `fonts/` — SMuFL fonts (Leipzig is Verovio's own, OFL-licensed; bundled
  here at `assets/Leipzig.ttf`)
- `doc/` — the toolkit API documentation

When porting a layout rule (stem direction, accidental spacing, tie shape),
read the corresponding C++ first (`src/layer*.cpp`, `src/note.cpp`,
`src/beam.cpp`, …) and cite the source file in a comment. Where the v0
subset simplifies (e.g. linear spacing instead of Verovio's non-linear
spacing algorithm), say so explicitly at the simplification site.

## The v0 subset (current)

Input: MusicXML `score-partwise`, single part, 1–2 staves (grand staff),
one voice per staff, treble/bass clefs, key signatures −7…+7, `beat-type` 4
meters, durations eighth…whole incl. dotted, chords, tie start/stop,
rests, full measures. This matches the KeyInSight exercise subset.

Engraving: five-line staves, brace + connected barlines for grand staff,
G/F clefs, key/time signatures, noteheads with stems (direction by staff
middle-line rule), eighth-note flags and simple pair beaming, augmentation
dots, accidentals (sharp/flat/natural with key-signature awareness),
ledger lines, ties as bezier slurs, quarter/half/whole/eighth rests,
end barline.

API (mirrors the C++ `Toolkit` shape):

- `Toolkit::load_music_xml(&str)` — parse + validate the subset
- `Toolkit::layout(&LayoutOptions)` — engrave into positioned elements
- `Toolkit::render(ctx, &RenderOptions)` — paint via agg-gui `DrawCtx`
- element ids (`note-N`, stable in document order), bounds lookup,
  per-id color overrides, and the onset-units timemap — the per-note
  addressability KeyInSight's feedback loop needs

## Roadmap beyond v0

Beaming beyond pairs → multiple voices per staff → slurs/articulations →
tuplets → lyrics → MEI input → Verovio's non-linear spacing. Each step
ports the corresponding C++ behavior; nothing lands half-supported.
