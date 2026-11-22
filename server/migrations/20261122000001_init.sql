-- Initial schema. See docs/KONZEPT.md §9.4.
-- Enum-like columns are TEXT with CHECK constraints: easier to extend than PG enums.
-- Every household-scoped table carries household_id so queries can always be scoped.

CREATE TABLE users (
    id              UUID PRIMARY KEY,
    email           TEXT UNIQUE,                    -- NULL for child profiles without e-mail
    password_hash   TEXT,                           -- Argon2id; NULL for child profiles (PIN/invite login)
    display_name    TEXT NOT NULL,
    locale          TEXT NOT NULL DEFAULT 'de-DE',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE households (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    timezone        TEXT NOT NULL DEFAULT 'Europe/Berlin',
    point_policy    JSONB NOT NULL,                 -- punkte_core::PointPolicy
    period_policy   JSONB NOT NULL,                 -- punkte_core::PeriodPolicy
    change_seq      BIGINT NOT NULL DEFAULT 0,      -- sync cursor (M7)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memberships (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('admin', 'member', 'child')),
    weight          DOUBLE PRECISION NOT NULL DEFAULT 1.0 CHECK (weight >= 0),
    color           TEXT NOT NULL DEFAULT '#4f7cff',
    active          BOOLEAN NOT NULL DEFAULT true,
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (household_id, user_id)
);

CREATE TABLE invites (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    code_hash       TEXT NOT NULL UNIQUE,
    role            TEXT NOT NULL CHECK (role IN ('admin', 'member', 'child')),
    created_by      UUID NOT NULL REFERENCES memberships(id),
    expires_at      TIMESTAMPTZ NOT NULL,
    used_by         UUID REFERENCES users(id),
    used_at         TIMESTAMPTZ
);

CREATE TABLE sessions (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_name     TEXT NOT NULL,
    refresh_hash    TEXT NOT NULL UNIQUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ
);

CREATE TABLE devices (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform        TEXT NOT NULL CHECK (platform IN ('android', 'ios', 'windows', 'macos', 'linux')),
    push_provider   TEXT CHECK (push_provider IN ('fcm', 'apns', 'unifiedpush')),
    push_token      TEXT,
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE categories (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    icon            TEXT,
    color           TEXT
);

CREATE TABLE task_templates (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    category_id     UUID REFERENCES categories(id) ON DELETE SET NULL,
    title           TEXT NOT NULL,
    description     TEXT,
    kind            TEXT NOT NULL DEFAULT 'regular' CHECK (kind IN ('regular', 'reward', 'penalty')),
    points_min      BIGINT NOT NULL CHECK (points_min > 0),
    points_default  BIGINT NOT NULL,
    points_max      BIGINT NOT NULL,
    duration_min    INTEGER,
    rrule           TEXT,                           -- RFC 5545, NULL = one-off template
    assignment      JSONB NOT NULL DEFAULT '{"type":"open"}',
    approval        TEXT NOT NULL DEFAULT 'household' CHECK (approval IN ('household', 'always', 'never', 'photo')),
    reminders       JSONB NOT NULL DEFAULT '[]',    -- offsets in seconds relative to due_at
    archived        BOOLEAN NOT NULL DEFAULT false,
    version         INTEGER NOT NULL DEFAULT 1,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (points_min <= points_default AND points_default <= points_max)
);

CREATE TABLE tasks (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    template_id     UUID REFERENCES task_templates(id) ON DELETE SET NULL,
    category_id     UUID REFERENCES categories(id) ON DELETE SET NULL,
    title           TEXT NOT NULL,
    description     TEXT,
    kind            TEXT NOT NULL DEFAULT 'regular' CHECK (kind IN ('regular', 'reward', 'penalty')),
    status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'claimed', 'submitted', 'completed', 'cancelled')),
    assignee_id     UUID REFERENCES memberships(id) ON DELETE SET NULL,
    due_at          TIMESTAMPTZ,
    scheduled_start TIMESTAMPTZ,                    -- calendar planning
    scheduled_end   TIMESTAMPTZ,
    points_min      BIGINT NOT NULL CHECK (points_min > 0),
    points_default  BIGINT NOT NULL,
    points_max      BIGINT NOT NULL,
    points_awarded  BIGINT,                         -- set on completion
    completed_by    UUID REFERENCES memberships(id),
    completed_at    TIMESTAMPTZ,
    approved_by     UUID REFERENCES memberships(id),
    version         INTEGER NOT NULL DEFAULT 1,
    created_by      UUID REFERENCES memberships(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (points_min <= points_default AND points_default <= points_max)
);
CREATE INDEX tasks_household_due ON tasks (household_id, due_at) WHERE status NOT IN ('completed', 'cancelled');
CREATE INDEX tasks_assignee ON tasks (assignee_id) WHERE status NOT IN ('completed', 'cancelled');

CREATE TABLE task_events (
    id              UUID PRIMARY KEY,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    actor_id        UUID REFERENCES memberships(id),
    type            TEXT NOT NULL,
    payload         JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX task_events_task ON task_events (task_id, created_at);

CREATE TABLE periods (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    starts_at       TIMESTAMPTZ NOT NULL,
    ends_at         TIMESTAMPTZ NOT NULL,
    status          TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'settling', 'settled')),
    UNIQUE (household_id, starts_at),
    CHECK (starts_at < ends_at)
);

