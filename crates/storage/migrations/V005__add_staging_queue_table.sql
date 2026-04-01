CREATE TABLE staging_queue (
    id TEXT PRIMARY KEY,
    assertion_id TEXT NOT NULL REFERENCES assertions(id),
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    field TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'proposed',
    submitted_at TEXT NOT NULL,
    submitted_by TEXT NOT NULL,
    reviewed_at TEXT,
    reviewed_by TEXT,
    review_note TEXT
);

CREATE INDEX idx_staging_queue_status_created ON staging_queue(status, submitted_at DESC);
CREATE INDEX idx_staging_queue_entity ON staging_queue(entity_id, entity_type);
