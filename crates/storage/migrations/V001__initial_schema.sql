PRAGMA foreign_keys = ON;

CREATE TABLE persons (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE families (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE places (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE sources (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE citations (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE repositories (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE media (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE sandboxes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    parent_sandbox TEXT,
    status TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE assertions (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    field TEXT NOT NULL,
    value JSON NOT NULL,
    value_date TEXT,
    value_text TEXT,
    confidence REAL NOT NULL,
    status TEXT NOT NULL,
    preferred INTEGER NOT NULL DEFAULT 0,
    source_citations JSON,
    proposed_by TEXT NOT NULL,
    reviewed_by TEXT,
    created_at TEXT NOT NULL,
    reviewed_at TEXT,
    evidence_type TEXT NOT NULL DEFAULT 'direct',
    idempotency_key TEXT UNIQUE,
    sandbox_id TEXT REFERENCES sandboxes(id)
);

CREATE INDEX idx_assertions_entity_field ON assertions(entity_id, field);
CREATE INDEX idx_assertions_date ON assertions(value_date) WHERE value_date IS NOT NULL;
CREATE INDEX idx_assertions_status ON assertions(status);
CREATE INDEX idx_assertions_confidence ON assertions(entity_id, field, confidence DESC);
CREATE INDEX idx_assertions_sandbox ON assertions(sandbox_id);

CREATE TABLE relationships (
    id TEXT PRIMARY KEY,
    from_entity TEXT NOT NULL,
    from_type TEXT NOT NULL,
    to_entity TEXT NOT NULL,
    to_type TEXT NOT NULL,
    rel_type TEXT NOT NULL,
    assertion_id TEXT REFERENCES assertions(id),
    directed INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_relationships_from_type ON relationships(from_entity, from_type, rel_type);
CREATE INDEX idx_relationships_to_type ON relationships(to_entity, to_type, rel_type);

CREATE VIRTUAL TABLE search_index USING fts5(
    entity_id,
    entity_type,
    content,
    tokenize='porter unicode61'
);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    actor TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    action TEXT NOT NULL,
    old_value JSON,
    new_value JSON
);

CREATE TABLE event_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    entity_id TEXT,
    entity_type TEXT,
    payload JSON NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_event_log_type_time ON event_log(event_type, timestamp);

CREATE TABLE research_log (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,
    objective TEXT NOT NULL,
    repository_id TEXT REFERENCES repositories(id),
    repository_name TEXT,
    search_terms JSON NOT NULL,
    source_id TEXT REFERENCES sources(id),
    result TEXT NOT NULL,
    findings TEXT,
    citations_created JSON,
    next_steps TEXT,
    person_refs JSON,
    tags JSON
);

CREATE INDEX idx_research_log_date ON research_log(date);
CREATE INDEX idx_research_log_result ON research_log(result);

CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    api_key_hash TEXT NOT NULL,
    registered_at TEXT NOT NULL,
    last_seen_at TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    config JSON
);
