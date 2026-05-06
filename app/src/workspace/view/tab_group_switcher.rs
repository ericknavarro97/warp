//! Skeleton for the cmux-style "Workspace" sidebar (Phase 4).
//!
//! This module holds state and pure helpers that the sidebar UI added in
//! Phase 4.5 will render against. Everything in here is gated behind
//! `FeatureFlag::CmuxStyleWorkspaces` -- no caller exists yet, but the
//! types are public so the eventual `render_tab_group_switcher` element
//! tree (and its `Workspace` view integration) lands in a focused PR
//! without further data-shape churn.
//!
//! See `specs/cmux-workspaces/` for the full design.
use crate::app_state::TabGroupSnapshot;

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
}
