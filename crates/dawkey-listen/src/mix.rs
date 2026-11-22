// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comparing several tracks' [`Description`]s against each other: the half
//! of a mix critique [`describe`](crate::describe) can't give, since it only
//! ever looks at one clip at a time. Built entirely on numbers `describe`
//! already computes, so it stays as deterministic as the rest of this crate.

use serde::Serialize;

use crate::Description;

/// What a [`Finding`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Two tracks sound busy in the same register at the same time.
    Masking,
    /// A track sits well above or below the others' level.
    LevelBalance,
    /// The mix is too close to (or over) full scale.
    Headroom,
    /// Samples at or over full scale.
    Clipping,
    /// Correlation, width or balance worth a look.
    Stereo,
}

/// One thing that stands out when comparing tracks, in a sentence, with
/// enough structure to turn into a fix.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub category: Category,
    /// 0 (barely worth mentioning) to 1 (clearly audible).
    pub severity: f64,
    /// Indices into the `tracks` slice [`compare`] was given. Empty for a
    /// `Headroom`, `Clipping` or `Stereo` finding: those are about the mix
    /// as a whole, not a particular track.
    pub tracks: Vec<usize>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_range: Option<(f64, f64)>,
    /// A fader gain change, in dB, that would help: for `LevelBalance`, add
    /// it to `tracks[0]`'s gain; for `Headroom`, to the master's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain_nudge_db: Option<f64>,
    /// For `Masking`: pan `tracks[0]` this much further left and `tracks[1]`
    /// this much further right (or the other way around, whichever keeps
    /// them apart) to separate them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan_nudge: Option<f32>,
}

/// Below this, a segment isn't "sounding" for masking purposes.
const ACTIVE_DBFS: f64 = -40.0;
/// Within this many octaves, two centroids are "the same register".
const SAME_REGISTER_OCTAVES: f64 = 0.5;
/// A pair must overlap at least this fraction of their active segments to be
/// worth flagging.
const MIN_OVERLAP_FRACTION: f64 = 0.4;
/// ...and for at least this many segments, so one coincidence doesn't count.
const MIN_OVERLAP_SEGMENTS: usize = 2;
/// A track this many dB from the ensemble median is an outlier.
const LEVEL_OUTLIER_DB: f64 = 9.0;
/// Peaks closer to full scale than this leave too little headroom.
const HEADROOM_TARGET_DBFS: f64 = -1.0;

/// Compares `tracks` (name and description, one per track) against each
/// other, and the full mix's own description, for what stands out: masking
/// (two tracks busy in the same register at once), level balance (one far
/// louder or quieter than the rest), headroom, clipping and stereo issues.
///
/// Pass every track's [`Description`] with the same timeline segments (as
/// `describe_audio` gives when called with the same range and segment
/// options for each), so timeline moments line up index for index.
pub fn compare(tracks: &[(String, Description)], mix: &Description) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(headroom_and_clipping(mix));
    findings.extend(stereo(mix));
    findings.extend(masking(tracks));
    findings.extend(level_balance(tracks));
    findings
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn headroom_and_clipping(mix: &Description) -> Vec<Finding> {
    let mut findings = Vec::new();
    if mix.levels.clipped_samples > 0 {
        findings.push(Finding {
            category: Category::Clipping,
            severity: clamp01(0.5 + mix.levels.clipped_samples as f64 / 1000.0),
            tracks: Vec::new(),
            message: format!(
                "The mix has {} sample(s) at or over full scale; they'll clip once written \
                 to a file.",
                mix.levels.clipped_samples
            ),
            segment_range: None,
            gain_nudge_db: None,
            pan_nudge: None,
        });
    }
    if let Some(peak) = mix.levels.peak_dbfs
        && peak > HEADROOM_TARGET_DBFS
    {
        let nudge = HEADROOM_TARGET_DBFS - peak;
        findings.push(Finding {
            category: Category::Headroom,
            severity: clamp01((peak - HEADROOM_TARGET_DBFS) / 6.0),
            tracks: Vec::new(),
            message: format!(
                "The mix peaks at {peak:.1} dBFS, leaving less than 1 dB of headroom; pull the \
                 master down {:.1} dB.",
                -nudge
            ),
            segment_range: None,
            gain_nudge_db: Some(nudge),
            pan_nudge: None,
        });
    }
    findings
}

