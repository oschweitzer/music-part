//! The pieces of a drum kit: the General MIDI percussion keys 35 to 81, with the name the user
//! reads and where the piece is written on a percussion staff. The staff position and the head
//! shape are part of the model because the MusicXML export writes them (`display-step`,
//! `display-octave`, `notehead`) and its import reads them back when a file names no instrument.
//! `ui/src/lib/drums.ts` mirrors this table by hand (checked by `every_piece_is_listed_in_the_ui`).
//! See `docs/drums.md`.

use std::ops::RangeInclusive;

use NoteHead::{CircleCross, Cross, Diamond, Normal, Triangle};

/// The General MIDI percussion keys a drum note can play.
pub const DRUM_PIECES: RangeInclusive<u8> = 35..=81;

/// The shape of a note head on a percussion staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteHead {
    /// The usual oval: drums.
    Normal,
    /// An x: cymbals, hi-hats, the side stick.
    Cross,
    /// An x in a circle: the open hi-hat, open or muted hand percussion.
    CircleCross,
    Diamond,
    Triangle,
}

/// One piece of the kit and where it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumPiece {
    /// General MIDI percussion key, 35 to 81.
    pub key: u8,
    pub name: &'static str,
    /// The line or space it is written on, as the note of a treble staff there: `'C'` to `'B'`.
    pub step: char,
    pub octave: u8,
    pub head: NoteHead,
}

const fn piece(key: u8, name: &'static str, step: char, octave: u8, head: NoteHead) -> DrumPiece {
    DrumPiece {
        key,
        name,
        step,
        octave,
        head,
    }
}

/// Every piece, by key. The drum kit follows the usual drum set notation (kick in the bottom
/// space, snare on the third line, hi-hat above the staff, crash on the first ledger line);
/// hand percussion, which has no common place on a drum staff, takes the free combinations of
/// line and head. No two pieces share both.
pub const PIECES: [DrumPiece; 47] = [
    piece(35, "Acoustic kick", 'E', 4, Normal),
    piece(36, "Kick", 'F', 4, Normal),
    piece(37, "Side stick", 'C', 5, Cross),
    piece(38, "Snare", 'C', 5, Normal),
    piece(39, "Hand clap", 'B', 4, Triangle),
    piece(40, "Electric snare", 'C', 5, Diamond),
    piece(41, "Low floor tom", 'G', 4, Normal),
    piece(42, "Closed hi-hat", 'G', 5, Cross),
    piece(43, "High floor tom", 'A', 4, Normal),
    piece(44, "Pedal hi-hat", 'D', 4, Cross),
    piece(45, "Low tom", 'B', 4, Normal),
    piece(46, "Open hi-hat", 'G', 5, CircleCross),
    piece(47, "Low-mid tom", 'D', 5, Normal),
    piece(48, "High-mid tom", 'E', 5, Normal),
    piece(49, "Crash", 'A', 5, Cross),
    piece(50, "High tom", 'F', 5, Normal),
    piece(51, "Ride", 'F', 5, Cross),
    piece(52, "China", 'C', 6, Cross),
    piece(53, "Ride bell", 'F', 5, Diamond),
    piece(54, "Tambourine", 'D', 5, Triangle),
    piece(55, "Splash", 'A', 5, Diamond),
    piece(56, "Cowbell", 'E', 5, Triangle),
    piece(57, "Crash 2", 'B', 5, Cross),
    piece(58, "Vibraslap", 'F', 4, Triangle),
    piece(59, "Ride 2", 'E', 5, Cross),
    piece(60, "High bongo", 'E', 5, Diamond),
    piece(61, "Low bongo", 'D', 5, Diamond),
    piece(62, "Muted high conga", 'B', 4, CircleCross),
    piece(63, "Open high conga", 'B', 4, Diamond),
    piece(64, "Low conga", 'A', 4, Diamond),
    piece(65, "High timbale", 'G', 4, Diamond),
    piece(66, "Low timbale", 'F', 4, Diamond),
    piece(67, "High agogo", 'A', 5, Triangle),
    piece(68, "Low agogo", 'G', 5, Triangle),
    piece(69, "Cabasa", 'D', 5, Cross),
    piece(70, "Maracas", 'B', 4, Cross),
    piece(71, "Short whistle", 'B', 5, Triangle),
    piece(72, "Long whistle", 'C', 6, Triangle),
    piece(73, "Short guiro", 'D', 5, CircleCross),
    piece(74, "Long guiro", 'E', 5, CircleCross),
    piece(75, "Claves", 'G', 4, Triangle),
    piece(76, "High wood block", 'A', 4, Triangle),
    piece(77, "Low wood block", 'E', 4, Triangle),
    piece(78, "Muted cuica", 'F', 4, CircleCross),
    piece(79, "Open cuica", 'E', 4, CircleCross),
    piece(80, "Muted triangle", 'B', 5, Diamond),
    piece(81, "Open triangle", 'C', 6, Diamond),
];

