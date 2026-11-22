-- M2: rooms, rhythm, condition, steps, started tasks.

-- "Categories" become rooms (KONZEPT §7.1).
ALTER TABLE categories RENAME TO rooms;
ALTER TABLE rooms ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE task_templates RENAME COLUMN category_id TO room_id;
ALTER TABLE tasks RENAME COLUMN category_id TO room_id;

-- Templates: typed schedule (punkte_core::Schedule) instead of a raw RRULE, steps, icon.
ALTER TABLE task_templates
    DROP COLUMN rrule,
    ADD COLUMN schedule JSONB NOT NULL DEFAULT '{"type":"once"}',
    ADD COLUMN step_order TEXT NOT NULL DEFAULT 'free' CHECK (step_order IN ('free', 'sequential')),
    ADD COLUMN icon TEXT;

CREATE TABLE template_steps (
    id              UUID PRIMARY KEY,
    template_id     UUID NOT NULL REFERENCES task_templates(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    title           TEXT NOT NULL,
    UNIQUE (template_id, position)
);

-- Tasks: current round (cycle_start → due_at), started state, check-in time, steps.
ALTER TABLE tasks DROP CONSTRAINT tasks_status_check;
ALTER TABLE tasks ADD CONSTRAINT tasks_status_check
    CHECK (status IN ('open', 'claimed', 'in_progress', 'submitted', 'completed', 'cancelled'));
ALTER TABLE tasks
    ADD COLUMN cycle_start TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN started_at TIMESTAMPTZ,
    ADD COLUMN remind_at TIMESTAMPTZ,
    ADD COLUMN duration_min INTEGER,
    ADD COLUMN step_order TEXT NOT NULL DEFAULT 'free' CHECK (step_order IN ('free', 'sequential')),
    ADD COLUMN icon TEXT;

CREATE TABLE task_steps (
    id              UUID PRIMARY KEY,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    title           TEXT NOT NULL,
    done_by         UUID REFERENCES memberships(id),
    done_at         TIMESTAMPTZ,
    UNIQUE (task_id, position)
);

-- At most one current (non-final) round per template.
CREATE UNIQUE INDEX tasks_one_current_round ON tasks (template_id)
    WHERE template_id IS NOT NULL AND status NOT IN ('completed', 'cancelled');

CREATE INDEX tasks_household_completed ON tasks (household_id, completed_at) WHERE status = 'completed';