CREATE TABLE bounties (
    id              UUID PRIMARY KEY,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    pledger_id      UUID NOT NULL REFERENCES memberships(id),
    amount          BIGINT NOT NULL CHECK (amount > 0),
    status          TEXT NOT NULL DEFAULT 'held' CHECK (status IN ('held', 'paid', 'refunded')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Append-only. Corrections are counter-entries. Balances = SUM over member_id.
CREATE TABLE ledger_entries (
    id                  UUID PRIMARY KEY,           -- client-generated → idempotent sync
    household_id        UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    member_id           UUID NOT NULL REFERENCES memberships(id),
    kind                TEXT NOT NULL CHECK (kind IN (
                            'task_reward', 'bounty_hold', 'bounty_payout', 'bounty_refund',
                            'transfer_out', 'transfer_in', 'reward_redeem', 'penalty_deduction',
                            'settlement_bonus', 'settlement_zero_sum', 'manual_adjust')),
    contribution_delta  BIGINT NOT NULL DEFAULT 0,
    balance_delta       BIGINT NOT NULL DEFAULT 0,
    ref_type            TEXT,
    ref_id              UUID,
    period_id           UUID REFERENCES periods(id),
    note                TEXT,
    created_by          UUID REFERENCES memberships(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ledger_member ON ledger_entries (member_id, created_at);
CREATE INDEX ledger_period ON ledger_entries (period_id, member_id);

CREATE TABLE settlements (
    id              UUID PRIMARY KEY,
    period_id       UUID NOT NULL UNIQUE REFERENCES periods(id) ON DELETE CASCADE,
    policy          JSONB NOT NULL,                 -- snapshot of the PeriodPolicy used
    status          TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved')),
    approved_by     UUID REFERENCES memberships(id),
    approved_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE settlement_lines (
    settlement_id       UUID NOT NULL REFERENCES settlements(id) ON DELETE CASCADE,
    member_id           UUID NOT NULL REFERENCES memberships(id),
    target              BIGINT NOT NULL,
    contribution        BIGINT NOT NULL,
    deviation           BIGINT NOT NULL,
    outcome             TEXT NOT NULL CHECK (outcome IN ('within_tolerance', 'deficit', 'surplus')),
    balance_delta       BIGINT NOT NULL DEFAULT 0,
    penalty_points_due  BIGINT NOT NULL DEFAULT 0,
    carry_next          BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (settlement_id, member_id)
);

CREATE TABLE absences (
    id              UUID PRIMARY KEY,
    member_id       UUID NOT NULL REFERENCES memberships(id) ON DELETE CASCADE,
    starts_on       DATE NOT NULL,
    ends_on         DATE NOT NULL,
    reason          TEXT,
    CHECK (starts_on <= ends_on)
);

CREATE TABLE rewards (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    description     TEXT,
    cost            BIGINT NOT NULL CHECK (cost > 0),
    needs_approval  BOOLEAN NOT NULL DEFAULT false,
    stock           INTEGER,                        -- NULL = unlimited
    cooldown_hours  INTEGER,
    archived        BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE reward_redemptions (
    id              UUID PRIMARY KEY,
    reward_id       UUID NOT NULL REFERENCES rewards(id),
    member_id       UUID NOT NULL REFERENCES memberships(id),
    status          TEXT NOT NULL DEFAULT 'requested' CHECK (status IN ('requested', 'approved', 'rejected', 'fulfilled')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE boards (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    owner_id        UUID REFERENCES memberships(id), -- NULL = shared with household
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE board_items (
    id              UUID PRIMARY KEY,
    board_id        UUID NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    type            TEXT NOT NULL CHECK (type IN ('task', 'zone', 'note')),
    task_id         UUID REFERENCES tasks(id) ON DELETE CASCADE,
    zone_binding    JSONB,                          -- e.g. {"assign":"<member>"} or {"schedule":"2026-11-23"}
    label           TEXT,
    x               DOUBLE PRECISION NOT NULL,
    y               DOUBLE PRECISION NOT NULL,
    w               DOUBLE PRECISION,
    h               DOUBLE PRECISION,
    z               INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE saved_views (
    id              UUID PRIMARY KEY,
    household_id    UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    owner_id        UUID NOT NULL REFERENCES memberships(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    shared          BOOLEAN NOT NULL DEFAULT false,
    filter          JSONB NOT NULL DEFAULT '{}',
    layout          TEXT NOT NULL DEFAULT 'list' CHECK (layout IN ('list', 'kanban', 'calendar', 'canvas')),
    sort            JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE notification_prefs (
    member_id       UUID PRIMARY KEY REFERENCES memberships(id) ON DELETE CASCADE,
    quiet_from      TIME,
    quiet_to        TIME,
    digest_time     TIME,
    channels        JSONB NOT NULL DEFAULT '{}'     -- event type → ["push","local","email"]
);

CREATE TABLE notifications (
    id              UUID PRIMARY KEY,
    member_id       UUID NOT NULL REFERENCES memberships(id) ON DELETE CASCADE,
    type            TEXT NOT NULL,
    title           TEXT NOT NULL,
    body            TEXT,
    ref_type        TEXT,
    ref_id          UUID,
    read_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX notifications_unread ON notifications (member_id, created_at) WHERE read_at IS NULL;

-- Background jobs: claimed with FOR UPDATE SKIP LOCKED by the scheduler loop.
CREATE TABLE jobs (
    id              UUID PRIMARY KEY,
    kind            TEXT NOT NULL,
    payload         JSONB NOT NULL DEFAULT '{}',
    run_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts        INTEGER NOT NULL DEFAULT 0,
    max_attempts    INTEGER NOT NULL DEFAULT 5,
    locked_until    TIMESTAMPTZ,
    last_error      TEXT,
    done_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX jobs_due ON jobs (run_at) WHERE done_at IS NULL;
