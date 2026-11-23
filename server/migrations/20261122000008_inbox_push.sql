-- M5: in-app inbox, push devices, reminders.

-- Inbox entries know their household (one person can be in several) and when push went out.
ALTER TABLE notifications
    ADD COLUMN household_id UUID REFERENCES households(id) ON DELETE CASCADE,
    ADD COLUMN category TEXT NOT NULL DEFAULT 'tasks'
        CHECK (category IN ('tasks', 'reviews', 'points', 'period', 'reminders')),
    ADD COLUMN push_sent_at TIMESTAMPTZ;
CREATE INDEX notifications_member ON notifications (member_id, created_at DESC);

-- One row per push token; a token moves with the app reinstall, never belongs to two users.
ALTER TABLE devices ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE UNIQUE INDEX devices_push_token ON devices (push_token) WHERE push_token IS NOT NULL;

-- Reminders for the person responsible: "bald fällig", "überfällig", then admins.
ALTER TABLE tasks
    ADD COLUMN due_notified_at TIMESTAMPTZ,
    ADD COLUMN overdue_notified_at TIMESTAMPTZ,
    ADD COLUMN escalated_at TIMESTAMPTZ;
