CREATE TABLE IF NOT EXISTS recent_paths (
    working_directory TEXT NOT NULL,
    path              TEXT NOT NULL,
    last_accessed     INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (working_directory, path)
);

CREATE INDEX IF NOT EXISTS idx_recent_paths_wd_time
    ON recent_paths (working_directory, last_accessed DESC);
