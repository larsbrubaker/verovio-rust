//! MusicXML import for the v0 subset (see `docs/porting.md`): single part,
//! 1–2 staves (one voice per staff), chords, ties, key signatures,
//! `beat-type` 4 meters, durations eighth…whole incl. dotted. Anything else
//! is rejected with a specific error — never silently mis-engraved.
//!
//! Ports the corresponding slice of Verovio's `src/iomusxml.cpp`, reduced
//! to the subset. Element ids are assigned in document order (`note-<n>`,
//! `rest-<n>`), which is the order Verovio emits ids for a timemap moment.

use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;

use crate::score::{Duration, Event, Measure, Note, Pitch, Score};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    NotMusicXml,
    MultipleParts,
    Unsupported(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotMusicXml => write!(f, "not a readable MusicXML file"),
            ImportError::MultipleParts => write!(f, "multi-part scores aren't supported"),
            ImportError::Unsupported(what) => write!(f, "unsupported for now: {what}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Minimal DOM node (quick-xml gives a pull parser; the importer logic
/// reads much more naturally over a tree, like the pugixml DOM upstream
/// Verovio traverses).
#[derive(Debug, Default)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn first(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn string_value(&self) -> String {
        let mut out = self.text.clone();
        for child in &self.children {
            out.push_str(&child.string_value());
        }
        out
    }

    fn int(&self, name: &str) -> Option<i32> {
        self.first(name)
            .and_then(|e| e.string_value().trim().parse().ok())
    }
}

fn parse_document(data: &[u8]) -> Option<Element> {
    let mut reader = Reader::from_reader(data);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(start)) => {
                let mut element = Element {
                    name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for attr in start.attributes().flatten() {
                    element.attributes.push((
                        String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&attr.value).into_owned(),
                    ));
                }
                stack.push(element);
            }
            Ok(XmlEvent::Empty(start)) => {
                let mut element = Element {
                    name: String::from_utf8_lossy(start.name().as_ref()).into_owned(),
                    ..Default::default()
                };
                for attr in start.attributes().flatten() {
                    element.attributes.push((
                        String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                        String::from_utf8_lossy(&attr.value).into_owned(),
                    ));
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            }
            Ok(XmlEvent::End(_)) => {
                let element = stack.pop()?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => {
                        root = Some(element);
                        break;
                    }
                }
            }
            Ok(XmlEvent::Text(text)) => {
                if let Some(current) = stack.last_mut() {
                    if let Ok(unescaped) = text.unescape() {
                        current.text.push_str(&unescaped);
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Ok(_) => {}
            Err(_) => return None,
        }
        buf.clear();
    }
    root
}

pub fn parse_music_xml(data: &[u8]) -> Result<Score, ImportError> {
    let root = parse_document(data).ok_or(ImportError::NotMusicXml)?;
    if root.name != "score-partwise" {
        return Err(ImportError::NotMusicXml);
    }

    let parts: Vec<&Element> = root.elements("part").collect();
    if parts.is_empty() {
        return Err(ImportError::NotMusicXml);
    }
    if parts.len() > 1 {
        return Err(ImportError::MultipleParts);
    }
    let part = parts[0];

    let title = first_text(&root, &["work", "work-title"])
        .or_else(|| first_text(&root, &["movement-title"]));

    let mut divisions = 0;
    let mut fifths = 0;
    let mut beats_per_measure = 4;
    let mut staves = 1usize;
    let mut score_measures: Vec<Measure> = Vec::new();
    // Cumulative onset per voice, in eighth units.
    let mut voice_units = [0i32; 2];
    let mut next_note_id = 0usize;
    let mut next_rest_id = 0usize;

    let measures: Vec<&Element> = part.elements("measure").collect();
    if measures.is_empty() {
        return Err(ImportError::NotMusicXml);
    }

    for measure in &measures {
        if let Some(attributes) = measure.first("attributes") {
            if let Some(d) = attributes.int("divisions") {
                divisions = d;
            }
            if let Some(s) = attributes.int("staves") {
                if !(1..=2).contains(&s) {
                    return Err(ImportError::Unsupported("more than two staves".into()));
                }
                staves = s as usize;
            }
            if let Some(key) = attributes.first("key") {
                if let Some(f) = key.int("fifths") {
                    if !(-7..=7).contains(&f) {
                        return Err(ImportError::Unsupported("key signature out of range".into()));
                    }
                    fifths = f;
                }
            }
            if let Some(time) = attributes.first("time") {
                if time.int("beat-type") != Some(4) {
                    return Err(ImportError::Unsupported("time signatures not over 4".into()));
                }
                beats_per_measure = time.int("beats").unwrap_or(4);
            }
            for clef in attributes.elements("clef") {
                let sign = clef.first("sign").map(|s| s.string_value());
                let sign = sign.as_deref().map(str::trim);
                if !(sign == Some("G") || (sign == Some("F") && staves == 2)) {
                    return Err(ImportError::Unsupported("clefs other than treble/bass".into()));
                }
            }
        }

        if staves == 1 && measure.first("backup").is_some() {
            return Err(ImportError::Unsupported("multiple voices on one staff".into()));
        }
        if measure.first("forward").is_some() {
            return Err(ImportError::Unsupported("forward skips (partial voices)".into()));
        }

        let mut out = Measure::default();
        for note_element in measure.elements("note") {
            if divisions <= 0 {
                return Err(ImportError::Unsupported("missing divisions".into()));
            }
            parse_note_into(
                note_element,
                divisions,
                &mut out,
                &mut voice_units,
                &mut next_note_id,
                &mut next_rest_id,
            )?;
        }
        score_measures.push(out);
    }

    Ok(Score {
        staves,
        fifths,
        beats_per_measure,
        measures: score_measures,
        title,
    })
}

