-- Adds the cmux-style "Workspace" primitive (called `tab_groups` internally to
-- avoid colliding with the existing `workspaces` (SaaS), `workspace::view::Workspace`
-- (UI shell) and `persistence::Workspace` types). See specs/cmux-workspaces/.
--
-- A tab_group is a sidebar entry inside a window that owns a subset of tabs,
-- carries optional display metadata (name, color), and tracks which tab is
-- active within the group. Windows that have no rows in tab_groups behave
-- exactly as before (single implicit group containing every tab).
CREATE TABLE tab_groups (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    window_id INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (window_id) REFERENCES windows (id) ON DELETE CASCADE
);

CREATE INDEX tab_groups_window_id_idx ON tab_groups (window_id);

-- Nullable on purpose: existing rows stay NULL (= ungrouped / default group),
-- and Phase 2 backfill happens lazily at read time, not in this migration.
ALTER TABLE tabs ADD COLUMN tab_group_id INTEGER REFERENCES tab_groups (id) ON DELETE SET NULL;
