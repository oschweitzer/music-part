use std::io::Write;

use score_model::{Clef, Instrument, Note, Score, Sound, Technique, GUITAR_STANDARD};

use super::*;

const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 3.1 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">"#;

/// A one-part score. `attributes` goes at the start of the first measure.
fn score_xml(part_name: &str, attributes: &str, measures: &[&str]) -> String {
    let body: String = measures
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let attrs = if i == 0 { attributes } else { "" };
            format!(r#"<measure number="{}">{attrs}{m}</measure>"#, i + 1)
        })
        .collect();
    format!(
        r#"{HEADER}
<score-partwise version="3.1">
  <work><work-title>Test piece</work-title></work>
  <part-list><score-part id="P1"><part-name>{part_name}</part-name></score-part></part-list>
  <part id="P1">{body}</part>
</score-partwise>"#
    )
}

const FOUR_FOUR: &str = "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>";

/// `FOUR_FOUR` with extra elements inside its `attributes`.
fn four_four_with(extra: &str) -> String {
    FOUR_FOUR.replace("</attributes>", &format!("{extra}</attributes>"))
}

fn pitched(step: &str, octave: i32, divisions: u32, extra: &str) -> String {
    format!(
        "<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>{divisions}</duration>{extra}</note>"
    )
}

fn rest(divisions: u32) -> String {
    format!("<note><rest/><duration>{divisions}</duration></note>")
}

fn import(xml: &str) -> Result<MusicXmlImport, MusicXmlImportError> {
    import_musicxml(xml.as_bytes())
}

fn ok(xml: &str) -> MusicXmlImport {
    let imported = import(xml).expect("import");
    assert_eq!(imported.score.validate(), Ok(()));
    imported
}

fn first_beats(score: &Score, measure: usize) -> &[Beat] {
    &score.tracks[0].measures[measure].voices[0].beats
}

fn placed(beat: &Beat) -> Vec<(u8, u8)> {
    beat.notes
        .iter()
        .map(|n| (n.key(), n.fret().unwrap_or(0)))
        .collect()
}

#[test]
fn four_quarter_notes_become_a_measure_of_a_guitar_track() {
    let notes: String = [("E", 4), ("B", 3), ("G", 3), ("D", 3)]
        .iter()
        .map(|&(step, octave)| pitched(step, octave, 2, ""))
        .collect();
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&notes]));

    let score = &imported.score;
    assert_eq!(score.title, "Test piece");
    assert_eq!(score.tracks.len(), 1);
    assert_eq!(score.tracks[0].name, "Guitar");
    assert_eq!(score.tracks[0].instrument, Instrument::Guitar);
    assert_eq!(score.tracks[0].tuning, GUITAR_STANDARD.to_vec());
    let beats = first_beats(score, 0);
    assert_eq!(beats.len(), 4);
    // Open strings 1 to 4.
    let strings: Vec<Vec<(u8, u8)>> = beats.iter().map(placed).collect();
    assert_eq!(
        strings,
        vec![vec![(1, 0)], vec![(2, 0)], vec![(3, 0)], vec![(4, 0)]]
    );
    assert_eq!(imported.skipped_notes, 0);
}

#[test]
fn the_time_signature_and_the_tempo_are_read() {
    let attributes = "<attributes><divisions>1</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes><direction><sound tempo=\"96\"/></direction>";
    let notes = [
        pitched("A", 3, 1, ""),
        pitched("A", 3, 1, ""),
        pitched("A", 3, 1, ""),
    ]
    .concat();
    let imported = ok(&score_xml("Guitar", attributes, &[&notes]));

    assert_eq!(imported.score.tempo_bpm, 96);
    let signature = imported.score.tracks[0].measures[0].time_signature;
    assert_eq!((signature.numerator, signature.denominator), (3, 4));
}

#[test]
fn a_metronome_mark_gives_the_tempo_when_there_is_no_sound_element() {
    let attributes = format!("{FOUR_FOUR}<direction><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>72</per-minute></metronome></direction-type></direction>");
    let notes = [rest(8)].concat();
    assert_eq!(
        ok(&score_xml("Guitar", &attributes, &[&notes]))
            .score
            .tempo_bpm,
        72
    );
    // No tempo at all: the default.
    assert_eq!(
        ok(&score_xml("Guitar", FOUR_FOUR, &[&notes]))
            .score
            .tempo_bpm,
        120
    );
}

