CREATE TABLE admin_credentials (
    id INTEGER PRIMARY KEY CHECK (id = 1) REFERENCES users(id),
    password_hash TEXT NOT NULL
);
