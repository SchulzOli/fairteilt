-- M1: child profiles (no e-mail), PIN lockout.

ALTER TABLE memberships
    ADD COLUMN pin_failed_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN pin_locked_until TIMESTAMPTZ;

-- Children without e-mail have neither e-mail nor password; everyone else needs both.
ALTER TABLE users
    ADD CONSTRAINT users_login_complete CHECK ((email IS NULL) = (password_hash IS NULL));