#[test]
fn string_and_fret_are_kept_when_the_file_gives_them() {
    let technical =
        "<notations><technical><string>3</string><fret>7</fret></technical></notations>";
    // Written as a pitch that would otherwise land elsewhere.
    let notes = [pitched("D", 4, 8, technical)].concat();
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&notes]));
    assert_eq!(placed(&first_beats(&imported.score, 0)[0]), vec![(3, 7)]);
}

#[test]
fn a_chord_is_spread_over_distinct_strings() {
    let chord = [
        pitched("E", 2, 8, ""),
        pitched("B", 2, 8, "").replace("<note>", "<note><chord/>"),
        pitched("E", 3, 8, "").replace("<note>", "<note><chord/>"),
    ]
    .concat();
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&chord]));

    let beat = &first_beats(&imported.score, 0)[0];
    assert_eq!(beat.notes.len(), 3);
    let mut strings: Vec<u8> = beat.notes.iter().map(Note::key).collect();
    strings.dedup();
    assert_eq!(strings.len(), 3, "three different strings");
}

#[test]
fn a_tie_marks_the_second_note_as_tied() {
    let first = pitched("E", 4, 4, "<tie type=\"start\"/>");
    let second = pitched("E", 4, 4, "<tie type=\"stop\"/>");
    let imported = ok(&score_xml(
        "Guitar",
        FOUR_FOUR,
        &[&format!("{first}{second}")],
    ));

    let beats = first_beats(&imported.score, 0);
    assert!(!beats[0].notes[0].tied);
    assert!(beats[1].notes[0].tied);
}

#[test]
fn a_tied_note_across_a_barline_keeps_its_tie() {
    let a = pitched("E", 4, 8, "<tie type=\"start\"/>");
    let b = pitched("E", 4, 8, "<tie type=\"stop\"/>");
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&a, &b]));
    assert!(first_beats(&imported.score, 1)[0].notes[0].tied);
}

#[test]
fn rests_and_whole_measure_rests_fill_the_measure() {
    let measures = [
        format!("{}{}", pitched("E", 4, 2, ""), rest(6)),
        "<note><rest measure=\"yes\"/><duration>8</duration></note>".to_string(),
    ];
    let refs: Vec<&str> = measures.iter().map(String::as_str).collect();
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &refs));

    assert_eq!(first_beats(&imported.score, 0).len(), 2);
    assert!(first_beats(&imported.score, 1)
        .iter()
        .all(|b| b.notes.is_empty()));
}

#[test]
fn triplets_are_rounded_onto_the_grid_and_the_measure_stays_full() {
    // Divisions 3: a triplet of eighth notes is 1 division each, three of them make a beat.
    let attributes = "<attributes><divisions>3</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>";
    let triplet: String = (0..3).map(|_| pitched("G", 3, 1, "")).collect();
    let rest_of_measure = rest(9);
    let imported = ok(&score_xml(
        "Guitar",
        attributes,
        &[&format!("{triplet}{rest_of_measure}")],
    ));

    let total: u32 = first_beats(&imported.score, 0)
        .iter()
        .map(|b| b.duration.ticks())
        .sum();
    assert_eq!(total, 128);
    let sounding = first_beats(&imported.score, 0)
        .iter()
        .filter(|b| !b.notes.is_empty())
        .count();
    assert!(sounding >= 3, "{sounding} beats with notes");
}

#[test]
fn backup_makes_a_second_voice() {
    let first = [pitched("E", 4, 8, "<voice>1</voice>")].concat();
    let backup = "<backup><duration>8</duration></backup>";
    let second = [
        pitched("G", 3, 4, "<voice>2</voice>"),
        pitched("G", 3, 4, "<voice>2</voice>"),
    ]
    .concat();
    let imported = ok(&score_xml(
        "Guitar",
        FOUR_FOUR,
        &[&format!("{first}{backup}{second}")],
    ));

    let measure = &imported.score.tracks[0].measures[0];
    assert_eq!(measure.voices.len(), 2);
    assert_eq!(measure.voices[0].beats.len(), 1);
    assert_eq!(measure.voices[1].beats.len(), 2);
}

