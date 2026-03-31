-- ADR-004 Remediation: Split co-stored Family and Relationship entities
-- Originally both were stored in the `families` table, discriminated by JSON field presence.
-- This migration formalizes the separation into dedicated tables, improving schema integrity.
--
-- NOTE: This migration creates the new family_relationships table for future use.
-- Existing Relationship rows are NOT migrated here (they would fail deserialization due to JSON schema mismatch).
-- The next import will populate family_relationships with properly-formatted Relationship entities.
-- After verification, the legacy Relationship rows can be cleaned from `families` table via V004.

-- Create dedicated relationships table for Family-specific relationships 
CREATE TABLE family_relationships (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    schema_version INTEGER NOT NULL,
    data JSON NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Add index for efficient lookups
CREATE INDEX idx_family_relationships_id ON family_relationships(id);

