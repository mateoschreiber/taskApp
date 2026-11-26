CREATE TABLE catalog (
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (kind, key)
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    activity_date TEXT NOT NULL,
    store TEXT NOT NULL,
    supervisor TEXT NOT NULL,
    zone TEXT NOT NULL,
    work TEXT NOT NULL,
    intervention TEXT NOT NULL,
    status TEXT NOT NULL,
    notes TEXT NOT NULL
);

CREATE INDEX tasks_by_date ON tasks (activity_date DESC, id DESC);