#[test]
fn a_tab_staff_is_read_alone_so_notes_are_not_doubled() {
    let attributes = "<attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>TAB</sign><line>5</line></clef></attributes>";
    let staff1 = format!(
        "{}{}",
        pitched("E", 4, 8, "<staff>1</staff>"),
        "<backup><duration>8</duration></backup>"
    );
    let tab = pitched(
        "E",
        4,
        8,
        "<staff>2</staff><notations><technical><string>2</string><fret>5</fret></technical></notations>",
    );
    let imported = ok(&score_xml(
        "Guitar",
        attributes,
        &[&format!("{staff1}{tab}")],
    ));

    let measure = &imported.score.tracks[0].measures[0];
    assert_eq!(
        measure.voices.len(),
        1,
        "only the tablature staff is imported"
    );
    assert_eq!(placed(&measure.voices[0].beats[0]), vec![(2, 5)]);
}

#[test]
fn staff_tuning_gives_the_tuning_highest_string_first() {
    let details = "<staff-details><staff-lines>6</staff-lines>\
        <staff-tuning line=\"1\"><tuning-step>D</tuning-step><tuning-octave>2</tuning-octave></staff-tuning>\
        <staff-tuning line=\"2\"><tuning-step>A</tuning-step><tuning-octave>2</tuning-octave></staff-tuning>\
        <staff-tuning line=\"3\"><tuning-step>D</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>\
        <staff-tuning line=\"4\"><tuning-step>G</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>\
        <staff-tuning line=\"5\"><tuning-step>B</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>\
        <staff-tuning line=\"6\"><tuning-step>E</tuning-step><tuning-octave>4</tuning-octave></staff-tuning>\
        </staff-details>";
    let attributes = four_four_with(details);
    let notes = [pitched("D", 2, 8, "")].concat();
    let imported = ok(&score_xml("Guitar", &attributes, &[&notes]));

    assert_eq!(
        imported.score.tracks[0].tuning,
        vec![64, 59, 55, 50, 45, 38]
    );
    // The low D is the open sixth string of this tuning.
    assert_eq!(placed(&first_beats(&imported.score, 0)[0]), vec![(6, 0)]);
}

#[test]
fn a_transposing_part_is_read_at_sounding_pitch() {
    // Guitar notation is written an octave above what sounds.
    let attributes = four_four_with(
        "<transpose><chromatic>0</chromatic><octave-change>-1</octave-change></transpose>",
    );
    let notes = [pitched("E", 3, 8, "")].concat();
    let imported = ok(&score_xml("Guitar", &attributes, &[&notes]));
    // E3 written sounds E2: the open sixth string.
    assert_eq!(placed(&first_beats(&imported.score, 0)[0]), vec![(6, 0)]);
}

#[test]
fn techniques_are_mapped() {
    let technical = |inner: &str| format!("<notations><technical>{inner}</technical></notations>");
    let notes = [
        pitched("E", 4, 2, &technical("<hammer-on type=\"stop\"/>")),
        pitched("E", 4, 2, &technical("<pull-off type=\"stop\"/>")),
        pitched(
            "E",
            4,
            2,
            &technical("<bend><bend-alter>2</bend-alter></bend>"),
        ),
        pitched("E", 4, 2, "<notations><slide type=\"start\"/></notations>"),
    ]
    .concat();
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&notes]));

    let techniques: Vec<Option<Technique>> = first_beats(&imported.score, 0)
        .iter()
        .map(|b| b.notes[0].technique)
        .collect();
    assert_eq!(
        techniques,
        vec![
            Some(Technique::HammerOn),
            Some(Technique::PullOff),
            Some(Technique::Bend),
            Some(Technique::Slide)
        ]
    );
}

#[test]
fn grace_notes_and_notes_no_string_can_play_are_counted_not_imported() {
    let grace = "<note><grace/><pitch><step>E</step><octave>4</octave></pitch></note>";
    let too_low = pitched("C", 1, 2, "");
    let notes = format!("{grace}{too_low}{}{}", pitched("E", 4, 2, ""), rest(4));
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&notes]));

    assert_eq!(imported.skipped_notes, 2);
    assert_eq!(imported.score.tracks[0].measures.len(), 1);
}

#[test]
fn a_percussion_part_becomes_a_drum_track() {
    let drums = r#"<part id="P2"><measure number="1"><attributes><divisions>2</divisions></attributes><note><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>8</duration></note></measure></part>"#;
    let xml = score_xml("Guitar", FOUR_FOUR, &[&pitched("E", 4, 8, "")])
        .replace(
            "</part-list>",
            "<score-part id=\"P2\"><part-name>Drums</part-name></score-part></part-list>",
        )
        .replace("</score-partwise>", &format!("{drums}</score-partwise>"));
    let imported = ok(&xml);
    let [_, drums] = &imported.score.tracks[..] else {
        panic!("two tracks expected")
    };
    assert_eq!(
        (drums.instrument, drums.sound),
        (Instrument::Drums, Sound::DRUMS_STANDARD)
    );
    // Written on the third line with a plain head: the snare.
    assert_eq!(
        drums.measures[0].voices[0].beats[0].notes,
        vec![Note::percussion(38)]
    );
}

