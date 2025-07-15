
CREATE TABLE task_attempts_new (
    id            BLOB PRIMARY KEY,
    task_id       BLOB NOT NULL,
    branch        TEXT NOT NULL DEFAULT '',
    base_branch   TEXT NOT NULL DEFAULT 'main',
    merge_commit  TEXT,
    executor      TEXT,
    pr_url        TEXT,
    pr_number     INTEGER,
    pr_status     TEXT,
    pr_merged_at  DATETIME,
    setup_completed_at DATETIME,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

INSERT INTO task_attempts_new (
    id, task_id, branch, base_branch, merge_commit, executor, 
    pr_url, pr_number, pr_status, pr_merged_at, setup_completed_at, 
    created_at, updated_at
)
SELECT 
    id, task_id, branch, base_branch, merge_commit, executor,
    pr_url, pr_number, pr_status, pr_merged_at, setup_completed_at,
    created_at, updated_at
FROM task_attempts;

DROP TABLE task_attempts;

ALTER TABLE task_attempts_new RENAME TO task_attempts;
