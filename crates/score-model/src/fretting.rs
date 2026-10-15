//! Finding where to play pitches on a stringed instrument. Importers use it for formats that
//! store pitches rather than strings and frets, and the edits for a note written on the staff of
//! a fretted track.

use crate::{pitch_fret, Instrument, Note};

/// The string and fret that play `pitch` on a string not in `taken`, at the lowest fret, or
/// `None` if no free string can play it.
pub fn place_pitch(
    instrument: Instrument,
    tuning: &[u8],
    pitch: u8,
    taken: &[u8],
) -> Option<(u8, u8)> {
    (1..=tuning.len().min(usize::from(u8::MAX)) as u8)
        .filter(|string| !taken.contains(string))
        .filter_map(|string| Some((string, pitch_fret(instrument, tuning, string, pitch)?)))
        .min_by_key(|&(_, fret)| fret)
}

/// Places the pitches of a chord on distinct strings of an instrument, preferring low frets.
/// `taken` lists strings that are already used (for instance by notes whose string the file gave).
/// Returns the notes sorted by string, and how many pitches no free string could play.
pub fn place_chord(
    instrument: Instrument,
    tuning: &[u8],
    pitches: &[u8],
    taken: &[u8],
) -> (Vec<Note>, usize) {
    let mut pitches = pitches.to_vec();
    pitches.sort_unstable();
    pitches.dedup();

    let candidates = |pitch: u8| -> Vec<(u8, u8)> {
        (1..=tuning.len().min(usize::from(u8::MAX)) as u8)
            .filter_map(|string| {
                let fret = pitch_fret(instrument, tuning, string, pitch)?;
                Some((string, fret))
            })
            .collect()
    };

    // Notes with the fewest options first, so they are not starved by easier ones.
    let mut options: Vec<(u8, Vec<(u8, u8)>)> =
        pitches.iter().map(|&p| (p, candidates(p))).collect();
    options.sort_by_key(|(pitch, c)| (c.len(), *pitch));

    let mut used: Vec<u8> = taken.to_vec();
    let mut notes = Vec::new();
    let mut skipped = 0;
    for (_, choices) in options {
        let best = choices
            .into_iter()
            .filter(|(string, _)| !used.contains(string))
            .min_by_key(|&(_, fret)| fret);
        match best {
            Some((string, fret)) => {
                used.push(string);
                notes.push(Note::fretted(string, fret));
            }
            None => skipped += 1,
        }
    }
    notes.sort_by_key(Note::key);
    (notes, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BANJO_STANDARD, GUITAR_STANDARD};

    fn place_on_guitar(pitches: &[u8], taken: &[u8]) -> (Vec<Note>, usize) {
        place_chord(Instrument::Guitar, &GUITAR_STANDARD, pitches, taken)
    }

    #[test]
    fn a_chord_goes_on_distinct_strings_at_low_frets() {
        // E major open: E2 B2 E3 G#3 B3 E4.
        let (notes, skipped) = place_on_guitar(&[40, 47, 52, 56, 59, 64], &[]);
        assert_eq!(skipped, 0);
        let placed: Vec<(u8, u8)> = notes.iter().map(|n| (n.key(), n.fret().unwrap())).collect();
        assert_eq!(placed, vec![(1, 0), (2, 0), (3, 1), (4, 2), (5, 2), (6, 0)]);
    }

    #[test]
    fn a_pitch_no_string_can_play_is_counted() {
        let (notes, skipped) = place_on_guitar(&[20, 64], &[]);
        assert_eq!((notes.len(), skipped), (1, 1));
    }

    #[test]
    fn strings_already_taken_are_left_alone() {
        // E4 could be string 1 (open) but that string is taken: it goes on string 2, fret 5.
        let (notes, _) = place_on_guitar(&[64], &[1]);
        assert_eq!(notes[0].place, crate::Place::Fretted { string: 2, fret: 5 });
    }

    #[test]
    fn a_note_on_the_short_banjo_string_is_numbered_from_the_neck() {
        // A4 on the fifth string only, the others being taken: one semitone above G4 is fret 6.
        let (notes, _) = place_chord(Instrument::Banjo, &BANJO_STANDARD, &[69], &[1, 2, 3, 4]);
        assert_eq!(notes[0].place, crate::Place::Fretted { string: 5, fret: 7 });
    }

    #[test]
    fn a_single_pitch_goes_on_the_free_string_with_the_lowest_fret() {
        // A3 (57): string 3 fret 2 is lower than string 4 fret 7 or string 5 fret 12.
        assert_eq!(
            place_pitch(Instrument::Guitar, &GUITAR_STANDARD, 57, &[]),
            Some((3, 2))
        );
        assert_eq!(
            place_pitch(Instrument::Guitar, &GUITAR_STANDARD, 57, &[3]),
            Some((4, 7))
        );
        assert_eq!(
            place_pitch(Instrument::Guitar, &GUITAR_STANDARD, 30, &[]),
            None
        );
    }
}
