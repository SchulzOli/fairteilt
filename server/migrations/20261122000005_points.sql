-- M3: points ledger in use, approval with message, bounties, transfers.

-- Review of a submitted task: when, and the message to the person who did it.
ALTER TABLE tasks
    ADD COLUMN approved_at TIMESTAMPTZ,
    ADD COLUMN review_note TEXT;

-- A task is credited at most once.
CREATE UNIQUE INDEX ledger_one_task_reward ON ledger_entries (ref_id) WHERE kind = 'task_reward';

CREATE INDEX bounties_task ON bounties (task_id) WHERE status = 'held';

-- Transfers between members. Children's transfers can wait for an admin (KONZEPT §4.4).
CREATE TABLE transfers (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    from_id         UUID NOT NULL REFERENCES memberships(id),
    to_id           UUID NOT NULL REFERENCES memberships(id),
    amount          BIGINT NOT NULL CHECK (amount > 0),
    note            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'done' CHECK (status IN ('pending', 'done', 'declined')),
    decided_by      UUID REFERENCES memberships(id),
    decided_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (from_id <> to_id)
);
CREATE INDEX transfers_household ON transfers (household_id, created_at);
