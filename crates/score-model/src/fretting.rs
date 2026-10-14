//! Finding where to play pitches on a stringed instrument. Importers use it for formats that
//! store pitches rather than strings and frets.

use crate::{Note, MAX_FRET};

/// Places the pitches of a chord on distinct strings, preferring low frets. `taken` lists strings
/// that are already used (for instance by notes whose string the file gave). Returns the notes
/// sorted by string, and how many pitches no free string could play.
pub fn place_chord(tuning: &[u8], pitches: &[u8], taken: &[u8]) -> (Vec<Note>, usize) {
    let mut pitches = pitches.to_vec();
    pitches.sort_unstable();
    pitches.dedup();

    let candidates = |pitch: u8| -> Vec<(u8, u8)> {
        tuning
            .iter()
            .enumerate()
            .filter_map(|(i, &open)| {
                let fret = pitch.checked_sub(open)?;
                (fret <= MAX_FRET).then_some((i as u8 + 1, fret))
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
                notes.push(Note {
                    string,
                    fret,
                    technique: None,
                    tied: false,
                });
            }
            None => skipped += 1,
        }
    }
    notes.sort_by_key(|n| n.string);
    (notes, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GUITAR_STANDARD;

    #[test]
    fn a_chord_goes_on_distinct_strings_at_low_frets() {
        // E major open: E2 B2 E3 G#3 B3 E4.
        let (notes, skipped) = place_chord(&GUITAR_STANDARD, &[40, 47, 52, 56, 59, 64], &[]);
        assert_eq!(skipped, 0);
        let placed: Vec<(u8, u8)> = notes.iter().map(|n| (n.string, n.fret)).collect();
        assert_eq!(placed, vec![(1, 0), (2, 0), (3, 1), (4, 2), (5, 2), (6, 0)]);
    }

    #[test]
    fn a_pitch_no_string_can_play_is_counted() {
        let (notes, skipped) = place_chord(&GUITAR_STANDARD, &[20, 64], &[]);
        assert_eq!((notes.len(), skipped), (1, 1));
    }

    #[test]
    fn strings_already_taken_are_left_alone() {
        // E4 could be string 1 (open) but that string is taken: it goes on string 2, fret 5.
        let (notes, _) = place_chord(&GUITAR_STANDARD, &[64], &[1]);
        assert_eq!((notes[0].string, notes[0].fret), (2, 5));
    }
}