#[test]
fn drum_notes_of_two_voices_take_their_piece_from_their_instrument() {
    // Hi-hat eighths in voice 1 and the kick on beats 1 and 3 in voice 2, on the power kit, as
    // notation programs write a drum part.
    let part_list = r#"<score-part id="P1"><part-name>Drum Set</part-name>
        <score-instrument id="P1-X1"><instrument-name>Hat</instrument-name></score-instrument>
        <score-instrument id="P1-X2"><instrument-name>Kick</instrument-name></score-instrument>
        <midi-instrument id="P1-X1"><midi-channel>10</midi-channel><midi-program>17</midi-program><midi-unpitched>43</midi-unpitched></midi-instrument>
        <midi-instrument id="P1-X2"><midi-channel>10</midi-channel><midi-program>17</midi-program><midi-unpitched>37</midi-unpitched></midi-instrument>
        </score-part>"#;
    let hit = |instrument: &str, voice: u32, duration: u32| {
        format!(
            r#"<note><unpitched><display-step>A</display-step><display-octave>3</display-octave></unpitched><duration>{duration}</duration><instrument id="{instrument}"/><voice>{voice}</voice></note>"#
        )
    };
    let hats: String = (0..8).map(|_| hit("P1-X1", 1, 1)).collect();
    let kicks = [hit("P1-X2", 2, 4), hit("P1-X2", 2, 4)].concat();
    let notes = format!("{hats}<backup><duration>8</duration></backup>{kicks}");
    let xml = score_xml("x", FOUR_FOUR, &[&notes]).replace(
        r#"<score-part id="P1"><part-name>x</part-name></score-part>"#,
        part_list,
    );
    let imported = ok(&xml);
    assert_eq!(imported.skipped_notes, 0);
    let drums = &imported.score.tracks[0];
    assert_eq!(drums.sound, Sound::DRUMS_POWER);
    let beats = first_beats(&imported.score, 0);
    assert_eq!(beats.len(), 8);
    let keys: Vec<Vec<u8>> = beats
        .iter()
        .map(|b| b.notes.iter().map(Note::key).collect())
        .collect();
    assert_eq!(keys[0], vec![36, 42]);
    assert_eq!(keys[1], vec![42]);
    assert_eq!(keys[4], vec![36, 42]);
    assert!(beats.iter().all(|b| b.duration.ticks() == 16));
}

#[test]
fn an_empty_part_on_the_percussion_channel_is_a_drum_track() {
    let xml = score_xml("Kit", FOUR_FOUR, &[&rest(8)]).replace(
        "<part-name>Kit</part-name>",
        "<part-name>Kit</part-name><midi-instrument id=\"i1\"><midi-channel>10</midi-channel></midi-instrument>",
    );
    assert_eq!(ok(&xml).score.tracks[0].instrument, Instrument::Drums);
}

#[test]
fn pitched_notes_in_a_drum_part_and_drum_notes_in_a_pitched_part_are_counted() {
    let unpitched = r#"<note><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>4</duration></note>"#;
    let mixed = format!("{}{unpitched}", pitched("E", 4, 4, ""));
    let imported = ok(&score_xml("Guitar", FOUR_FOUR, &[&mixed]));
    assert_eq!(imported.score.tracks[0].instrument, Instrument::Guitar);
    assert_eq!(imported.skipped_notes, 1);
}

#[test]
fn instruments_come_from_the_midi_program_or_the_name() {
    let notes = [pitched("E", 2, 8, "")].concat();
    let bass = ok(&score_xml("Electric Bass", FOUR_FOUR, &[&notes]));
    assert_eq!(bass.score.tracks[0].instrument, Instrument::Bass);

    let violin = ok(&score_xml(
        "Violin I",
        FOUR_FOUR,
        &[&pitched("E", 5, 8, "")],
    ));
    assert_eq!(violin.score.tracks[0].instrument, Instrument::Violin);

    let with_program = score_xml("Part", FOUR_FOUR, &[&notes])
        .replace("<part-name>Part</part-name>", "<part-name>Part</part-name><midi-instrument id=\"i1\"><midi-program>34</midi-program></midi-instrument>");
    assert_eq!(
        ok(&with_program).score.tracks[0].instrument,
        Instrument::Bass
    );
}

