-- Phase 1B: generated/indexed hot query columns for person filtering/sorting
-- SQLite allows ADD COLUMN with VIRTUAL generated columns; these stay in sync
-- with `data` JSON after snapshot recomputation.

ALTER TABLE persons
ADD COLUMN birth_year INTEGER GENERATED ALWAYS AS (
  CASE
    WHEN json_type(data, '$.birth_date') = 'object'
      THEN CAST(json_extract(data, '$.birth_date.date.year') AS INTEGER)
    WHEN json_type(data, '$.birth_date') = 'text'
      THEN CAST(substr(json_extract(data, '$.birth_date'), 1, 4) AS INTEGER)
    ELSE NULL
  END
) VIRTUAL;

ALTER TABLE persons
ADD COLUMN death_year INTEGER GENERATED ALWAYS AS (
  CASE
    WHEN json_type(data, '$.death_date') = 'object'
      THEN CAST(json_extract(data, '$.death_date.date.year') AS INTEGER)
    WHEN json_type(data, '$.death_date') = 'text'
      THEN CAST(substr(json_extract(data, '$.death_date'), 1, 4) AS INTEGER)
    ELSE NULL
  END
) VIRTUAL;

ALTER TABLE persons
ADD COLUMN primary_surname TEXT GENERATED ALWAYS AS (
  json_extract(data, '$.name.surnames[0].value')
) VIRTUAL;

ALTER TABLE persons
ADD COLUMN primary_given_name TEXT GENERATED ALWAYS AS (
  json_extract(data, '$.name.given_names')
) VIRTUAL;

CREATE INDEX idx_persons_birth_year ON persons(birth_year);
CREATE INDEX idx_persons_death_year ON persons(death_year);
CREATE INDEX idx_persons_primary_surname ON persons(primary_surname);
CREATE INDEX idx_persons_primary_given_name ON persons(primary_given_name);
