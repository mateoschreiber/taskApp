ALTER TABLE tasks ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;

CREATE TABLE task_mutations (
    operation TEXT PRIMARY KEY,
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    revision INTEGER NOT NULL
);