#[test]
fn the_midi_program_picks_the_closest_sound() {
    let notes = [pitched("E", 3, 8, "")].concat();
    let with_program = |program: u8| {
        score_xml("Part", FOUR_FOUR, &[&notes]).replace(
            "<part-name>Part</part-name>",
            &format!("<part-name>Part</part-name><midi-instrument id=\"i1\"><midi-program>{program}</midi-program></midi-instrument>"),
        )
    };
    // MusicXML counts programs from 1: 31 is General MIDI program 30.
    assert_eq!(
        ok(&with_program(31)).score.tracks[0].sound,
        Sound::ELECTRIC_GUITAR_DISTORTION
    );
    assert_eq!(
        ok(&with_program(26)).score.tracks[0].sound,
        Sound::GUITAR_STEEL
    );
    assert_eq!(ok(&with_program(106)).score.tracks[0].sound, Sound::BANJO);
    // A piano is not a string instrument: the part name decides, else the part is a piano.
    assert_eq!(ok(&with_program(1)).score.tracks[0].sound, Sound::PIANO);
    let named_guitar = with_program(1).replace("<part-name>Part<", "<part-name>Guitar<");
    assert_eq!(ok(&named_guitar).score.tracks[0].sound, Sound::GUITAR_NYLON);
}

#[test]
fn keyboard_synth_and_voice_programs_win_over_the_part_name() {
    let notes = pitched("C", 4, 8, "");
    let with_program = |program: u8| {
        score_xml("Part", FOUR_FOUR, &[&notes]).replace(
            "<part-name>Part</part-name>",
            &format!("<part-name>Part</part-name><midi-instrument id=\"i1\"><midi-program>{program}</midi-program></midi-instrument>"),
        )
    };
    // Counted from 1 in the file.
    assert_eq!(
        ok(&with_program(5)).score.tracks[0].sound,
        Sound::PIANO_ELECTRIC
    );
    assert_eq!(
        ok(&with_program(20)).score.tracks[0].sound,
        Sound::ORGAN_CHURCH
    );
    assert_eq!(
        ok(&with_program(82)).score.tracks[0].sound,
        Sound::SYNTH_SAW_LEAD
    );
    assert_eq!(
        ok(&with_program(53)).score.tracks[0].sound,
        Sound::VOICE_AAHS
    );
}

#[test]
fn part_names_find_voices_organs_synthesizers_and_electric_pianos() {
    let notes = pitched("C", 4, 8, "");
    let sound_of = |name: &str| ok(&score_xml(name, FOUR_FOUR, &[&notes])).score.tracks[0].sound;
    for name in [
        "Voice",
        "Vocals",
        "Soprano",
        "Alto",
        "Tenor",
        "Bass Voice",
        "Baritone",
        "Choir",
        "Lead vocal",
    ] {
        assert_eq!(sound_of(name), Sound::VOICE_AAHS, "{name}");
    }
    assert_eq!(sound_of("Synth Voice"), Sound::VOICE_SYNTH);
    // "Bass" alone stays the bass guitar, and a saxophone is no singer.
    assert_eq!(sound_of("Bass"), Sound::BASS_FINGER);
    assert_eq!(sound_of("Alto Saxophone"), Sound::PIANO);
    assert_eq!(sound_of("Tenor Banjo"), Sound::BANJO);
    assert_eq!(sound_of("Organ"), Sound::ORGAN_DRAWBAR);
    assert_eq!(sound_of("Church Organ"), Sound::ORGAN_CHURCH);
    assert_eq!(sound_of("Accordion"), Sound::ORGAN_REED);
    assert_eq!(sound_of("Synthesizer"), Sound::SYNTH_SQUARE_LEAD);
    assert_eq!(sound_of("Synth Pad"), Sound::SYNTH_WARM_PAD);
    assert_eq!(sound_of("Synth Bass"), Sound::BASS_FINGER);
    assert_eq!(sound_of("Electric Piano"), Sound::PIANO_ELECTRIC);
    assert_eq!(sound_of("Piano"), Sound::PIANO);
}

