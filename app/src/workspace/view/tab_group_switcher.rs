//! cmux-style "Workspace" sidebar.
//!
//! Renders a per-window vertical list of [`TabGroupSnapshot`]s with the
//! active group highlighted. Gated behind
//! `FeatureFlag::CmuxStyleWorkspaces`. See `specs/cmux-workspaces/` for
//! the full design.
//!
//! The renderer is deliberately minimal -- no scroll, no drag-and-drop,
//! no click handling -- because users drive the sidebar from
//! `Cmd+Shift+T` and `Cmd+Shift+1..8` (registered in
//! `app/src/workspace/mod.rs`). Polishing chrome (hover states, drag
//! reorder, color stripes from `TabGroupSnapshot::color`,
//! `TabGroupRuntimeMetadata` badges) lands in subsequent PRs.
use crate::app_state::TabGroupSnapshot;
use crate::appearance::Appearance;
use crate::workspace::action::TabGroupContextMenuAnchor;
use crate::workspace::WorkspaceAction;
use warp_core::ui::theme::color::internal_colors;
use warp_core::ui::Icon as WarpIcon;
use warpui::elements::{
    ChildView, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Empty,
    Expanded, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle, Padding,
    ParentElement, Radius, Text,
};
use warpui::platform::Cursor;
use warpui::{AppContext, SingletonEntity};

use super::Workspace;

/// Sidebar position. Mirrors the existing `super::PanelPosition` so the
/// switcher panel can be rendered on either side without re-parameterizing
/// every call site once `super::PanelPosition` is reused in Phase 4.5.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // `Right` is wired up by the user-side toggle in Phase 4.5.
pub(crate) enum TabGroupSwitcherSide {
    Left,
    Right,
}

impl Default for TabGroupSwitcherSide {
    fn default() -> Self {
        Self::Left
    }
}

/// View-local state for the switcher panel. Currently parks the values
/// the rendering pass will need (resizable width, scroll position, drag
/// hover index) without committing to the `warpui` handle types yet --
/// those are imported in Phase 4.5 alongside the actual `render_*`
/// function so the cargo graph stays minimal until the UI is wired up.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // fields read by Phase 4.5 renderer/event handlers.
pub(crate) struct TabGroupSwitcherState {
    pub side: TabGroupSwitcherSide,
    pub width_px: Option<f32>,
    pub scroll_offset_px: f32,
    pub drag_hover_index: Option<usize>,
}

impl TabGroupSwitcherState {
    pub(crate) const DEFAULT_WIDTH_PX: f32 = 248.;
    // Reserved for the resizable bounds the renderer will install in
    // Phase 4.5 (mirrors `vertical_tabs::PANEL_WIDTH` clamping).
    #[allow(dead_code)]
    pub(crate) const MIN_WIDTH_PX: f32 = 200.;
    #[allow(dead_code)]
    pub(crate) const MAX_WIDTH_RATIO: f32 = 0.5;

    /// Resolve the panel width to use for the upcoming frame, falling
    /// back to the cmux-matching default when the user has not yet
    /// resized the panel.
    pub(crate) fn effective_width(&self) -> f32 {
        self.width_px.unwrap_or(Self::DEFAULT_WIDTH_PX)
    }
}

/// Resolves the user-visible label for the group at `index` in
/// `groups`. Falls back to the auto-generated `Workspace N` form when
/// the user hasn't named it yet (or has explicitly cleared the name).
fn display_label(groups: &[TabGroupSnapshot], index: usize) -> String {
    match groups.get(index) {
        Some(g) if !g.name.is_empty() => g.name.clone(),
        _ => format!("Workspace {}", index + 1),
    }
}

