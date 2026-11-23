-- M4: periods with settlement, family goal (race against Krümel), rewards.

ALTER TABLE households
    ADD COLUMN goal_policy JSONB NOT NULL DEFAULT '{"target":null,"reward":"Familienausflug","boost_percent":25}';

-- One family goal per period.
CREATE TABLE family_goals (
    period_id       UUID PRIMARY KEY REFERENCES periods(id) ON DELETE CASCADE,
    target          BIGINT NOT NULL CHECK (target > 0),
    reward          TEXT NOT NULL,
    boost_percent   INTEGER NOT NULL DEFAULT 25,
    status          TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'won', 'lost')),
    decided_at      TIMESTAMPTZ
);

-- At most one running period per household.
CREATE UNIQUE INDEX periods_one_running ON periods (household_id) WHERE status = 'running';

-- When a draft is approved automatically (NULL = only by hand).
ALTER TABLE settlements ADD COLUMN auto_approve_at TIMESTAMPTZ;

-- Catch-up tasks remember the settlement they came from.
ALTER TABLE tasks ADD COLUMN settlement_id UUID REFERENCES settlements(id) ON DELETE SET NULL;

ALTER TABLE rewards ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE reward_redemptions
    ADD COLUMN cost BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN decided_by UUID REFERENCES memberships(id),
    ADD COLUMN decided_at TIMESTAMPTZ;
CREATE INDEX reward_redemptions_member ON reward_redemptions (member_id, created_at);