#[test]
fn a_voice_takes_the_clef_of_the_file_else_the_clef_of_its_range() {
    let low = [pitched("C", 3, 4, ""), pitched("G", 2, 4, "")].concat();
    let clef_of = |xml: String| ok(&xml).score.tracks[0].clef;
    assert_eq!(
        clef_of(score_xml("Voice", FOUR_FOUR, &[&low])),
        Some(Clef::Bass)
    );
    let tenor = four_four_with(
        "<clef><sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change></clef>",
    );
    assert_eq!(
        clef_of(score_xml("Voice", &tenor, &[&low])),
        Some(Clef::Treble8vb)
    );
    let treble = four_four_with("<clef><sign>G</sign><line>2</line></clef>");
    assert_eq!(
        clef_of(score_xml("Voice", &treble, &[&low])),
        Some(Clef::Treble)
    );
    // Only a voice has a clef of its own.
    assert_eq!(clef_of(score_xml("Organ", &treble, &[&low])), None);
}

#[test]
fn a_part_with_no_string_instrument_becomes_a_piano_with_its_pitches() {
    let notes = [pitched("C", 2, 4, ""), pitched("E", 6, 4, "")].concat();
    let imported = ok(&score_xml("Flute", FOUR_FOUR, &[&notes]));
    let track = &imported.score.tracks[0];
    assert_eq!(track.instrument, Instrument::Piano);
    assert!(track.tuning.is_empty());
    assert_eq!(imported.skipped_notes, 0, "no string limits the range");
    let pitches: Vec<u8> = first_beats(&imported.score, 0)
        .iter()
        .flat_map(|b| b.notes.iter().map(Note::key))
        .collect();
    assert_eq!(pitches, vec![36, 88]);
}

#[test]
fn the_staves_of_a_piano_part_are_merged_into_one_voice_of_tied_chords() {
    // Right hand: two half notes E4 G4. Left hand, on staff 2: a whole note C3.
    let staff = |n: u32| format!("<voice>{n}</voice><staff>{n}</staff>");
    let measure = [
        pitched("E", 4, 4, &staff(1)),
        pitched("G", 4, 4, &staff(1)),
        "<backup><duration>8</duration></backup>".to_string(),
        pitched("C", 3, 8, &staff(2)),
    ]
    .concat();
    let imported = ok(&score_xml("Piano", FOUR_FOUR, &[&measure]));
    let track = &imported.score.tracks[0];
    assert_eq!(track.measures[0].voices.len(), 1);
    let chords: Vec<Vec<(u8, bool)>> = first_beats(&imported.score, 0)
        .iter()
        .map(|b| b.notes.iter().map(|n| (n.key(), n.tied)).collect())
        .collect();
    assert_eq!(
        chords,
        vec![
            vec![(48, false), (64, false)],
            vec![(48, true), (67, false)]
        ]
    );
}

#[test]
fn a_banjo_part_is_found_by_its_name_and_gets_five_strings() {
    let banjo = ok(&score_xml("Banjo", FOUR_FOUR, &[&pitched("G", 4, 8, "")]));
    let track = &banjo.score.tracks[0];
    assert_eq!(track.sound, Sound::BANJO);
    assert_eq!(track.tuning, score_model::BANJO_STANDARD);
}

#[test]
fn a_pickup_measure_keeps_its_own_length() {
    let pickup = [pitched("E", 4, 2, "")].concat();
    let full = [pitched("E", 4, 8, "")].concat();
    let xml = score_xml("Guitar", FOUR_FOUR, &[&pickup, &full]).replacen(
        "<measure number=\"1\">",
        "<measure number=\"0\" implicit=\"yes\">",
        1,
    );
    let imported = ok(&xml);

    let first = imported.score.tracks[0].measures[0].time_signature;
    assert_eq!((first.numerator, first.denominator), (1, 4));
    let second = imported.score.tracks[0].measures[1].time_signature;
    assert_eq!((second.numerator, second.denominator), (4, 4));
}

#[test]
fn parts_of_different_lengths_are_padded_to_the_longest() {
    let long = score_xml(
        "Guitar",
        FOUR_FOUR,
        &[&pitched("E", 4, 8, ""), &pitched("E", 4, 8, "")],
    );
    let short = r#"<part id="P2"><measure number="1"><attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><pitch><step>E</step><octave>2</octave></pitch><duration>8</duration></note></measure></part>"#;
    let xml = long
        .replace(
            "</part-list>",
            "<score-part id=\"P2\"><part-name>Bass</part-name></score-part></part-list>",
        )
        .replace("</score-partwise>", &format!("{short}</score-partwise>"));
    let imported = ok(&xml);

    assert_eq!(imported.score.tracks.len(), 2);
    assert_eq!(imported.score.tracks[1].measures.len(), 2);
}

