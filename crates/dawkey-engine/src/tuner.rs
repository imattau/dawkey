// SPDX-License-Identifier: AGPL-3.0-or-later
//! A tuner's live pitch feed: the real-time thread copies the armed track's
//! raw input into a lock-free ring, a short rolling window; the control
//! side drains it to run pitch detection off the real-time thread. Simpler
//! than [`crate::record`]'s rings: mono, no timeline tagging (only the most
//! recent samples matter), and one tap at a time.

use dawkey_core::InputChannels;
use rtrb::{Consumer, Producer, RingBuffer};

/// The real-time half of a tuner tap.
pub struct TunerTarget {
    channels: InputChannels,
    samples: Producer<f32>,
}

impl std::fmt::Debug for TunerTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunerTarget")
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

/// The control-side half of a tuner tap.
pub struct TunerTap {
    pub samples: Consumer<f32>,
}

impl std::fmt::Debug for TunerTap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunerTap").finish_non_exhaustive()
    }
}

/// Creates a tuner tap reading `channels` input, holding up to
/// `buffer_frames` frames before the control side falls behind.
pub fn tuner_session(channels: InputChannels, buffer_frames: usize) -> (TunerTarget, TunerTap) {
    let (tx, rx) = RingBuffer::new(buffer_frames);
    (
        TunerTarget {
            channels,
            samples: tx,
        },
        TunerTap { samples: rx },
    )
}

impl TunerTarget {
    /// Pushes this block's input, picked down to one channel. Real-time
    /// safe: if the ring is full, the newest samples are dropped (the
    /// control side only ever wants the most recent audio).
    pub(crate) fn push(&mut self, input: [&[f32]; 2]) {
        let picked = match self.channels {
            InputChannels::Stereo | InputChannels::Left => input[0],
            InputChannels::Right => input[1],
        };
        for &s in picked {
            if self.samples.push(s).is_err() {
                break;
            }
        }
    }
}
