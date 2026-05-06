# cmux-style Workspaces (Tech Spec)

## 1. Problem

Warp's `Window` owns one flat `Vec<TabSnapshot>` (`app/src/app_state.rs:45`) and `Workspace` renders that list directly (`app/src/workspace/view.rs:914-916`). To deliver cmux's "sidebar of contexts" UX we need a real intermediate level between `Window` and `tabs[]` that:

1. Persists across restarts in the existing SQLite schema with strict backwards compatibility.
2. Coexists with the SaaS `workspaces::Workspace` and the persistence `Workspace` tables without renaming either.
3. Plugs into `WorkspaceRegistry` (`app/src/workspace/registry.rs:11`), `AppState` snapshotting (`app/src/persistence/sqlite.rs:808-1000`, `:2650-2802`), and the `Workspace` view path without forking the renderer.
4. Ships incrementally — each phase merges and unblocks the next.

The internal name is `TabGroup`. The user-facing label is "Workspace". This single naming decision avoids the triple collision that has historically blocked the feature.

## 2. Relevant code

- `app/src/app_state.rs:28-70` — `AppState`, `WindowSnapshot`, `TabSnapshot`.
- `app/src/workspace/view.rs:914-916` — `Workspace` UI struct holding `tabs: Vec<TabData>`.
- `app/src/workspace/registry.rs:11` — `WorkspaceRegistry::workspaces: HashMap<WindowId, Weak<Workspace>>`.
- `app/src/workspace/view/vertical_tabs.rs` — pattern for the new switcher panel.
- `crates/persistence/src/schema.rs:357-364` (tabs), `:433-450` (windows).
- `crates/persistence/src/model.rs:344-359` — `Tab` / `NewTab` Diesel structs.
- `app/src/persistence/sqlite.rs:808-1000` — `save_app_state`.
- `app/src/persistence/sqlite.rs:2650-2802` — `read_sqlite_data`.

## 3. Architecture

### 3.1 Current shape

```
AppState
 └── windows: Vec<WindowSnapshot>
      └── tabs: Vec<TabSnapshot>     (active_tab_index: usize)
           └── root: PaneNodeSnapshot
```

`Workspace` (the view) owns one `Vec<TabData>`. `WorkspaceRegistry` is `WindowId -> Weak<Workspace>`. SQLite mirrors this: `windows` rows have `active_tab_index`, `tabs` rows belong to `windows` via `tabs.window_id`.

### 3.2 Proposed shape

```
AppState
 └── windows: Vec<WindowSnapshot>
      ├── tab_groups: Vec<TabGroupSnapshot>      (NEW; len >= 1)
      │    ├── id, name, color
      │    ├── tabs: Vec<TabSnapshot>            (moved here)
      │    └── active_tab_index: usize           (moved here)
      └── active_tab_group_index: usize          (NEW)
```

`Workspace` gains a `TabGroupId`-keyed `IndexMap<TabGroupId, TabGroupState>`; the renderer reads only the active group's state. `WorkspaceRegistry` becomes `HashMap<WindowId, Vec<TabGroupId>>` plus `HashMap<TabGroupId, Weak<Workspace>>` so existing window-keyed callers still work and group-specific actions resolve directly.

A new `TabGroupSwitcherPanel` (phase 4) renders the sidebar, structurally mirroring `vertical_tabs.rs`.

## 4. Data model changes

Phase 1 adds two schema-level changes:

- New table `tab_groups`:
  - `id Integer PK`
  - `window_id Integer NOT NULL` (FK)
  - `position Integer NOT NULL`
  - `name Text NOT NULL`
  - `color Nullable<Text>`
  - `active_tab_index Integer NOT NULL DEFAULT 0`
- New nullable column `tabs.tab_group_id Nullable<Integer>` referencing `tab_groups.id`.

A new column `windows.active_tab_group_index Nullable<Integer>` lands in phase 2. `windows.active_tab_index` stays so legacy reads keep working; phase 2 deprecates but never drops it.

`tabs.tab_group_id` is intentionally nullable: phase 1 ships only the schema and Diesel bindings (with tests). No runtime path writes it yet, so phase 1 lands without migration risk — every existing row stays valid, every existing query keeps working.

`crates/persistence/src/model.rs:344-359` gets `TabGroup` / `NewTabGroup` structs and `Tab` / `NewTab` gain the optional `tab_group_id` field.

## 5. AppState integration (phase 2)

`WindowSnapshot` (`app/src/app_state.rs:45`) gains `tab_groups: Vec<TabGroupSnapshot>` and `active_tab_group_index: usize`. `tabs` and `active_tab_index` move into `TabGroupSnapshot`. `save_app_state` writes one `tab_groups` row per snapshot group and stamps each `tabs` row with `tab_group_id`.

### Backwards-compatible read

`read_sqlite_data` implements one backfill rule:

> If a window has rows in `tabs` but **zero** rows in `tab_groups`, synthesize one `TabGroupSnapshot { name: "Workspace 1", tabs: <all tabs of that window>, active_tab_index: window.active_tab_index }` and set `active_tab_group_index = 0`.

This is the only reason the feature ships without a forced migration. It runs on every load; once `save_app_state` writes a real `tab_groups` row, subsequent loads skip the backfill naturally.

`WindowSnapshot::PartialEq` is regenerated; snapshot tests in `app/src/persistence/sqlite_tests.rs` get a fixture for the synthesized-group case.

## 6. Registry and lifecycle (phase 3)

