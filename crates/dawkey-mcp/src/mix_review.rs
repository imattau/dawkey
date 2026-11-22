// SPDX-License-Identifier: AGPL-3.0-or-later
//! mix_review: a mix critique built on describe_audio's own rendering and
//! listening, plus `dawkey_listen::compare`'s cross-track comparison. Reads
//! only; it never changes the project, so applying any of its suggestions
//! is a separate, explicit batch call.

use std::collections::HashMap;

use dawkey_core::{GAIN_DB_MAX, GAIN_DB_MIN, MASTER, TrackId};
use dawkey_listen::{Category, DescribeOptions, Finding};
use serde_json::{Value, json};

use crate::times::SegmentSpec;
use crate::tools::{AudioSource, MixReview};
use crate::{DawkeyMcp, run, to_json};

/// Pan stays in this range (as `set_track_pan` requires).
const PAN_MIN: f32 = -1.0;
const PAN_MAX: f32 = 1.0;
/// Two tracks panned at least this far apart already don't need a nudge.
const PAN_ALREADY_SEPARATED: f32 = 0.4;

impl DawkeyMcp {
    pub(crate) async fn mix_review(&self, args: MixReview) -> Result<Value, String> {
        let source = AudioSource {
            start_s: args.start_s,
            end_s: args.end_s,
            tracks: args.tracks.clone(),
            stems: true,
            path: None,
        };
        let plan = self.plan_listen(source, true).await?;
        let maps = plan
            .maps
            .clone()
            .expect("mix_review only listens to the project, which always has a tempo map");
        let wanted = SegmentSpec {
            length_s: args.segment_length_s,
            bars: args.segment_bars,
            ranges: None,
        };
        let wanted = if wanted.length_s.is_some() || wanted.bars.is_some() {
            wanted
        } else {
            SegmentSpec {
                bars: Some(maps.timeline_bars(plan.start_s, plan.end_s)),
                ..SegmentSpec::default()
            }
        };
        let segments = maps.segments(plan.start_s, plan.end_s, &wanted)?;
        let options = DescribeOptions {
            segments,
            notes: false,
            max_notes: 0,
            max_onsets: 0,
        };
        let mut described = self
            .listen_each(&plan, move |clip, _| {
                dawkey_listen::describe(clip, &options)
            })
            .await?;
        // plan_listen (with_mix: true) puts the mix first, then one stem per
        // track in the same order as plan.renders[1..].
        let mix = described.remove(0);
        let ids: Vec<TrackId> = plan.renders[1..]
            .iter()
            .map(|r| r.stem.as_ref().expect("stems after the mix").0)
            .collect();
        let named: Vec<(String, dawkey_listen::Description)> = plan.renders[1..]
            .iter()
            .zip(described)
            .map(|(r, d)| (r.stem.as_ref().expect("stems after the mix").1.clone(), d))
            .collect();

        let findings = dawkey_listen::compare(&named, &mix);

        let mut wanted_tracks: Vec<TrackId> = ids.clone();
        wanted_tracks.push(MASTER);
        let current = self.current_gain_and_pan(&wanted_tracks).await?;

        let findings: Vec<Value> = findings
            .into_iter()
            .map(|f| finding_json(f, &ids, &named, &current))
            .collect();

        let mut result = json!({
            "start_s": plan.start_s,
            "end_s": plan.end_s,
        });
        if let Some(tracks) = &plan.tracks {
            result["tracks"] = json!(tracks);
        }
        result["findings"] = Value::Array(findings);
        Ok(result)
    }

    /// Each track's current gain (dB) and pan, for turning a dB or pan
    /// nudge into an absolute, in-range value.
    async fn current_gain_and_pan(
        &self,
        tracks: &[TrackId],
    ) -> Result<HashMap<TrackId, (f32, f32)>, String> {
        let tracks = tracks.to_vec();
        run(&self.app, move |s| {
            let project = s.session().project();
            Ok::<_, String>(
                tracks
                    .into_iter()
                    .filter_map(|id| project.track(id).map(|t| (id, (t.gain_db, t.pan))))
                    .collect(),
            )
        })
        .await
    }
}

/// One finding as JSON, with a `suggestion` of batch-ready operations
/// (`{tool, args}`, as `batch`'s `operations` takes) where there's a safe,
/// generic fix.
fn finding_json(
    finding: Finding,
    ids: &[TrackId],
    named: &[(String, dawkey_listen::Description)],
    current: &HashMap<TrackId, (f32, f32)>,
) -> Value {
    let track_refs: Vec<Value> = finding
        .tracks
        .iter()
        .filter_map(|&i| {
            let id = *ids.get(i)?;
            let name = named.get(i)?.0.clone();
            Some(json!({ "track": id, "name": name }))
        })
        .collect();
    let suggestion = match finding.category {
        Category::LevelBalance => finding.tracks.first().and_then(|&i| {
            let id = *ids.get(i)?;
            let (gain_db, _) = current.get(&id)?;
            let nudge = finding.gain_nudge_db?;
            let target =
                (f64::from(*gain_db) + nudge).clamp(f64::from(GAIN_DB_MIN), f64::from(GAIN_DB_MAX));
            Some(vec![json!({
                "tool": "set_track_gain",
                "args": { "track": id, "gain_db": target },
            })])
        }),
        Category::Headroom => (|| {
            let (gain_db, _) = current.get(&MASTER)?;
            let nudge = finding.gain_nudge_db?;
            let target =
                (f64::from(*gain_db) + nudge).clamp(f64::from(GAIN_DB_MIN), f64::from(GAIN_DB_MAX));
            Some(vec![json!({
                "tool": "set_track_gain",
                "args": { "track": MASTER, "gain_db": target },
            })])
        })(),
        Category::Masking => masking_suggestion(&finding, ids, current),
        Category::Clipping | Category::Stereo => None,
    };
    let mut value = to_json(&finding);
    if let Value::Object(map) = &mut value {
        map.insert("tracks".to_owned(), Value::Array(track_refs));
        match suggestion {
            Some(ops) => {
                map.insert("suggestion".to_owned(), Value::Array(ops));
            }
            None => {
                map.insert("suggestion".to_owned(), Value::Null);
            }
        }
    }
    value
}

fn masking_suggestion(
    finding: &Finding,
    ids: &[TrackId],
    current: &HashMap<TrackId, (f32, f32)>,
) -> Option<Vec<Value>> {
    let nudge = finding.pan_nudge?;
    let (&i, &j) = (finding.tracks.first()?, finding.tracks.get(1)?);
    let (id_a, id_b) = (*ids.get(i)?, *ids.get(j)?);
    let (_, pan_a) = *current.get(&id_a)?;
    let (_, pan_b) = *current.get(&id_b)?;
    if (pan_a - pan_b).abs() >= PAN_ALREADY_SEPARATED {
        return None;
    }
    let (target_a, target_b) = if pan_a <= pan_b {
        (pan_a - nudge, pan_b + nudge)
    } else {
        (pan_a + nudge, pan_b - nudge)
    };
    Some(vec![
        json!({
            "tool": "set_track_pan",
            "args": { "track": id_a, "pan": target_a.clamp(PAN_MIN, PAN_MAX) },
        }),
        json!({
            "tool": "set_track_pan",
            "args": { "track": id_b, "pan": target_b.clamp(PAN_MIN, PAN_MAX) },
        }),
    ])
}
