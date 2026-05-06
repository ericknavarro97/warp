# cmux-style Workspaces — Product Spec

## Summary

Add a sidebar-driven concept that lets a single Warp window hold multiple independent groups of tabs, each scoped to a project, branch, or task. Internally the concept is named `TabGroup`; in the UI it is shown as "Workspace". Switching between groups instantly swaps the tab bar, the active pane, and (in later phases) the live metadata. Multiple windows and multiple groups per window coexist.

## Problem

Today a `Window` owns a flat `Vec<TabSnapshot>` (`app/src/app_state.rs:45`) and `Workspace` renders that single list (`app/src/workspace/view.rs:914`). There is no level between OS window and tab bar, so a user juggling two projects has to either open a second window (loses single-window focus, doubles chrome) or pile every tab into one bar (breaks visual context).

Tools like cmux solve this with sidebar workspace entries. The triple naming collision in our codebase — `workspace::view::Workspace`, `workspaces::Workspace` (SaaS), `persistence::Workspace` — has historically blocked introducing a fourth meaning. The internal name `TabGroup` resolves that while preserving the user-facing word "Workspace".

## Goals

- A window can host any number of tab groups (1..N), navigable from a sidebar.
- Each group owns its own ordered tab list and active tab.
- Tab bar contents are filtered to the active group; switching feels instant.
- Tabs can be moved between groups via drag, context menu, or shortcut.
- Existing single-group windows keep working unchanged: opening Warp on a previous build never loses tabs.
- Sidebar position (left default, right configurable) mirrors the existing vertical-tabs panel control.

## Non-goals

- Worktree auto-binding (one group per git worktree). Deferred to phase 4.
- Cross-window group moves. A group lives in exactly one window for now; tabs may only be transferred via existing cross-window tab drag.
- Renaming the existing `workspace::view::Workspace` struct or the SaaS `Workspaces` surface.
- Changing the horizontal tab bar's visual style or vertical tabs panel layout.
- New persistence backend; everything lands in the existing SQLite schema.

## User experience

### Anatomy

The window adds a vertical strip — the **Workspace switcher** — on the configured side (default left). Each entry is a tab-group row with icon, name, active-tab indicator, and (phase 5) a notification badge. The horizontal tab bar, the optional vertical tabs panel, and the active pane group all reflect the active group only.

### First launch

A user who never created a group sees the switcher with one auto-created entry named "Workspace 1" containing every tab the window had. Behavior is identical to today's Warp; nothing changes until the user opts in.

### Creating, switching, and lifecycle

- `Cmd+Shift+T` (default) creates a new empty group, focuses it, and opens a fresh tab.
- The Command Palette exposes "New Workspace"; right-click on the switcher exposes "New / Rename / Close Workspace".
- `Cmd+Shift+1..9` activates the Nth group. `Cmd+1..9` continues to activate tabs within the active group (preserved from today). The two shortcuts never collide.
- Clicking a switcher entry activates that group. Switching is atomic: the tab bar repaints, the previously active pane in the new group is focused, terminal input is ready immediately.
- Closing the last tab in a group does **not** auto-close the group. Closing the last group of a window does **not** close the window — Warp creates a fresh empty "Workspace 1" instead.

### Moving a tab between groups

- Drag a tab from the tab bar onto a switcher entry. The entry highlights; release transfers the tab into that group's tab list at the end position. Cross-window tab drag (`specs/pei/cross-window-tab-drag/PRODUCT.md`) is unaffected — perpendicular drag still detaches into a new window.
- Right-click a tab → "Move to Workspace" → submenu of other groups in the same window.
- Moved tabs preserve identity: running PTYs, editor state, panel state.

### Persistence and multi-window

- Quitting and reopening Warp restores every window with its groups, each group's tab order, and each group's previously active tab.
- The window-level "active group" is restored.
- Each window owns its own sequence of groups; the switcher is per-window.

## Out of scope

- Worktree, branch, or PR auto-bind (phase 4+).
- Per-group themes or settings overrides.
- Sharing a group across windows or machines.
- Sync of group state to Warp Drive.
- Reordering switcher entries via drag (may land in phase 4; not validated here).

## Validation criteria

1. Launching Warp on a build that predates this feature loads every prior tab into a synthesized "Workspace 1" with no data loss and no extra prompts.
2. `Cmd+Shift+T` creates a new group, focuses it, opens a fresh tab, and the previous group's tabs disappear from the tab bar.
3. `Cmd+Shift+2` activates the second group while `Cmd+2` still activates the second tab inside the active group.
4. Switching groups updates the horizontal tab bar, the vertical tabs panel (if open), and the focused pane atomically — no intermediate frame shows mixed state.
5. Dragging a tab onto another switcher entry moves the tab to that group with the running terminal still alive.
6. Right-click → "Move to Workspace → Workspace 2" produces the same result.
7. Closing every tab in a group leaves the (empty) group selected; closing every group leaves a fresh "Workspace 1" — the window itself stays open.
8. Quitting and reopening Warp restores N windows × M groups with the correct active group per window and active tab per group.
9. Changing the switcher position setting from Left to Right immediately moves the strip without restart.
10. A window with exactly one tab group renders identically to today's Warp aside from the switcher strip.