`WorkspaceRegistry` becomes:

```rust
pub struct WorkspaceRegistry {
    by_window: HashMap<WindowId, Vec<TabGroupId>>,
    by_group: HashMap<TabGroupId, WeakViewHandle<Workspace>>,
    active_group: HashMap<WindowId, TabGroupId>,
}
```

Existing callers (`get(WindowId)`, `all_workspaces`) keep working: `get(window_id)` resolves to the active group's handle. New methods: `groups_for_window`, `group(TabGroupId)`, `set_active_group(WindowId, TabGroupId)`.

New `WorkspaceAction` variants: `ActivateTabGroup(TabGroupId)`, `NewTabGroup`, `CloseTabGroup(TabGroupId)`, `RenameTabGroup(TabGroupId)`, `MoveTabToGroup { tab_index, target }`.

Keybindings: `cmd-shift-1..9` → `ActivateTabGroup(nth)`, `cmd-shift-t` → `NewTabGroup`. `cmd-1..9` continue to drive `ActivateTab`. Cross-window tab drag is unchanged; the new switcher-drop target dispatches `MoveTabToGroup` directly rather than entering `CrossWindowTabDrag`.

Closing the last group in a window synthesizes a fresh "Workspace 1" before the close completes; closing the last tab in a group leaves the (empty) group active. These rules live in `Workspace::on_tab_group_closed` / `on_tab_closed` to keep `WorkspaceRegistry` mechanism-only.

## 7. UI layer (phase 4)

`TabGroupSwitcherPanel` is a new view under `app/src/workspace/view/tab_group_switcher.rs`, mirroring `vertical_tabs.rs` so the team only learns one panel pattern. Width is fixed (~48 px); right-click exposes the `*TabGroup` actions. The horizontal tab bar and vertical tabs panel read `Workspace::active_tab_group_state()` instead of `self.tabs`. The tab bar's drop targets gain switcher entries so dragging a tab onto another group's row dispatches `MoveTabToGroup`.

A new setting `tab_group_switcher_position: Left | Right` mirrors `vertical_tabs_panel_position`. Default `Left`.

## 8. Migration strategy

- **Phase 1** is additive only: a new table and a nullable column. Every legacy query keeps compiling and returning the same results. No data migrated.
- **Phase 2** introduces the `tab_groups` writer. The first save after upgrade writes one row per existing window. Reads use the backfill rule.
- **Downgrade** to a previous build is safe: legacy code ignores `tab_groups` entirely; new rows become unreferenced but harmless.

## 9. Phased plan

| Phase | Scope | AppState? | UI? |
|-------|-------|-----------|-----|
| 1 (this PR) | Schema + Diesel model + tests | No | No |
| 2 | `WindowSnapshot.tab_groups`, `TabGroupSnapshot`, save/load + backfill in `sqlite.rs`, snapshot tests | Yes | No |
| 3 | Registry refactor, `WorkspaceAction::*TabGroup`, lifecycle rules, keybindings | Yes | CLI only |
| 4 | `TabGroupSwitcherPanel`, tab bar filtering, drag onto switcher, "Move to Workspace" submenu, position setting | Yes | Yes |
| 5 | Live metadata per group (branch, PR, ports, badge), aggressive caching (lesson cmux #2746); evaluate worktree auto-binding | Yes | Yes |

Each phase is independently shippable. Phase 1 ships with no flag (pure schema). Phases 2–3 ship dark — backfill plus an always-1 group. Phase 4 ships behind `FeatureFlag::TabGroups` and graduates per the standard rollout.

## 10. Open questions and trade-offs

- **One `Workspace` view per window vs per group**: phase 3 keeps one view per window and swaps which group it renders. Per-group views simplify lifecycle but multiply pane subscriptions and complicate cross-window tab drag (which keys on `WindowId`). Revisit only if metadata phase exposes contention.
- **Switcher in fullscreen / quake mode**: not collapsed by default; user toggles via existing panel-toggle pattern. Decision deferred to phase 4 reviewer.
- **`Cmd+Shift+9` when fewer than 9 groups exist**: no-op, matching today's `Cmd+9` for tabs.
- **Persistence ordering**: `tab_groups.position` persisted explicitly so a reorder-via-drag feature lands without a schema change.

## 11. Risks

- **Snapshot equality regressions**: `WindowSnapshot::PartialEq` flows into `crash_recovery` and `save_app` debouncing; a missed field can cause excessive saves. Mitigation: derive plus targeted unit tests in `sqlite_tests.rs`.
- **Cross-window tab drag interaction**: the existing singleton (`CrossWindowTabDrag`) keys on `WindowId`. Moving a tab between groups in the same window must NOT enter that state machine. Mitigation: phase 4 wires `MoveTabToGroup` as a distinct in-window code path; integration tests cover both gestures in sequence.
- **Backfill misfire**: writing a synthesized group back to disk before phase 2 ships would create rows the legacy reader ignores but the writer assumes are canonical. Mitigation: phase 1 only adds schema; the writer change is exclusive to phase 2.
- **Keybinding collisions**: third-party keymaps may already bind `Cmd+Shift+1..9`. Mitigation: register through the standard keybinding layer so users can rebind; document in release notes.
- **Phase 5 metadata cost**: live branch / PR polling per group can multiply traffic. Mitigation explicitly called out: aggressive cache, share fetchers across groups bound to the same repo, mirror cmux's lesson #2746 (debounce + soft-TTL).
