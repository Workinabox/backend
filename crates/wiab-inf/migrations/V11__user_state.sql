-- User lifecycle state: 'active' (the default for everyone existing), 'pending' (invited
-- but not accepted, or signed up but email not yet confirmed), or 'deactivated'. Only
-- 'active' users may authenticate.
ALTER TABLE app_user ADD COLUMN state TEXT NOT NULL DEFAULT 'active';
