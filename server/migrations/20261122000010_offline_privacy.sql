-- M7: idempotent writes for the offline queue, account deletion.

-- The app sends queued writes with an Idempotency-Key; a repeated request gets the stored answer.
CREATE TABLE idempotency_keys (
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key             TEXT NOT NULL,
    method          TEXT NOT NULL,
    path            TEXT NOT NULL,
    status          SMALLINT NOT NULL,
    body            BYTEA NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, key)
);
CREATE INDEX idempotency_keys_age ON idempotency_keys (created_at);

-- Deleted accounts stay as anonymous rows (the ledger and history point to them).
ALTER TABLE users ADD COLUMN deleted_at TIMESTAMPTZ;
