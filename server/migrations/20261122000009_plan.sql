-- M6: calendar planning, board positions, saved views.

-- Board cards follow a recurring task from round to round (template), one-off tasks by task.
ALTER TABLE board_items ADD COLUMN template_id UUID REFERENCES task_templates(id) ON DELETE CASCADE;
CREATE UNIQUE INDEX board_items_template ON board_items (board_id, template_id) WHERE template_id IS NOT NULL;
CREATE UNIQUE INDEX board_items_task ON board_items (board_id, task_id) WHERE template_id IS NULL AND task_id IS NOT NULL;

-- One shared board per household (owner NULL).
CREATE UNIQUE INDEX boards_shared ON boards (household_id) WHERE owner_id IS NULL;

ALTER TABLE saved_views ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX tasks_scheduled ON tasks (household_id, scheduled_start) WHERE scheduled_start IS NOT NULL;
