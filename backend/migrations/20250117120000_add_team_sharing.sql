PRAGMA foreign_keys = ON;

-- Users table to store user information
CREATE TABLE users (
    id            BLOB PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    email         TEXT NOT NULL UNIQUE,
    display_name  TEXT,
    github_login  TEXT UNIQUE,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

-- Teams table for organizing users
CREATE TABLE teams (
    id          BLOB PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_by  BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (created_by) REFERENCES users(id)
);

-- Team members junction table
CREATE TABLE team_members (
    id         BLOB PRIMARY KEY,
    team_id    BLOB NOT NULL,
    user_id    BLOB NOT NULL,
    role       TEXT NOT NULL DEFAULT 'member'
                  CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    joined_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (team_id) REFERENCES teams(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE(team_id, user_id)
);

-- Add owner_id and team_id to projects table
ALTER TABLE projects ADD COLUMN owner_id BLOB;
ALTER TABLE projects ADD COLUMN team_id BLOB;
ALTER TABLE projects ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private'
    CHECK (visibility IN ('private', 'team', 'public'));

-- Project permissions table for sharing projects with specific users
CREATE TABLE project_permissions (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL,
    user_id     BLOB NOT NULL,
    permission  TEXT NOT NULL DEFAULT 'view'
                   CHECK (permission IN ('view', 'edit', 'admin')),
    granted_by  BLOB NOT NULL,
    granted_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (granted_by) REFERENCES users(id),
    UNIQUE(project_id, user_id)
);

-- Add assigned_to field to tasks for task assignment
ALTER TABLE tasks ADD COLUMN assigned_to BLOB;
ALTER TABLE tasks ADD COLUMN created_by BLOB;

-- Add foreign key constraints for new columns
CREATE INDEX idx_projects_owner_id ON projects(owner_id);
CREATE INDEX idx_projects_team_id ON projects(team_id);
CREATE INDEX idx_tasks_assigned_to ON tasks(assigned_to);
CREATE INDEX idx_tasks_created_by ON tasks(created_by);
CREATE INDEX idx_team_members_user_id ON team_members(user_id);
CREATE INDEX idx_project_permissions_user_id ON project_permissions(user_id);

-- Team invitations table for pending invitations
CREATE TABLE team_invitations (
    id           BLOB PRIMARY KEY,
    team_id      BLOB NOT NULL,
    email        TEXT NOT NULL,
    invited_by   BLOB NOT NULL,
    token        TEXT NOT NULL UNIQUE,
    expires_at   TEXT NOT NULL,
    accepted_at  TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (team_id) REFERENCES teams(id) ON DELETE CASCADE,
    FOREIGN KEY (invited_by) REFERENCES users(id)
);

-- User sessions table for authentication
CREATE TABLE user_sessions (
    id         BLOB PRIMARY KEY,
    user_id    BLOB NOT NULL,
    token      TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_user_sessions_token ON user_sessions(token);
CREATE INDEX idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX idx_team_invitations_token ON team_invitations(token);