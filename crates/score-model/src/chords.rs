//! Turning notes that overlap freely (a MIDI channel, the staves and voices of a piano part) into
//! the single voice of a pitched track: a chord for each stretch of time in which the same pitches
//! sound, a note held over the next stretch being tied on. Nothing is shortened or dropped.

use crate::{Note, DRUM_PIECES};

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

/// Drum hits as the single voice of a drum track: the hits that start together are one beat,
/// which lasts until the next hit starts, and no longer than the longest of its hits or
/// `min_length`, whichever is longer; a rest fills the gap up to the next hit. A drum is struck,
/// not held, so how long a hit is written matters little: files often give a few ticks, and
/// `min_length` keeps those from turning into a 32nd note and a string of rests. Hits that
/// continue a tie are not struck again and are left out, as are keys that are no drum piece; the
/// second number counts those. The span's `pitch` is the key of the piece.
pub fn drum_segments(spans: &[Span], min_length: u32) -> (Vec<Segment>, usize) {
    let is_piece = |s: &&Span| DRUM_PIECES.contains(&s.pitch);
    let skipped = spans
        .iter()
        .filter(|s| !s.is_tie_end && !is_piece(s))
        .count();
    let mut hits: Vec<&Span> = spans
        .iter()
        .filter(|s| !s.is_tie_end && is_piece(s))
        .collect();
    hits.sort_by_key(|s| (s.start, s.pitch));

    let mut segments = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    while i < hits.len() {
        let start = hits[i].start;
        let group: Vec<&Span> = hits[i..]
            .iter()
            .take_while(|s| s.start == start)
            .copied()
            .collect();
        i += group.len();
        let written = group.iter().map(|s| s.end).max().unwrap_or(start);
        let longest = written.max(start.saturating_add(min_length.max(1)));
        let end = hits.get(i).map_or(longest, |next| longest.min(next.start));
        if start > cursor {
            segments.push((cursor, start - cursor, Vec::new()));
        }
        let mut notes: Vec<Note> = group.iter().map(|s| Note::percussion(s.pitch)).collect();
        notes.dedup_by_key(|n| n.key());
        segments.push((start, end - start, notes));
        cursor = end;
    }
    (segments, skipped)
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
    fn a_drum_hit_lasts_until_the_next_one_or_its_minimum_length() {
        // A kick and a hi-hat of a few ticks, a hi-hat an eighth later, then a crash alone.
        let (segments, skipped) = drum_segments(
            &[
                span(0, 2, 42),
                span(0, 2, 36),
                span(16, 18, 42),
                span(64, 66, 49),
            ],
            32,
        );
        assert_eq!(skipped, 0);
        assert_eq!(
            summary(&segments),
            vec![
                (0, 16, vec![(36, false), (42, false)]),
                (16, 32, vec![(42, false)]),
                (48, 16, vec![]),
                (64, 32, vec![(49, false)]),
            ]
        );
    }

    #[test]
    fn keys_that_are_no_drum_piece_are_counted_and_tied_hits_are_not_struck_again() {
        let mut tied = span(32, 64, 38);
        tied.is_tie_end = true;
        let (segments, skipped) = drum_segments(
            &[span(0, 32, 38), span(0, 32, 38), tied, span(0, 32, 20)],
            4,
        );
        assert_eq!(skipped, 1);
        assert_eq!(summary(&segments), vec![(0, 32, vec![(38, false)])]);
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
