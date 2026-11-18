-- Owner-local project/workspace API fields, not transferable ticket fields.
CREATE TABLE g_checkout_owner(
 checkout_id TEXT PRIMARY KEY NOT NULL REFERENCES g_checkout(checkout_id),
 owner_id TEXT NOT NULL REFERENCES g_local_owner(owner_id),
 root_device TEXT NOT NULL,
 root_inode TEXT NOT NULL
) WITHOUT ROWID;
CREATE INDEX g_checkout_owner_owner ON g_checkout_owner(owner_id,checkout_id);
CREATE TABLE g_project_binding(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 project_id TEXT NOT NULL,
 root TEXT NOT NULL,
 PRIMARY KEY(checkout_id,project_id), UNIQUE(checkout_id,root)
) WITHOUT ROWID;
CREATE TABLE g_checkout_project(
 checkout_id TEXT PRIMARY KEY NOT NULL REFERENCES g_checkout(checkout_id),
 project_id TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE g_project_workspaces(
 owner_id TEXT NOT NULL REFERENCES g_local_owner(owner_id),
 id TEXT NOT NULL,
 project_id TEXT NOT NULL,
 path TEXT NOT NULL,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 PRIMARY KEY(owner_id,id), UNIQUE(owner_id,path,project_id)
) WITHOUT ROWID;
CREATE INDEX g_project_workspaces_path ON g_project_workspaces(owner_id,path);
-- No FK to a derived live project: archive/unresolved state must remain diagnosable.
