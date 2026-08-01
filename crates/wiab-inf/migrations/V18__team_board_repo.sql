-- A team pulls from one board and works in one repo. Both are required, but the column has
-- to be added permissively first: an existing row has no value to give.
ALTER TABLE team ADD COLUMN board_id TEXT NOT NULL DEFAULT '';
ALTER TABLE team ALTER COLUMN board_id DROP DEFAULT;
ALTER TABLE team ADD COLUMN repo_id TEXT NOT NULL DEFAULT '';
ALTER TABLE team ALTER COLUMN repo_id DROP DEFAULT;
