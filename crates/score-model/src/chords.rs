//! Turning notes that overlap freely (a MIDI channel, the staves and voices of a piano part) into
//! the single voice of a pitched track: a chord for each stretch of time in which the same pitches
//! sound, a note held over the next stretch being tied on. Nothing is shortened or dropped.

use crate::Note;

/// A note as an importer reads it: start and end in ticks, MIDI pitch, and whether it continues a
/// tie from before its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub pitch: u8,
    pub is_tie_end: bool,
}

/// A stretch of time as (start, length, notes); no notes is a rest.
pub type Segment = (u32, u32, Vec<Note>);

/// Cuts the time from 0 to the last end at every start and end of a span, and gives each piece as
/// (start, length, notes). A piece where nothing sounds has no notes (a rest). A note is tied in
/// every piece after its first, and in its first when it ends a tie. Notes are sorted by pitch,
/// one per pitch. Spans that end before they start are ignored.
pub fn pitched_segments(spans: &[Span]) -> Vec<Segment> {
    let spans: Vec<&Span> = spans.iter().filter(|s| s.end > s.start).collect();
    let mut cuts: Vec<u32> = spans.iter().flat_map(|s| [s.start, s.end]).collect();
    cuts.push(0);
    cuts.sort_unstable();
    cuts.dedup();

    let mut segments = Vec::with_capacity(cuts.len());
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let mut notes: Vec<Note> = Vec::new();
        for span in spans.iter().filter(|s| s.start <= from && s.end >= to) {
            let tied = span.start < from || span.is_tie_end;
            match notes.iter_mut().find(|n| n.key() == span.pitch) {
                // The same pitch twice at once sounds as one note, replayed if either is new.
                Some(existing) => existing.tied &= tied,
                None => notes.push(Note {
                    tied,
                    ..Note::pitched(span.pitch)
                }),
            }
        }
        notes.sort_by_key(Note::key);
        segments.push((from, to - from, notes));
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, end: u32, pitch: u8) -> Span {
        Span {
            start,
            end,
            pitch,
            is_tie_end: false,
        }
    }

    /// Start, length, and (pitch, tied) of each note.
    type Summary = Vec<(u32, u32, Vec<(u8, bool)>)>;

    fn summary(segments: &[Segment]) -> Summary {
        segments
            .iter()
            .map(|(s, l, notes)| (*s, *l, notes.iter().map(|n| (n.key(), n.tied)).collect()))
            .collect()
    }

    #[test]
    fn a_held_note_under_moving_notes_is_cut_and_tied_not_shortened() {
        // A half note C3 under two quarters E4, G4.
        let segments = pitched_segments(&[span(0, 64, 48), span(0, 32, 64), span(32, 64, 67)]);
        assert_eq!(
            summary(&segments),
            vec![
                (0, 32, vec![(48, false), (64, false)]),
                (32, 32, vec![(48, true), (67, false)])
            ]
        );
    }

    #[test]
    fn a_gap_becomes_a_rest_from_the_start() {
        let segments = pitched_segments(&[span(32, 64, 60)]);
        assert_eq!(
            summary(&segments),
            vec![(0, 32, vec![]), (32, 32, vec![(60, false)])]
        );
    }

    #[test]
    fn a_note_that_ends_a_tie_starts_tied_and_doubles_merge() {
        let mut tied = span(0, 32, 60);
        tied.is_tie_end = true;
        let segments = pitched_segments(&[tied, span(0, 32, 60), span(10, 5, 70)]);
        assert_eq!(summary(&segments), vec![(0, 32, vec![(60, false)])]);
        assert_eq!(
            summary(&pitched_segments(&[tied])),
            vec![(0, 32, vec![(60, true)])]
        );
    }
}
