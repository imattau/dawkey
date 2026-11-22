// SPDX-License-Identifier: AGPL-3.0-or-later
//! The tuner panel: a live read of an armed track's input against standard
//! guitar tuning.

use crate::App;

impl App {
    /// Opens the tuner for `track` (must be armed and audio), closing any
    /// other overlay and whatever track was being tuned before.
    pub(crate) fn open_tuner(&mut self, track: dawkey_core::TrackId) -> Result<(), String> {
        self.close_overlays();
        self.studio.start_tuner(track).map_err(|e| e.to_string())?;
        self.view.shared.tuner_track = Some(track);
        Ok(())
    }

    fn close_tuner(&mut self) {
        if self.view.shared.tuner_track.take().is_some() {
            self.studio.stop_tuner();
        }
    }

    /// Clicks on the tuner panel. `None` if `parts` isn't one of theirs.
    pub(crate) fn on_tuner_click(&mut self, parts: &[&str]) -> Option<Result<(), String>> {
        Some(match parts {
            ["tuner", "close" | "backdrop"] => {
                self.close_tuner();
                Ok(())
            }
            _ => return None,
        })
    }

    /// Escape closes the tuner panel, like the other overlays.
    pub(crate) fn on_tuner_key(&mut self, key: &str) -> Option<Result<(), String>> {
        self.view.shared.tuner_track?;
        if key == "escape" {
            self.close_tuner();
        }
        Some(Ok(()))
    }
}
