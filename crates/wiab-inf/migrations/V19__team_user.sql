-- A team authenticates to the backend as its own user. Required, but added permissively
-- first: an existing row has no value to give.
ALTER TABLE team ADD COLUMN user_id TEXT NOT NULL DEFAULT '';
ALTER TABLE team ALTER COLUMN user_id DROP DEFAULT;
