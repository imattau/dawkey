// SPDX-License-Identifier: AGPL-3.0-or-later
//! How far a detected pitch is from the nearest string of a tuning (a
//! tuner's readout), for any list of target frequencies, not just a
//! guitar's.

use crate::spectral::hz_of_midi;

/// Standard 6-string guitar tuning, low to high (E2 A2 D3 G3 B3 E4), in Hz.
pub fn standard_guitar() -> [f64; 6] {
    [40.0, 45.0, 50.0, 55.0, 59.0, 64.0].map(hz_of_midi)
}

/// The nearest frequency in `targets` to `hz`, its index, and how far off it
/// is in cents (positive: `hz` is sharp of it).
pub fn nearest_target(hz: f64, targets: &[f64]) -> (usize, f64) {
    let (i, target) = targets
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (hz / **a).log2().abs().total_cmp(&(hz / **b).log2().abs()))
        .expect("targets is non-empty");
    (i, 1200.0 * (hz / target).log2())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_guitar_is_e_a_d_g_b_e() {
        let hz = standard_guitar();
        assert!((hz[0] - 82.41).abs() < 0.01, "{hz:?}"); // E2
        assert!((hz[1] - 110.0).abs() < 0.01, "{hz:?}"); // A2
        assert!((hz[5] - 329.63).abs() < 0.01, "{hz:?}"); // E4
    }

    #[test]
    fn snaps_to_the_nearest_string() {
        let targets = standard_guitar();
        // A shade sharp of the open A string (110 Hz).
        let (i, cents) = nearest_target(111.3, &targets);
        assert_eq!(i, 1);
        assert!((cents - 20.0).abs() < 1.0, "{cents}");
    }

    #[test]
    fn picks_the_closer_of_two_neighbors() {
        let targets = standard_guitar();
        // Nearer to D3 (147 Hz) than to G3 (196 Hz) in cents.
        let (i, _) = nearest_target(155.0, &targets);
        assert_eq!(i, 2);
    }
}
