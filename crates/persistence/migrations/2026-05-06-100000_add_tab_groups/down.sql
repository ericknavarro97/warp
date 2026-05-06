-- Reverses 2026-05-06-100000_add_tab_groups/up.sql.
ALTER TABLE tabs DROP COLUMN tab_group_id;

DROP INDEX IF EXISTS tab_groups_window_id_idx;

DROP TABLE tab_groups;
