// SPDX-License-Identifier: AGPL-3.0-or-later
//! mix_review over MCP: a mix critique built from describe_audio's own
//! rendering, plus the cross-track comparison in dawkey_listen::compare.

use dawkey_testkit::Harness;
use serde_json::{Value, json};

const DAWKEY: &str = env!("CARGO_BIN_EXE_dawkey");

async fn start() -> Harness {
    Harness::headless(DAWKEY, std::iter::empty::<&str>())
        .await
        .expect("dawkey starts")
}

/// A MIDI track with one sustained note across `bars` bars, with `name`,
/// `pitch` and `velocity`. Returns its track id.
async fn note_track(dawkey: &Harness, name: &str, pitch: u8, velocity: u8, bars: u32) -> u64 {
    let track = dawkey
        .call(
            "add_midi_clip",
            json!({
                "position_s": 0,
                "notes": [{
                    "pitch": pitch,
                    "velocity": velocity,
                    "start_beats": 0,
                    "length_beats": bars * 4,
                }],
            }),
        )
        .await
        .unwrap()["track"]
        .as_u64()
        .unwrap();
    dawkey
        .call("rename_track", json!({ "track": track, "name": name }))
        .await
        .unwrap();
    track
}

fn findings(result: &Value) -> &[Value] {
    result["findings"].as_array().expect("findings array")
}

fn finding<'a>(result: &'a Value, category: &str) -> &'a Value {
    findings(result)
        .iter()
        .find(|f| f["category"] == category)
        .unwrap_or_else(|| panic!("no {category} finding in {result:#}"))
}

#[tokio::test]
async fn is_listed_as_read_only_with_beats_and_bars() {
    let dawkey = start().await;
    let tools = dawkey.tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == "mix_review").unwrap();
    assert_eq!(
        tool.annotations.as_ref().and_then(|a| a.read_only_hint),
        Some(true)
    );
    let schema = Value::Object((*tool.input_schema).clone());
    assert!(schema["properties"]["start_bar"].is_object(), "{schema}");
    assert!(schema["properties"]["end_beats"].is_object(), "{schema}");
    dawkey.shutdown().await.unwrap();
}

#[tokio::test]
async fn flags_masking_between_two_identical_tracks() {
    let dawkey = start().await;
    dawkey
        .call("set_tempo", json!({ "bpm": 120 }))
        .await
        .unwrap();
    let a = note_track(&dawkey, "Lead A", 60, 100, 4).await;
    let b = note_track(&dawkey, "Lead B", 60, 100, 4).await;

    let heard = dawkey
        .call("mix_review", json!({ "start_bar": 1, "end_bar": 5 }))
        .await
        .unwrap();
    let masking = finding(&heard, "masking");
    let tracks: Vec<u64> = masking["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["track"].as_u64().unwrap())
        .collect();
    assert_eq!(tracks, vec![a, b], "{heard:#}");
    assert!(masking["message"].as_str().unwrap().contains("Lead A"));

    let suggestion = masking["suggestion"].as_array().expect("a suggestion");
    assert_eq!(suggestion.len(), 2, "{suggestion:#?}");
    for op in suggestion {
        assert_eq!(op["tool"], "set_track_pan");
    }

    // The suggestion is ready to drop straight into batch.
    let outcome = dawkey
        .call(
            "batch",
            json!({ "operations": suggestion, "label": "mix_review fix" }),
        )
        .await
        .unwrap();
    assert_eq!(
        outcome["outcomes"].as_array().unwrap().len(),
        2,
        "{outcome:#}"
    );
    dawkey.shutdown().await.unwrap();
}

#[tokio::test]
async fn flags_a_buried_track() {
    let dawkey = start().await;
    dawkey
        .call("set_tempo", json!({ "bpm": 120 }))
        .await
        .unwrap();
    let loud1 = note_track(&dawkey, "Loud 1", 60, 110, 4).await;
    let loud2 = note_track(&dawkey, "Loud 2", 67, 110, 4).await;
    let quiet = note_track(&dawkey, "Quiet", 72, 110, 4).await;
    dawkey
        .call(
            "set_track_gain",
            json!({ "track": quiet, "gain_db": -30.0 }),
        )
        .await
        .unwrap();

    let heard = dawkey
        .call("mix_review", json!({ "start_bar": 1, "end_bar": 5 }))
        .await
        .unwrap();
    let balance = finding(&heard, "level_balance");
    assert_eq!(
        balance["tracks"][0]["track"].as_u64(),
        Some(quiet),
        "{heard:#}"
    );
    let suggestion = &balance["suggestion"][0];
    assert_eq!(suggestion["tool"], "set_track_gain");
    assert_eq!(suggestion["args"]["track"].as_u64(), Some(quiet));
    // It should be a boost, not a further cut.
    assert!(
        suggestion["args"]["gain_db"].as_f64().unwrap() > -30.0,
        "{suggestion}"
    );
    assert!(!findings(&heard).iter().any(
        |f| f["category"] == "level_balance" && f["tracks"][0]["track"].as_u64() == Some(loud1)
    ));
    let _ = loud2;
    dawkey.shutdown().await.unwrap();
}

#[tokio::test]
async fn flags_a_hot_mix() {
    let dawkey = start().await;
    dawkey
        .call("set_tempo", json!({ "bpm": 120 }))
        .await
        .unwrap();
    for pitch in [60, 64, 67, 72] {
        let track = note_track(&dawkey, "Loud", pitch, 127, 4).await;
        dawkey
            .call("set_track_gain", json!({ "track": track, "gain_db": 12.0 }))
            .await
            .unwrap();
    }

    let heard = dawkey
        .call("mix_review", json!({ "start_bar": 1, "end_bar": 5 }))
        .await
        .unwrap();
    let headroom = finding(&heard, "headroom");
    assert_eq!(headroom["tracks"].as_array().unwrap().len(), 0, "{heard:#}");
    let suggestion = &headroom["suggestion"][0];
    assert_eq!(suggestion["tool"], "set_track_gain");
    assert_eq!(suggestion["args"]["track"].as_u64(), Some(0));
    assert!(
        suggestion["args"]["gain_db"].as_f64().unwrap() < 0.0,
        "{suggestion}"
    );

    let outcome = dawkey
        .call("batch", json!({ "operations": [suggestion.clone()] }))
        .await
        .unwrap();
    assert_eq!(
        outcome["outcomes"].as_array().unwrap().len(),
        1,
        "{outcome:#}"
    );
    dawkey.shutdown().await.unwrap();
}
