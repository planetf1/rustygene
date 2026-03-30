CREATE TABLE lds_ordinances (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
