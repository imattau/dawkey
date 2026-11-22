// SPDX-License-Identifier: AGPL-3.0-or-later
//! The tuner panel: the armed track's detected pitch against standard
//! guitar tuning, as a centered cents gauge.

use dawkey_ui::{
    Align, Color, FontWeight, Insets, Justify, Node, Paint, Rect, Role, Sizing, UiState,
};

use super::{ViewModel, button};
use crate::state::ViewState;
use crate::theme::*;

const PANEL_W: f32 = 320.0;
const BAR_W: f32 = 260.0;
const BAR_H: f32 = 28.0;
/// Cents either side of the gauge's ends.
const RANGE_CENTS: f64 = 50.0;

const STRING_NAMES: [&str; 6] = ["E2", "A2", "D3", "G3", "B3", "E4"];

/// The panel over `area` (the window below its title bar), if it's open.
pub(super) fn panel(
    model: &ViewModel,
    view: &ViewState,
    ui: &UiState,
    area: Rect,
    radius: f32,
) -> Option<Node> {
    view.shared.tuner_track?;
    let dim = |text: String| {
        Node::label(text).style(|s| {
            s.color = pal().text_dim;
            s.font_size = 12.0;
        })
    };
    let (heading, bar) = match model.tuner {
        Some(reading) => (
            format!("{} · {:.1} Hz", STRING_NAMES[reading.string], reading.hz),
            cents_bar(Some(reading.cents)),
        ),
        None => ("Listening…".to_owned(), cents_bar(None)),
    };
    let panel = Node::column()
        .id("tuner")
        .role(Role::Group)
        .accessible_label("Tuner")
        .clickable()
        .width(Sizing::Fixed(PANEL_W))
        .padding(Insets::all(SPACING.xxl))
        .gap(SPACING.md)
        .align(Align::Center)
        .style(|s| {
            s.background = Some(pal().panel);
            s.border = Some((1.0, pal().border));
            s.radius = [PANEL_RADIUS; 4];
            s.shadow = Some(ELEVATION.medium());
        })
        .child(Node::label("Tuner").style(|s| {
            s.color = pal().text;
            s.weight = FontWeight::SemiBold;
            s.font_size = 15.0;
        }))
        .child(dim(heading))
        .child(bar)
        .child(
            Node::row()
                .width(Sizing::Fill(1.0))
                .justify(Justify::End)
                .padding(Insets {
                    top: SPACING.md,
                    ..Insets::default()
                })
                .child(
                    button("tuner/close", "Close", ui, None)
                        .text("Close")
                        .padding(Insets::xy(12.0, 0.0))
                        .height(Sizing::Fixed(26.0)),
                ),
        );
    Some(
        Node::row()
            .id("tuner/backdrop")
            .accessible_label("Tuner")
            .clickable()
            .absolute(area)
            .align(Align::Center)
            .justify(Justify::Center)
            .style(|s| {
                s.background = Some(Color::rgba(0.0, 0.0, 0.0, 0.45));
                s.radius = [0.0, 0.0, radius, radius];
            })
            .child(panel),
    )
}

/// A centered gauge from -[`RANGE_CENTS`] to +[`RANGE_CENTS`]: a track with
/// a center tick, and a fill from center to `cents` (clamped), colored by
/// how close it is to in tune. `None` (no clear pitch yet) shows just the
/// track.
fn cents_bar(cents: Option<f64>) -> Node {
    let center = BAR_W / 2.0;
    let mut node = Node::row()
        .id("tuner/bar")
        .role(Role::Meter)
        .accessible_label("Cents from the nearest string")
        .value(match cents {
            Some(c) => format!("{c:+.0} cents"),
            None => "no signal".to_owned(),
        })
        .size(BAR_W, BAR_H)
        .style(|s| {
            s.background = Some(pal().meter);
            s.radius = [4.0; 4];
        });
    if let Some(cents) = cents {
        let offset = (cents.clamp(-RANGE_CENTS, RANGE_CENTS) / RANGE_CENTS) as f32 * center;
        let color = match cents.abs() {
            c if c < 5.0 => pal().solo_on,
            c if c < 20.0 => pal().mute_on,
            _ => pal().record_on,
        };
        let (x, w) = if offset >= 0.0 {
            (center, offset)
        } else {
            (center + offset, -offset)
        };
        node = node.child(
            Node::row()
                .absolute(Rect::new(x, 0.0, w.max(1.0), BAR_H))
                .paint(Paint::Rects {
                    rects: vec![Rect::new(0.0, 0.0, w.max(1.0), BAR_H)].into(),
                    color,
                }),
        );
    }
    node.child(
        Node::row()
            .absolute(Rect::new(center - 1.0, 0.0, 2.0, BAR_H))
            .paint(Paint::Rects {
                rects: vec![Rect::new(0.0, 0.0, 2.0, BAR_H)].into(),
                color: pal().text,
            }),
    )
}