fn stereo(mix: &Description) -> Vec<Finding> {
    let Some(stereo) = &mix.stereo else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    if stereo.correlation < 0.0 {
        findings.push(Finding {
            category: Category::Stereo,
            severity: clamp01(-stereo.correlation),
            tracks: Vec::new(),
            message: format!(
                "Left and right are out of phase (correlation {:.2}); the mix will thin out \
                 or cancel when summed to mono.",
                stereo.correlation
            ),
            segment_range: None,
            gain_nudge_db: None,
            pan_nudge: None,
        });
    }
    if stereo.balance_db.abs() >= 1.5 {
        let side = if stereo.balance_db > 0.0 {
            "left"
        } else {
            "right"
        };
        findings.push(Finding {
            category: Category::Stereo,
            severity: clamp01(stereo.balance_db.abs() / 6.0),
            tracks: Vec::new(),
            message: format!(
                "The mix leans {:.1} dB to the {side}; check the master's pan and anything \
                 panned hard that way.",
                stereo.balance_db.abs()
            ),
            segment_range: None,
            gain_nudge_db: None,
            pan_nudge: None,
        });
    }
    findings
}

fn masking(tracks: &[(String, Description)]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for i in 0..tracks.len() {
        for j in (i + 1)..tracks.len() {
            let (name_a, a) = &tracks[i];
            let (name_b, b) = &tracks[j];
            let len = a.timeline.len().min(b.timeline.len());
            if len == 0 {
                continue;
            }
            let mut active = 0usize;
            let mut overlapping = 0usize;
            let mut range: Option<(f64, f64)> = None;
            for k in 0..len {
                let (ma, mb) = (&a.timeline[k], &b.timeline[k]);
                let loud_a = ma.peak_dbfs.is_some_and(|p| p > ACTIVE_DBFS);
                let loud_b = mb.peak_dbfs.is_some_and(|p| p > ACTIVE_DBFS);
                if !(loud_a || loud_b) {
                    continue;
                }
                active += 1;
                if !(loud_a && loud_b) {
                    continue;
                }
                let same_register = match (ma.centroid_hz, mb.centroid_hz) {
                    (Some(ca), Some(cb)) if ca > 0.0 && cb > 0.0 => {
                        (ca.log2() - cb.log2()).abs() <= SAME_REGISTER_OCTAVES
                    }
                    _ => false,
                };
                if same_register {
                    overlapping += 1;
                    range = Some(match range {
                        Some((start, _)) => (start, ma.end_s),
                        None => (ma.start_s, ma.end_s),
                    });
                }
            }
            if active == 0 || overlapping < MIN_OVERLAP_SEGMENTS {
                continue;
            }
            let fraction = overlapping as f64 / active as f64;
            if fraction < MIN_OVERLAP_FRACTION {
                continue;
            }
            findings.push(Finding {
                category: Category::Masking,
                severity: clamp01(fraction),
                tracks: vec![i, j],
                message: format!(
                    "{name_a} and {name_b} are busy in the same register at the same time for \
                     {:.0}% of where either plays; separating them in gain or pan would help \
                     them stay distinct.",
                    fraction * 100.0
                ),
                segment_range: range,
                gain_nudge_db: None,
                pan_nudge: Some(0.1),
            });
        }
    }
    findings
}

