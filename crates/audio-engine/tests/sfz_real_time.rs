//! The SFZ voices run inside the audio callback, which must not allocate (see `docs/sfz.md`).
//! This test binary counts the allocations made on the test's thread while notes start, stop,
//! bend and render.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::path::Path;

use audio_engine::sequencer::Timbre;
use audio_engine::sfz::{self, NoteOn, SfzSound, Voices};
use score_model::Sound;

struct CountingAllocator;

thread_local! {
    static IS_COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    // `try_with`: the thread-local may be gone while a thread shuts down.
    let _ = IS_COUNTING.try_with(|counting| {
        if counting.get() {
            let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        }
    });
}

// SAFETY: every call is forwarded unchanged to the system allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// A 16-bit mono WAV file of a 220 Hz sine.
fn sine_wav(frames: usize) -> Vec<u8> {
    let data_len = (frames * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    for value in [16u32, 1 | (1 << 16), 44_100, 88_200, 2 | (16 << 16)] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let value = (std::f32::consts::TAU * 220.0 * i as f32 / 44_100.0).sin();
        out.extend_from_slice(&((value * 20_000.0) as i16).to_le_bytes());
    }
    out
}

#[test]
fn playing_sfz_voices_does_not_allocate() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-sfz/real-time");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.wav"), sine_wav(44_100)).unwrap();
    fs::write(
        dir.join("a.sfz"),
        "<group> seq_length=2 ampeg_release=0.1 loop_mode=loop_sustain\n\
         <region> sample=a.wav seq_position=1\n\
         <region> sample=a.wav seq_position=2 lorand=0 hirand=0.5\n\
         <region> sample=a.wav seq_position=2 lorand=0.5 hirand=1",
    )
    .unwrap();
    let mut budget = sfz::MAX_PACK_SAMPLE_BYTES;
    let (instrument, _) = sfz::load(&dir.join("a.sfz"), &dir, &mut budget).unwrap();
    let mut sounds = vec![SfzSound {
        sound: Sound::GuitarNylon,
        keys: 0..=127,
        instrument,
    }];
    let mut voices = Voices::new();

    IS_COUNTING.with(|c| c.set(true));
    let mut energy = 0.0f32;
    for round in 0..200u32 {
        let timbre = [
            Timbre::Open,
            Timbre::Muted,
            Timbre::Harmonic,
            Timbre::Legato,
        ][round as usize % 4];
        let key = 40 + (round % 30) as u8;
        voices.note_on(
            &mut sounds,
            0,
            NoteOn {
                track: (round % 3) as usize,
                key,
                velocity: 100,
                timbre,
                sample_rate: 44_100,
            },
        );
        voices.set_bend((round % 3) as usize, 8192 + (round % 100) as u16 * 40);
        for _ in 0..256 {
            let (l, r) = voices.next_frame(&sounds);
            energy += l * l + r * r;
        }
        if round % 5 == 0 {
            voices.note_off((round % 3) as usize, key);
        }
        if round % 50 == 0 {
            voices.release_track(1);
            voices.release_all();
        }
        if round == 150 {
            voices.cut_all();
        }
    }
    IS_COUNTING.with(|c| c.set(false));

    assert!(energy > 0.0, "the voices made no sound");
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "allocations in the audio path"
    );
}
