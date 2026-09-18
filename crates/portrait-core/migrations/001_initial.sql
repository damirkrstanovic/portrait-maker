CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('folder', 'archive', 'game')),
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    original_location TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS portraits (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    original_folder TEXT NOT NULL,
    description TEXT,
    provenance TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    trashed_at TEXT
) STRICT;

-- Model-generated analysis deliberately lives apart from the user's note above.
-- Re-running a model can replace this row without disturbing user-authored data.
CREATE TABLE IF NOT EXISTS portrait_analysis (
    portrait_id TEXT PRIMARY KEY NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    analyzed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- The primary source remains on portraits for compatibility and display. Additional
-- sources are retained when exact duplicates are skipped or consolidated.
CREATE TABLE IF NOT EXISTS portrait_sources (
    portrait_id TEXT NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    PRIMARY KEY (portrait_id, source_id)
) STRICT;

CREATE TABLE IF NOT EXISTS assets (
    portrait_id TEXT NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('small', 'medium', 'large')),
    relative_path TEXT NOT NULL CHECK (
        length(relative_path) > 0
        AND substr(relative_path, 1, 1) <> '/'
        AND substr(relative_path, -1, 1) <> '/'
        AND instr(relative_path, '//') = 0
        AND instr(relative_path, char(92)) = 0
        AND instr(relative_path, ':') = 0
        AND instr(relative_path, char(0)) = 0
        AND relative_path NOT IN ('.', '..')
        AND relative_path NOT LIKE './%'
        AND relative_path NOT LIKE '../%'
        AND relative_path NOT LIKE '%/./%'
        AND relative_path NOT LIKE '%/../%'
        AND relative_path NOT LIKE '%/.'
        AND relative_path NOT LIKE '%/..'
    ),
    width INTEGER NOT NULL CHECK (width > 0),
    height INTEGER NOT NULL CHECK (height > 0),
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    PRIMARY KEY (portrait_id, role),
    UNIQUE (relative_path)
) STRICT;

CREATE TABLE IF NOT EXISTS labels (
    id INTEGER PRIMARY KEY,
    category TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    display_value TEXT NOT NULL,
    UNIQUE (category, normalized_value)
) STRICT;

CREATE TABLE IF NOT EXISTS portrait_labels (
    portrait_id TEXT NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    origin TEXT NOT NULL CHECK (origin IN ('filename', 'user', 'model')),
    producer TEXT,
    producer_version TEXT,
    provenance TEXT,
    PRIMARY KEY (portrait_id, label_id)
) STRICT;

CREATE TABLE IF NOT EXISTS suppressed_inferred_labels (
    portrait_id TEXT NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    producer TEXT NOT NULL,
    producer_version TEXT NOT NULL,
    suppressed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (portrait_id, category, normalized_value, producer, producer_version)
) STRICT;

CREATE TABLE IF NOT EXISTS user_label_suppressions (
    portrait_id TEXT NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    suppressed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (portrait_id, category, normalized_value)
) STRICT;

CREATE TABLE IF NOT EXISTS selection (
    portrait_id TEXT PRIMARY KEY NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    selected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE IF NOT EXISTS search_documents (
    id INTEGER PRIMARY KEY,
    portrait_id TEXT NOT NULL UNIQUE REFERENCES portraits(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    original_folder TEXT NOT NULL,
    source_name TEXT NOT NULL,
    labels TEXT NOT NULL,
    description TEXT NOT NULL
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    name,
    original_folder,
    source_name,
    labels,
    description,
    content='search_documents',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS operation_state (
    operation_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    state_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- Performance-only caches. Missing entries are recomputed from managed PNG files.
CREATE TABLE IF NOT EXISTS portrait_fingerprints (
    portrait_id TEXT PRIMARY KEY NOT NULL REFERENCES portraits(id) ON DELETE CASCADE,
    algorithm TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    small_stamp TEXT NOT NULL,
    medium_stamp TEXT NOT NULL,
    large_stamp TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS pixel_fingerprints (
    encoded_sha256 TEXT NOT NULL,
    renderer TEXT NOT NULL,
    pixel_hash TEXT NOT NULL,
    width INTEGER NOT NULL CHECK (width > 0),
    height INTEGER NOT NULL CHECK (height > 0),
    PRIMARY KEY (encoded_sha256, renderer)
) STRICT;

CREATE INDEX IF NOT EXISTS portraits_source_id_idx ON portraits(source_id);
CREATE INDEX IF NOT EXISTS portrait_sources_source_id_idx ON portrait_sources(source_id);
CREATE INDEX IF NOT EXISTS portraits_trashed_at_idx ON portraits(trashed_at);
CREATE INDEX IF NOT EXISTS portrait_labels_label_id_idx ON portrait_labels(label_id);

INSERT OR IGNORE INTO operation_state (operation_id, kind, state_json)
VALUES ('catalog_revision', 'catalog_revision', '{"revision":0}');
