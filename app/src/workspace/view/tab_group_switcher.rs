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
use crate::workspace::WorkspaceAction;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    ChildView, ConstrainedBox, Container, CrossAxisAlignment, Element, Empty, Flex, Hoverable,
    MainAxisAlignment, MainAxisSize, MouseStateHandle, Padding, ParentElement, Text,
};
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

/// Resolve the entry that should currently appear "selected" in the
/// sidebar given a snapshot of the window's tab groups and the active
/// index. Empty `tab_groups` (legacy windows pre-Phase-2 backfill) is
/// reported as `None` -- the sidebar UI either renders the placeholder
/// "Workspace 1" row or hides itself entirely in that case.
pub(crate) fn active_group<'a>(
    tab_groups: &'a [TabGroupSnapshot],
    active_index: usize,
) -> Option<&'a TabGroupSnapshot> {
    tab_groups.get(active_index)
}

/// Predicate the renderer uses to decide whether to short-circuit the
/// whole panel. Phase 4.5 wires this to the feature flag *and* to the
/// per-window `vertical_tabs_panel_open` toggle so users with the flag
/// off see no UI delta whatsoever.
pub(crate) fn should_render_switcher(feature_enabled: bool, panel_open: bool) -> bool {
    feature_enabled && panel_open
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

    column.add_child(
        Container::new(Text::new_inline("Workspaces", font_family, 12.).finish())
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

            // The editor handle lives on Workspace; clone its handle
            // by reference so the closure can mount it when needed.
            let editor_handle = workspace.tab_group_rename_editor.clone();

            let row_padding_clone = row_padding;
            let mut row = Hoverable::new(mouse_state, move |_state| {
                let body: Box<dyn Element> = match label_text.as_ref() {
                    Some(text) => Text::new_inline(text.clone(), font_family, 12.).finish(),
                    None => ChildView::new(&editor_handle).finish(),
                };

                let mut row_flex = Flex::row()
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

                row_flex.add_child(body);

                Container::new(row_flex.finish())
                    .with_background(bg)
                    .with_padding(row_padding_clone)
                    .finish()
            });
            // While the editor is mounted in the row, suppress the
            // double-click handler so clicks inside the editor don't
            // re-trigger the rename flow.
            if !is_renaming {
                row = row.on_double_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(WorkspaceAction::BeginRenameTabGroup(idx));
                });
            }
            column.add_child(row.finish());
        }
    }

    Container::new(column.finish())
        .with_background(internal_colors::fg_overlay_1(theme))
        .with_padding(Padding::uniform(0.))
        .finish()
}

#[allow(dead_code)]
fn _unused_empty_placeholder() -> Box<dyn Element> {
    Empty::new().finish()
}

/// Live, per-group sidebar metadata (Phase 5). Deliberately *not* part
/// of [`crate::app_state::TabGroupSnapshot`]: every field is recomputed
/// at runtime from the focused session's cwd / `gh` cache / port
/// listener, so persisting it to SQLite would only let stale data
/// flicker on relaunch.
///
/// Phase 5.5 will plug in the actual fetchers (lesson learned from the
/// upstream cmux #2746 incident: cache PR lookups aggressively to avoid
/// hammering the GitHub API) -- this struct is the contract those
/// fetchers populate against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabGroupRuntimeMetadata {
    /// Git branch of the focused session's working directory, when the
    /// directory is a git worktree. `None` for non-repo workspaces.
    pub git_branch: Option<String>,
    /// Number of the GitHub pull request linked to `git_branch`.
    pub pr_number: Option<i32>,
    /// Ports the workspace's sessions are currently listening on. Empty
    /// for workspaces without active processes.
    pub listening_ports: Vec<u16>,
    /// Number of unseen agent / OSC-99 / OSC-777 notifications -- drives
    /// the badge ring around the sidebar entry.
    pub pending_notifications: u32,
}

impl TabGroupRuntimeMetadata {
    /// True when the renderer should draw the notification ring for
    /// this group. Centralizes the rule so Phase 5.5 fetchers and the
    /// sidebar UI agree on the threshold.
    pub(crate) fn has_pending_notifications(&self) -> bool {
        self.pending_notifications > 0
    }

    /// Convenience accessor used by the sidebar to short-circuit when
    /// nothing live is worth showing for the group.
    pub(crate) fn is_empty(&self) -> bool {
        self.git_branch.is_none()
            && self.pr_number.is_none()
            && self.listening_ports.is_empty()
            && self.pending_notifications == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(name: &str, position: i32, is_active: bool) -> TabGroupSnapshot {
        TabGroupSnapshot {
            name: name.into(),
            color: None,
            position,
            is_active,
        }
    }

    #[test]
    fn active_group_returns_indexed_entry() {
        let groups = vec![group("a", 0, false), group("b", 1, true), group("c", 2, false)];
        assert_eq!(active_group(&groups, 1).map(|g| g.name.as_str()), Some("b"));
    }

    #[test]
    fn active_group_handles_legacy_empty_window() {
        let groups: Vec<TabGroupSnapshot> = Vec::new();
        assert!(active_group(&groups, 0).is_none());
    }

    #[test]
    fn switcher_state_default_width_matches_cmux() {
        let state = TabGroupSwitcherState::default();
        assert_eq!(
            state.effective_width(),
            TabGroupSwitcherState::DEFAULT_WIDTH_PX
        );
    }

    #[test]
    fn switcher_hidden_when_feature_off() {
        assert!(!should_render_switcher(false, true));
        assert!(!should_render_switcher(true, false));
        assert!(should_render_switcher(true, true));
    }

    #[test]
    fn runtime_metadata_default_is_empty() {
        let meta = TabGroupRuntimeMetadata::default();
        assert!(meta.is_empty());
        assert!(!meta.has_pending_notifications());
    }

    #[test]
    fn runtime_metadata_reports_pending_notifications() {
        let meta = TabGroupRuntimeMetadata {
            pending_notifications: 3,
            ..TabGroupRuntimeMetadata::default()
        };
        assert!(meta.has_pending_notifications());
        assert!(!meta.is_empty());
    }

    #[test]
    fn runtime_metadata_is_not_empty_when_branch_known() {
        let meta = TabGroupRuntimeMetadata {
            git_branch: Some("main".into()),
            ..TabGroupRuntimeMetadata::default()
        };
        assert!(!meta.is_empty());
        assert!(!meta.has_pending_notifications());
    }
}