/// The piece played by a General MIDI percussion key, if it is one (35 to 81).
pub fn drum_piece(key: u8) -> Option<&'static DrumPiece> {
    PIECES.get(usize::from(key.checked_sub(*DRUM_PIECES.start())?))
}

/// The piece written at a line or space with a head, or, when no piece has that head there, the
/// one the plain click of the staff would write: a normal head first, then a cross. For a file
/// that gives the place of an unpitched note but no instrument.
pub fn drum_piece_at(step: char, octave: u8, head: Option<NoteHead>) -> Option<u8> {
    let here = || {
        PIECES
            .iter()
            .filter(|p| p.step == step && p.octave == octave)
    };
    if let Some(head) = head {
        if let Some(found) = here().find(|p| p.head == head) {
            return Some(found.key);
        }
    }
    [Normal, Cross, CircleCross, Diamond, Triangle]
        .into_iter()
        .find_map(|head| here().find(|p| p.head == head))
        .map(|p| p.key)
}

/// True for the pieces played with a foot (the kicks and the pedal hi-hat): their stems point
/// down when a beat holds only them.
pub fn is_played_by_foot(key: u8) -> bool {
    matches!(key, 35 | 36 | 44)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lists_every_key_from_35_to_81_in_order() {
        let keys: Vec<u8> = PIECES.iter().map(|p| p.key).collect();
        assert_eq!(keys, DRUM_PIECES.collect::<Vec<u8>>());
        for key in DRUM_PIECES {
            assert_eq!(drum_piece(key).map(|p| p.key), Some(key));
        }
        assert_eq!(drum_piece(34), None);
        assert_eq!(drum_piece(82), None);
        assert_eq!(drum_piece(0), None);
    }

    #[test]
    fn no_two_pieces_share_a_place_and_a_head() {
        for (i, a) in PIECES.iter().enumerate() {
            for b in &PIECES[..i] {
                assert!(
                    (a.step, a.octave, a.head) != (b.step, b.octave, b.head),
                    "{} and {}",
                    a.name,
                    b.name
                );
            }
            assert!("CDEFGAB".contains(a.step), "{}", a.name);
        }
    }

    #[test]
    fn a_place_with_its_head_gives_back_its_piece() {
        for p in PIECES {
            assert_eq!(drum_piece_at(p.step, p.octave, Some(p.head)), Some(p.key));
        }
    }

    #[test]
    fn a_place_without_a_head_gives_the_drum_or_else_the_cymbal_written_there() {
        assert_eq!(drum_piece_at('C', 5, None), Some(38));
        assert_eq!(drum_piece_at('F', 4, None), Some(36));
        assert_eq!(drum_piece_at('G', 5, None), Some(42));
        assert_eq!(drum_piece_at('A', 5, None), Some(49));
        assert_eq!(drum_piece_at('C', 4, None), None);
    }

    /// `ui/src/lib/drums.ts` mirrors this table by hand: catch a piece it is missing or that
    /// differs.
    #[test]
    fn every_piece_is_listed_in_the_ui() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/src/lib/drums.ts");
        let ui = std::fs::read_to_string(path).expect("ui/src/lib/drums.ts is missing");
        for p in PIECES {
            let head = match p.head {
                Normal => "normal",
                Cross => "cross",
                CircleCross => "circleCross",
                Diamond => "diamond",
                Triangle => "triangle",
            };
            let row = format!(
                "{{ key: {}, name: '{}', step: '{}', octave: {}, head: '{head}' }}",
                p.key, p.name, p.step, p.octave
            );
            assert!(ui.contains(&row), "drums.ts lacks {row}");
        }
    }
}
