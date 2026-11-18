-- Exact proposed compatibility projections; never canonical replay inputs.
CREATE TABLE g_gest_projects(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 "archived_at" TEXT,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE TABLE g_gest_authors(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "author_type" TEXT NOT NULL,
 "name" TEXT NOT NULL,
 "email" TEXT,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE UNIQUE INDEX g_idx_authors_name_email ON g_gest_authors(scope_key,name,email);
CREATE TABLE g_gest_tasks(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "project_id" TEXT NOT NULL ,
 "title" TEXT NOT NULL,
 "priority" INTEGER,
 "status" TEXT NOT NULL,
 "description" TEXT NOT NULL,
 "assigned_to" TEXT ,
 "metadata" TEXT NOT NULL,
 "resolved_at" TEXT,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE INDEX g_idx_tasks_project_id ON g_gest_tasks(scope_key,project_id);
CREATE INDEX g_idx_tasks_status ON g_gest_tasks(scope_key,project_id,status);
CREATE TABLE g_gest_artifacts(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "project_id" TEXT NOT NULL ,
 "title" TEXT NOT NULL,
 "body" TEXT NOT NULL,
 "metadata" TEXT NOT NULL,
 "archived_at" TEXT,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE INDEX g_idx_artifacts_project_id ON g_gest_artifacts(scope_key,project_id);
CREATE TABLE g_gest_iterations(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "project_id" TEXT NOT NULL ,
 "title" TEXT NOT NULL,
 "status" TEXT NOT NULL,
 "description" TEXT NOT NULL,
 "metadata" TEXT NOT NULL,
 "completed_at" TEXT,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE INDEX g_idx_iterations_project_id ON g_gest_iterations(scope_key,project_id);
CREATE INDEX g_idx_iterations_status ON g_gest_iterations(scope_key,project_id,status);
CREATE TABLE g_gest_tags(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "label" TEXT NOT NULL,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id),
 UNIQUE(scope_key,"label")
) WITHOUT ROWID;
CREATE TABLE g_gest_entity_tags(
 scope_key TEXT NOT NULL,
 "entity_id" TEXT NOT NULL,
 "entity_type" TEXT NOT NULL,
 "tag_id" TEXT NOT NULL ,
 "created_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,entity_id,entity_type,tag_id)
) WITHOUT ROWID;
CREATE INDEX g_idx_entity_tags_tag_id ON g_gest_entity_tags(scope_key,tag_id);
CREATE TABLE g_gest_notes(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "entity_id" TEXT NOT NULL,
 "entity_type" TEXT NOT NULL,
 "author_id" TEXT ,
 "body" TEXT NOT NULL,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE INDEX g_idx_notes_entity ON g_gest_notes(scope_key,entity_type,entity_id);
CREATE TABLE g_gest_relationships(
 scope_key TEXT NOT NULL,
 "id" TEXT NOT NULL,
 "rel_type" TEXT NOT NULL,
 "source_id" TEXT NOT NULL,
 "source_type" TEXT NOT NULL,
 "target_id" TEXT NOT NULL,
 "target_type" TEXT NOT NULL,
 "created_at" TEXT NOT NULL,
 "updated_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,id)
) WITHOUT ROWID;
CREATE INDEX g_idx_relationships_source ON g_gest_relationships(scope_key,source_type,source_id);
CREATE INDEX g_idx_relationships_target ON g_gest_relationships(scope_key,target_type,target_id);
CREATE UNIQUE INDEX g_idx_relationships_unique ON g_gest_relationships(scope_key,rel_type,source_type,source_id,target_type,target_id);
CREATE TABLE g_gest_iteration_tasks(
 scope_key TEXT NOT NULL,
 "iteration_id" TEXT NOT NULL ,
 "task_id" TEXT NOT NULL ,
 "phase" INTEGER NOT NULL,
 "created_at" TEXT NOT NULL,
 PRIMARY KEY(scope_key,iteration_id,task_id)
) WITHOUT ROWID;
CREATE INDEX g_idx_iteration_tasks_task_id ON g_gest_iteration_tasks(scope_key,task_id);
