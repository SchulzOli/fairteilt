-- M8: shopping lists. Each list belongs to an "auf Abruf" task template ("Einkaufen") that a
-- non-empty list triggers. Adding items never earns XP.
CREATE TABLE shopping_lists (
    id UUID PRIMARY KEY,
    household_id UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    template_id UUID REFERENCES task_templates(id) ON DELETE SET NULL,
    position INT NOT NULL DEFAULT 0,
    archived BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX shopping_lists_household ON shopping_lists (household_id);

CREATE TABLE shopping_items (
    id UUID PRIMARY KEY,
    list_id UUID NOT NULL REFERENCES shopping_lists(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key TEXT NOT NULL,
    note TEXT,
    section TEXT NOT NULL DEFAULT 'other',
    urgent BOOLEAN NOT NULL DEFAULT false,
    added_by UUID REFERENCES memberships(id) ON DELETE SET NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- In the cart (while someone is shopping); cleared items move to shopping_history.
    checked_by UUID REFERENCES memberships(id) ON DELETE SET NULL,
    checked_at TIMESTAMPTZ
);
CREATE INDEX shopping_items_list ON shopping_items (list_id);

-- What the family buys, for suggestions ("Milch" with its section, most frequent first).
CREATE TABLE shopping_history (
    household_id UUID NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    section TEXT NOT NULL DEFAULT 'other',
    times INT NOT NULL DEFAULT 1,
    last_bought TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (household_id, key)
);

-- Existing households get their default list ("Supermarkt" + "Einkaufen") on first use.
ALTER TABLE households ADD COLUMN shopping_seeded BOOLEAN NOT NULL DEFAULT false;
