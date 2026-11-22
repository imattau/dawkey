// SPDX-License-Identifier: AGPL-3.0-or-later
//! The first-run setup for a new project.

use dawkey_ui::{Align, Color, FontWeight, Insets, Justify, Node, Rect, Role, Sizing, UiState};

use super::{ViewModel, button};
use crate::state::{NewProjectDraft, ViewState};
use crate::theme::*;

pub(super) fn dialog(
    _model: &ViewModel,
    view: &ViewState,
    ui: &UiState,
    area: Rect,
    radius: f32,
) -> Option<Node> {
    let draft = view.shared.new_project.as_ref()?;
    let action = |id: &str, label: &str, active| {
        button(id, label, ui, active)
            .text(label)
            .padding(Insets::xy(SPACING.xl, 0.0))
            .height(Sizing::Fixed(30.0))
            .style(|s| s.weight = FontWeight::SemiBold)
    };
    let line = |label: &str, value: &str| {
        Node::row()
            .gap(SPACING.md)
            .child(
                Node::label(label)
                    .width(Sizing::Fixed(130.0))
                    .style(|s| s.color = pal().text_dim),
            )
            .child(
                Node::label(value)
                    .width(Sizing::Fixed(70.0))
                    .style(|s| s.color = pal().text),
            )
    };
    let adjust = |id: &str, label: &str| {
        button(id, label, ui, None)
            .text(label)
            .padding(Insets::xy(SPACING.sm, 0.0))
            .height(Sizing::Fixed(24.0))
    };
    let tempo = Node::row()
        .gap(SPACING.xs)
        .child(line("Tempo", &format!("{} BPM", draft.tempo)))
        .child(adjust("new_project/tempo_down", "−"))
        .child(adjust("new_project/tempo_up", "+"));
    let meter = Node::row()
        .gap(SPACING.xs)
        .child(line(
            "Time signature",
            &format!("{}/{}", draft.numerator, draft.denominator),
        ))
        .child(adjust("new_project/meter_down", "−"))
        .child(adjust("new_project/meter_up", "+"))
        .child(adjust("new_project/meter_den_down", "Beat −"))
        .child(adjust("new_project/meter_den_up", "Beat +"));
    let panel = Node::column()
        .id("new_project")
        .role(Role::Group)
        .accessible_label("New Project")
        .width(Sizing::Fixed(420.0))
        .padding(Insets::all(SPACING.xxl))
        .gap(SPACING.md)
        .align(Align::Start)
        .style(|s| {
            s.background = Some(pal().panel);
            s.border = Some((1.0, pal().border));
            s.radius = [PANEL_RADIUS; 4];
            s.shadow = Some(ELEVATION.medium());
        })
        .child(Node::label("Set up a new project").style(|s| {
            s.color = pal().text;
            s.weight = FontWeight::SemiBold;
            s.font_size = 15.0;
        }))
        .child(line("Project name", &draft.name))
        .child(line("Starting point", "Empty project"))
        .child(tempo)
        .child(meter)
        .child(
            Node::label("Choose a name and project file location after continuing.").style(|s| {
                s.color = pal().text_dim;
                s.font_size = 12.0;
            }),
        )
        .child(
            Node::row()
                .width(Sizing::Fill(1.0))
                .gap(SPACING.md)
                .padding(Insets {
                    top: SPACING.xl,
                    ..Insets::default()
                })
                .child(Node::spacer())
                .child(action("new_project/cancel", "Cancel", None))
                .child(action(
                    "new_project/create",
                    "Choose location…",
                    Some(pal().accent),
                )),
        );
    Some(
        Node::row()
            .id("new_project/backdrop")
            .accessible_label("New Project")
            .clickable()
            .scrollable()
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

pub(crate) fn defaults() -> NewProjectDraft {
    NewProjectDraft {
        name: "Untitled".into(),
        tempo: "120".into(),
        numerator: "4".into(),
        denominator: "4".into(),
    }
}
