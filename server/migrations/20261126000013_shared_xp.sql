-- Several people on one task: whoever joins ("+") is a helper; all on the task share its XP.
CREATE TABLE task_helpers (
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    member_id  UUID NOT NULL REFERENCES memberships(id) ON DELETE CASCADE,
    joined_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (task_id, member_id)
);

-- A task is credited at most once per person (shared XP books one row per person).
DROP INDEX ledger_one_task_reward;
CREATE UNIQUE INDEX ledger_one_task_reward ON ledger_entries (ref_id, member_id) WHERE kind = 'task_reward';

-- Bonus XP on tasks (no Guthaben any more): the monthly allowance counts what each member gave.
CREATE INDEX bounties_pledger_month ON bounties (pledger_id, created_at);