/// Render the cmux-style sidebar. Returns the bare element tree -- the
/// `SavePosition` wrapping is the caller's responsibility so this
/// function stays composable across both left and right placements.
pub(crate) fn render_tab_group_switcher(
    workspace: &Workspace,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let font_family = appearance.ui_font_family();
    let row_padding = Padding::uniform(0.).with_vertical(8.).with_horizontal(12.);
    let header_padding = Padding::uniform(0.).with_vertical(10.).with_horizontal(12.);

    let mut column = Flex::column()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch);

    // Sidebar header: "Workspaces" label + a trailing "+" button that
    // dispatches `NewTabGroup` (same action Cmd+Shift+T fires). The
    // button is its own Hoverable so the click doesn't bubble up to
    // any ambient row handlers.
    let plus_icon_color = theme.sub_text_color(theme.background());
    let plus_hover_bg = internal_colors::fg_overlay_3(theme);
    let new_workspace_state = workspace.tab_group_new_button_mouse_state.clone();
    let new_workspace_button = Hoverable::new(new_workspace_state, move |state| {
        let mut container = Container::new(
            ConstrainedBox::new(WarpIcon::Plus.to_warpui_icon(plus_icon_color).finish())
                .with_width(14.)
                .with_height(14.)
                .finish(),
        )
        .with_padding(Padding::uniform(3.))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
        if state.is_hovered() {
            container = container.with_background(plus_hover_bg);
        }
        container.finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(|ctx, _, _| {
        ctx.dispatch_typed_action(WorkspaceAction::NewTabGroup);
    })
    .finish();

    let header_row = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(Expanded::new(
            1.,
            Text::new_inline("Workspaces", font_family, 12.).finish(),
        ).finish())
        .with_child(new_workspace_button)
        .finish();

    column.add_child(
        Container::new(header_row)
            .with_padding(header_padding)
            .finish(),
    );

    if workspace.tab_groups.is_empty() {
        column.add_child(
            Container::new(
                Text::new_inline(
                    "No workspaces yet. Press Cmd+Shift+T to create one.",
                    font_family,
                    11.,
                )
                .finish(),
            )
            .with_padding(row_padding)
            .finish(),
        );
    } else {
        let terminal_colors = theme.terminal_colors().normal.clone();
        for (idx, _group) in workspace.tab_groups.iter().enumerate() {
            let is_active = idx == workspace.active_tab_group_index;
            let is_renaming = workspace.tab_group_being_renamed == Some(idx);
            let bg = if is_active {
                internal_colors::fg_overlay_2(theme)
            } else {
                internal_colors::fg_overlay_1(theme)
            };

            // Reuse the same per-tab color machinery the horizontal
            // tab bar already drives -- a workspace inherits the color
            // of its active tab (or the first colored tab in the
            // group). Renders as a thin stripe on the leading edge of
            // the row, matching cmux's visual idiom.
            let stripe_color = workspace
                .tab_group_color(idx)
                .map(|color_id| color_id.to_ansi_color(&terminal_colors).into());

            let label_text = if is_renaming {
                None
            } else {
                Some(display_label(&workspace.tab_groups, idx))
            };

            // Stable mouse state per row so hover/double-click
            // bookkeeping survives re-renders. Created lazily.
            let mouse_state = workspace
                .tab_group_row_mouse_states
                .borrow_mut()
                .entry(idx)
                .or_insert_with(MouseStateHandle::default)
                .clone();

            // Stable mouse state for the hover-only close button on
            // this row. Lazily created -- empty workspaces still get a
            // handle so the close button renders stably across hover
            // transitions.
            let close_state = workspace
                .tab_group_close_mouse_states
                .borrow_mut()
                .entry(idx)
                .or_insert_with(MouseStateHandle::default)
                .clone();

            // The editor handle lives on Workspace; clone its handle
            // by reference so the closure can mount it when needed.
            let editor_handle = workspace.tab_group_rename_editor.clone();

            // Pre-compute the close-button colors so the inner closure
            // doesn't need to borrow `theme` (it is `'static + FnMut`).
            let close_icon_color = theme.sub_text_color(theme.background());
            let close_hover_bg = internal_colors::fg_overlay_3(theme);

            let row_padding_clone = row_padding;
            let mut row = Hoverable::new(mouse_state, move |state| {
                let row_hovered = state.is_hovered();
                let body: Box<dyn Element> = match label_text.as_ref() {
                    Some(text) => Text::new_inline(text.clone(), font_family, 12.).finish(),
                    None => ChildView::new(&editor_handle).finish(),
                };

                let mut row_flex = Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::Start)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);

                if let Some(color) = stripe_color {
                    row_flex.add_child(
                        ConstrainedBox::new(
                            Container::new(Empty::new().finish())
                                .with_background_color(color)
                                .finish(),
                        )
                        .with_width(3.)
                        .with_height(16.)
                        .finish(),
                    );
                    // Tiny gutter between the stripe and the label so
                    // text doesn't sit flush against the color bar.
                    row_flex.add_child(
                        ConstrainedBox::new(Empty::new().finish())
                            .with_width(8.)
                            .finish(),
                    );
                }

                row_flex.add_child(Expanded::new(1., body).finish());

                if row_hovered && !is_renaming {
                    let close_button = Hoverable::new(close_state.clone(), move |btn_state| {
                        let mut container = Container::new(
                            ConstrainedBox::new(
                                WarpIcon::X.to_warpui_icon(close_icon_color).finish(),
                            )
                            .with_width(12.)
                            .with_height(12.)
                            .finish(),
                        )
                        .with_padding(Padding::uniform(3.))
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
                        if btn_state.is_hovered() {
                            container = container.with_background(close_hover_bg);
                        }
                        container.finish()
                    })
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(WorkspaceAction::CloseTabGroup(idx));
                    })
                    .finish();
                    row_flex.add_child(close_button);
                }

                Container::new(row_flex.finish())
                    .with_background(bg)
                    .with_padding(row_padding_clone)
                    .finish()
            });
            // While the editor is mounted in the row, suppress the
            // click and double-click handlers so clicks inside the
            // editor don't re-trigger activation or the rename flow.
            if !is_renaming {
                row = row
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(WorkspaceAction::ActivateTabGroup(idx));
                    })
                    .on_double_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(WorkspaceAction::BeginRenameTabGroup(idx));
                    })
                    .on_right_click(move |ctx, _, position| {
                        ctx.dispatch_typed_action(
                            WorkspaceAction::ToggleTabGroupRightClickMenu {
                                index: idx,
                                anchor: TabGroupContextMenuAnchor::Pointer(position),
                            },
                        );
                    });
            }
            column.add_child(row.finish());
        }
    }

    let panel = Container::new(column.finish())
        .with_background(internal_colors::fg_overlay_1(theme))
        .with_padding(Padding::uniform(0.))
        .finish();

    ConstrainedBox::new(panel)
        .with_width(workspace.tab_group_switcher_state.effective_width())
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switcher_state_default_width_matches_cmux() {
        let state = TabGroupSwitcherState::default();
        assert_eq!(
            state.effective_width(),
            TabGroupSwitcherState::DEFAULT_WIDTH_PX
        );
    }
}
