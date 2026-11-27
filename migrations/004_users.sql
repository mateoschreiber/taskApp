CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name TEXT NOT NULL,
    password_hash TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    temporary_until INTEGER,
    CHECK ((id = 1 AND username = 'admin' AND password_hash IS NULL)
        OR (id <> 1 AND username <> 'admin' AND password_hash IS NOT NULL))
);
INSERT INTO users (id, username, name) VALUES (1, 'admin', 'Administrador');

ALTER TABLE tasks ADD COLUMN owner_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
UPDATE tasks SET owner_id = 1;
CREATE INDEX tasks_by_owner_date ON tasks (owner_id, activity_date DESC, id DESC);

CREATE TABLE user_catalog (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (user_id, kind, key)
);

ALTER TABLE task_mutations RENAME TO legacy_task_mutations;
CREATE TABLE task_mutations (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,
    task_id INTEGER NOT NULL REFERENCES tasks(id),
    revision INTEGER NOT NULL,
    PRIMARY KEY (user_id, operation)
);
INSERT INTO task_mutations SELECT 1, operation, task_id, revision FROM legacy_task_mutations;
DROP TABLE legacy_task_mutations;

CREATE TABLE user_events (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    actor_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    action TEXT NOT NULL
);


CREATE TEMP TABLE initial_catalog (kind TEXT, key TEXT, label TEXT);
INSERT INTO initial_catalog (kind, key, label) VALUES
    ('store', 'encarnación cambyreta', 'ENCARNACIÓN CAMBYRETA'),
    ('store', 'encarnación entrada', 'ENCARNACIÓN ENTRADA'),
    ('store', 'encarnación lomas valentinas', 'ENCARNACIÓN LOMAS VALENTINAS'),
    ('store', 'encarnación maria auxiliadora', 'ENCARNACIÓN MARIA AUXILIADORA'),
    ('store', 'encarnación matiauda', 'ENCARNACIÓN MATIAUDA'),
    ('store', 'encarnación padre juan', 'ENCARNACIÓN PADRE JUAN'),
    ('store', 'san ignacio entrada', 'SAN IGNACIO ENTRADA'),
    ('store', 'san ignacio salida', 'SAN IGNACIO SALIDA'),
    ('store', 'san juan bautista', 'SAN JUAN BAUTISTA'),
    ('supervisor', 'florencia silvero', 'FLORENCIA SILVERO'),
    ('zone', 'interior', 'INTERIOR'),
    ('work', 'ajuste de exhibición de congelados', 'Ajuste de exhibición de congelados'),
    ('work', 'ajuste de exhibición de lácteos', 'Ajuste de exhibición de lácteos'),
    ('work', 'ajuste de exhibición de pastas', 'Ajuste de exhibición de pastas'),
    ('work', 'ajuste de exhibición de quesos y fiambres', 'Ajuste de exhibición de quesos y fiambres'),
    ('work', 'colocación de precios y flejes', 'Colocación de precios y flejes'),
    ('work', 'control de fyv y open', 'Control de FyV y open'),
    ('work', 'control de mermas', 'Control de mermas'),
    ('work', 'control de sencillo grande', 'Control de sencillo grande'),
    ('work', 'corrección de planogramas', 'Corrección de planogramas'),
    ('work', 'limpieza de góndolas, exhibidores y muebles', 'Limpieza de góndolas, exhibidores y muebles'),
    ('work', 'limpieza de heladeras y equipos de frío', 'Limpieza de heladeras y equipos de frío'),
    ('work', 'limpieza en farma', 'Limpieza en farma'),
    ('work', 'limpieza general de tienda', 'Limpieza general de tienda'),
    ('work', 'limpieza sector helados', 'Limpieza sector helados'),
    ('work', 'organización de depósito', 'Organización de depósito'),
    ('work', 'organización de góndolas', 'Organización de góndolas'),
    ('work', 'reubicación de equipos', 'Reubicación de equipos'),
    ('work', 'seguimiento de reclamos y mantenimientos', 'Seguimiento de reclamos y mantenimientos'),
    ('intervention', 'instrucción al supervisor de tienda', 'Instrucción al supervisor de tienda'),
    ('intervention', 'seguimiento', 'Seguimiento'),
    ('status', 'en proceso', 'En proceso'),
    ('status', 'pendiente de ejecución', 'Pendiente de ejecución');

INSERT INTO user_catalog
SELECT 1, kind, key, label FROM catalog AS saved
WHERE NOT EXISTS (SELECT 1 FROM initial_catalog AS initial WHERE initial.kind = saved.kind AND initial.key = saved.key);
DELETE FROM catalog WHERE NOT EXISTS (SELECT 1 FROM initial_catalog AS initial WHERE initial.kind = catalog.kind AND initial.key = catalog.key);
DROP TABLE initial_catalog;