fn parse_note_into(
    element: &Element,
    divisions: i32,
    measure: &mut Measure,
    voice_units: &mut [i32; 2],
    next_note_id: &mut usize,
    next_rest_id: &mut usize,
) -> Result<(), ImportError> {
    if element.first("grace").is_some() {
        return Err(ImportError::Unsupported("grace notes".into()));
    }
    if element.first("time-modification").is_some() {
        return Err(ImportError::Unsupported("tuplets".into()));
    }
    let is_chord_member = element.first("chord").is_some();
    let staff_index = match element.int("staff").unwrap_or(1) {
        1 => 0usize,
        2 => 1usize,
        _ => return Err(ImportError::Unsupported("more than two staves".into())),
    };
    let tie_stop = element
        .elements("tie")
        .any(|t| t.attribute("type") == Some("stop"));
    let tie_start = element
        .elements("tie")
        .any(|t| t.attribute("type") == Some("start"));
    let explicit_accidental = element.first("accidental").is_some();

    let Some(raw_duration) = element.int("duration") else {
        return Err(ImportError::Unsupported("notes without duration".into()));
    };
    let scaled = raw_duration * 2;
    if scaled % divisions != 0 {
        return Err(ImportError::Unsupported("note values outside eighth–whole".into()));
    }
    let Some(duration) = Duration::from_units(scaled / divisions) else {
        return Err(ImportError::Unsupported("note values outside eighth–whole".into()));
    };

    let voice = &mut measure.voices[staff_index];

    if element.first("rest").is_some() {
        let id = format!("rest-{}", *next_rest_id);
        *next_rest_id += 1;
        voice.push(Event {
            onset_units: voice_units[staff_index],
            duration,
            notes: Vec::new(),
            rest_id: Some(id),
        });
        voice_units[staff_index] += duration.total_units();
        return Ok(());
    }

    let pitch_el = element
        .first("pitch")
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let step_text = pitch_el
        .first("step")
        .map(|s| s.string_value().trim().to_string())
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let step = ["C", "D", "E", "F", "G", "A", "B"]
        .iter()
        .position(|&s| s == step_text)
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))? as u8;
    let octave = pitch_el
        .int("octave")
        .ok_or_else(|| ImportError::Unsupported("unreadable pitch".into()))?;
    let alter = pitch_el.int("alter").unwrap_or(0);
    if !(-1..=1).contains(&alter) {
        return Err(ImportError::Unsupported("double accidentals".into()));
    }

    let note = Note {
        pitch: Pitch {
            step,
            alter,
            octave,
        },
        id: {
            let id = format!("note-{}", *next_note_id);
            *next_note_id += 1;
            id
        },
        tie_start,
        tie_stop,
        explicit_accidental,
    };

    if is_chord_member {
        let event = voice
            .last_mut()
            .ok_or_else(|| ImportError::Unsupported("chord member without anchor".into()))?;
        event.notes.push(note);
    } else {
        voice.push(Event {
            onset_units: voice_units[staff_index],
            duration,
            notes: vec![note],
            rest_id: None,
        });
        voice_units[staff_index] += duration.total_units();
    }
    Ok(())
}

fn first_text(element: &Element, path: &[&str]) -> Option<String> {
    let mut current = element;
    for name in path {
        current = current.first(name)?;
    }
    let text = current.string_value().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