#[test]
fn a_compressed_file_is_read_through_its_container() {
    let xml = score_xml("Guitar", FOUR_FOUR, &[&pitched("E", 4, 8, "")]);
    let container = r#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="score.xml"/></rootfiles></container>"#;

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("META-INF/container.xml", options)
            .unwrap();
        writer.write_all(container.as_bytes()).unwrap();
        writer.start_file("score.xml", options).unwrap();
        writer.write_all(xml.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    let imported = import_musicxml(&bytes).unwrap();
    assert_eq!(imported.score.title, "Test piece");
}

#[test]
fn files_that_are_not_scores_are_refused_with_an_error() {
    assert!(matches!(
        import("hello"),
        Err(MusicXmlImportError::Parse(_))
    ));
    assert_eq!(
        import("<html><body/></html>").err(),
        Some(MusicXmlImportError::NotMusicXml)
    );
    assert!(matches!(
        import("<score-timewise/>"),
        Err(MusicXmlImportError::Unsupported(_))
    ));
    assert_eq!(
        import("<score-partwise><part-list/></score-partwise>").err(),
        Some(MusicXmlImportError::NoPlayablePart)
    );
    assert!(matches!(
        import_musicxml(b"PK\x03\x04 not really"),
        Err(MusicXmlImportError::Parse(_))
    ));
    assert!(import_musicxml(&[]).is_err());
}

#[test]
fn a_utf16_file_is_decoded() {
    let xml = score_xml("Guitar", FOUR_FOUR, &[&pitched("E", 4, 8, "")]).replace("UTF-8", "UTF-16");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(import_musicxml(&bytes).unwrap().score.title, "Test piece");
}

#[test]
fn absurd_values_do_not_crash_or_allocate_without_bound() {
    let notes = format!(
        "<note><pitch><step>E</step><octave>4</octave></pitch><duration>99999999999999999999</duration></note>{}",
        "<note><pitch><step>H</step><octave>x</octave></pitch><duration>-5</duration></note>"
    );
    let attributes = "<attributes><divisions>0</divisions><time><beats>0</beats><beat-type>0</beat-type></time></attributes>";
    let imported = import(&score_xml("Guitar", attributes, &[&notes]));
    if let Ok(imported) = imported {
        assert_eq!(imported.score.validate(), Ok(()));
    }

    // Millions of measures are refused instead of built.
    let many = "<measure number=\"1\"><note><rest measure=\"yes\"/><duration>4</duration></note></measure>".repeat(5001);
    let xml = format!(
        "{HEADER}<score-partwise><part-list><score-part id=\"P1\"/></part-list><part id=\"P1\">{many}</part></score-partwise>"
    );
    assert_eq!(import(&xml).err(), Some(MusicXmlImportError::TooLong));
}

/// Whatever is thrown at the importer, it answers with an error or a valid score.
#[test]
fn damaged_files_never_panic() {
    let notes = [
        pitched("E", 4, 2, "<tie type=\"stop\"/>"),
        pitched(
            "G",
            3,
            2,
            "<notations><technical><string>2</string><fret>3</fret></technical></notations>",
        ),
        rest(4),
    ]
    .concat();
    let good = score_xml("Guitar", FOUR_FOUR, &[&notes, &notes]).into_bytes();

    let mut seed = 99u64;
    let mut next = move |below: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as usize) % below
    };
    for _ in 0..400 {
        let mut bytes = good.clone();
        match next(4) {
            0 => bytes.truncate(next(bytes.len())),
            1 => {
                for _ in 0..1 + next(8) {
                    let at = next(bytes.len());
                    bytes[at] = next(256) as u8;
                }
            }
            2 => {
                let at = next(bytes.len());
                let cut = (at + next(40)).min(bytes.len());
                bytes.drain(at..cut);
            }
            _ => {
                let at = next(bytes.len());
                bytes.splice(
                    at..at,
                    b"<note><duration>7</duration></note>".iter().copied(),
                );
            }
        }
        if let Ok(imported) = import_musicxml(&bytes) {
            assert_eq!(imported.score.validate(), Ok(()));
        }
    }
}