fn level_balance(tracks: &[(String, Description)]) -> Vec<Finding> {
    let mut levels: Vec<f64> = tracks
        .iter()
        .filter_map(|(_, d)| d.levels.rms_dbfs.or(d.levels.peak_dbfs))
        .collect();
    if levels.len() < 2 {
        return Vec::new();
    }
    levels.sort_by(|a, b| a.total_cmp(b));
    let median = levels[levels.len() / 2];
    let mut findings = Vec::new();
    for (i, (name, d)) in tracks.iter().enumerate() {
        let Some(level) = d.levels.rms_dbfs.or(d.levels.peak_dbfs) else {
            continue;
        };
        let diff = level - median;
        if diff.abs() < LEVEL_OUTLIER_DB {
            continue;
        }
        let word = if diff < 0.0 { "quieter" } else { "louder" };
        findings.push(Finding {
            category: Category::LevelBalance,
            severity: clamp01((diff.abs() - LEVEL_OUTLIER_DB) / 15.0 + 0.3),
            tracks: vec![i],
            message: format!(
                "{name} sits about {:.1} dB {word} than the other tracks' median level.",
                diff.abs()
            ),
            segment_range: None,
            gain_nudge_db: Some(-diff / 2.0),
            pan_nudge: None,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::describe::{Levels, Moment, Rhythm};

    fn levels(peak: Option<f64>, rms: Option<f64>) -> Levels {
        Levels {
            peak_dbfs: peak,
            true_peak_dbtp: peak,
            rms_dbfs: rms,
            lufs: rms,
            max_short_term_lufs: None,
            loudness_range_lu: None,
            crest_db: None,
            clipped_samples: 0,
            non_finite_samples: 0,
            sound_from_s: None,
            sound_to_s: None,
        }
    }

    fn moment(start_s: f64, peak: Option<f64>, centroid_hz: Option<f64>) -> Moment {
        Moment {
            start_s,
            end_s: start_s + 1.0,
            bar: None,
            peak_dbfs: peak,
            rms_dbfs: peak,
            centroid_hz,
            chord: None,
            onsets: 0,
            notes: String::new(),
        }
    }

    fn rhythm() -> Rhythm {
        Rhythm {
            onsets: 0,
            onsets_per_s: 0.0,
            tempo: None,
            timing: None,
            list: Vec::new(),
            unlisted: 0,
        }
    }

    fn description(peak: Option<f64>, rms: Option<f64>, timeline: Vec<Moment>) -> Description {
        Description {
            duration_s: timeline.len() as f64,
            levels: levels(peak, rms),
            stereo: None,
            spectrum: None,
            tonality: None,
            rhythm: rhythm(),
            notes: None,
            timeline,
            observations: Vec::new(),
        }
    }

    #[test]
    fn flags_two_tracks_busy_in_the_same_register() {
        let a = description(
            Some(-10.0),
            Some(-14.0),
            (0..4)
                .map(|k| moment(k as f64, Some(-10.0), Some(200.0)))
                .collect(),
        );
        let b = description(
            Some(-10.0),
            Some(-14.0),
            (0..4)
                .map(|k| moment(k as f64, Some(-10.0), Some(210.0)))
                .collect(),
        );
        let mix = description(Some(-10.0), Some(-14.0), Vec::new());
        let findings = compare(&[("Bass".into(), a), ("Kick".into(), b)], &mix);
        let masking = findings
            .iter()
            .find(|f| f.category == Category::Masking)
            .expect("masking finding");
        assert_eq!(masking.tracks, vec![0, 1]);
        assert!(masking.message.contains("Bass"));
        assert!(masking.message.contains("Kick"));
        assert_eq!(masking.pan_nudge, Some(0.1));
    }

    #[test]
    fn does_not_flag_tracks_in_different_registers() {
        let a = description(
            Some(-10.0),
            Some(-14.0),
            (0..4)
                .map(|k| moment(k as f64, Some(-10.0), Some(80.0)))
                .collect(),
        );
        let b = description(
            Some(-10.0),
            Some(-14.0),
            (0..4)
                .map(|k| moment(k as f64, Some(-10.0), Some(4000.0)))
                .collect(),
        );
        let mix = description(Some(-10.0), Some(-14.0), Vec::new());
        let findings = compare(&[("Bass".into(), a), ("Hats".into(), b)], &mix);
        assert!(!findings.iter().any(|f| f.category == Category::Masking));
    }

    #[test]
    fn flags_a_buried_track() {
        let loud = description(Some(-6.0), Some(-10.0), Vec::new());
        let quiet = description(Some(-26.0), Some(-30.0), Vec::new());
        let mix = description(Some(-6.0), Some(-10.0), Vec::new());
        let findings = compare(
            &[
                ("Lead".into(), loud),
                ("Pad".into(), quiet.clone()),
                ("Pad doubled".into(), quiet),
            ],
            &mix,
        );
        let buried = findings
            .iter()
            .find(|f| f.category == Category::LevelBalance)
            .expect("level balance finding");
        assert!(buried.message.contains("Lead"));
        assert!(buried.gain_nudge_db.unwrap() < 0.0);
    }

    #[test]
    fn flags_a_hot_mix() {
        let mix = description(Some(-0.2), Some(-6.0), Vec::new());
        let findings = compare(&[], &mix);
        let headroom = findings
            .iter()
            .find(|f| f.category == Category::Headroom)
            .expect("headroom finding");
        assert!((headroom.gain_nudge_db.unwrap() - -0.8).abs() < 1e-9);
    }

    #[test]
    fn flags_clipping() {
        let mut mix = description(Some(0.0), Some(-6.0), Vec::new());
        mix.levels.clipped_samples = 12;
        let findings = compare(&[], &mix);
        assert!(findings.iter().any(|f| f.category == Category::Clipping));
    }
}
