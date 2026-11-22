-- M1: accounts, sessions, avatars.

-- Animal avatar per membership (KONZEPT §7.5); the same animal may appear twice in a household.
ALTER TABLE memberships
    ADD COLUMN animal TEXT NOT NULL DEFAULT 'fox'
        CHECK (animal IN ('fox', 'owl', 'otter', 'hedgehog', 'bear', 'cat', 'dog', 'whale', 'lion')),
    -- Argon2id hash of a child's 4-digit PIN for the family tablet (NULL = no PIN login)
    ADD COLUMN pin_hash TEXT;

ALTER TABLE sessions
    ADD COLUMN last_used_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- E-mail addresses are stored lowercased; enforce it so lookups stay simple.
ALTER TABLE users
    ADD CONSTRAINT users_email_lowercase CHECK (email IS NULL OR email = lower(email));

CREATE INDEX sessions_user ON sessions (user_id) WHERE revoked_at IS NULL;
CREATE INDEX invites_household ON invites (household_id);
