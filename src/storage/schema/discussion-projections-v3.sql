-- Derived views of native forum/control/attachment artifacts and typed descriptors.
-- Descriptor wire bytes are not defined by these SQL columns.
CREATE TABLE g_descriptor(
 scope_key TEXT NOT NULL, descriptor_hash TEXT NOT NULL,
 attachment_artifact_hash TEXT NOT NULL, target_hash TEXT NOT NULL,
 record_kind TEXT NOT NULL, schema_version INTEGER NOT NULL,
 status TEXT NOT NULL CHECK(status IN('valid','invalid','unsupported','unresolved','conflict')),
 PRIMARY KEY(scope_key,descriptor_hash,attachment_artifact_hash)
) WITHOUT ROWID;
CREATE INDEX g_descriptor_target ON g_descriptor(scope_key,target_hash,record_kind);
CREATE TABLE g_discussion_thread(
 scope_key TEXT NOT NULL, thread_id TEXT NOT NULL,
 kind TEXT CHECK(kind IN('chat','annotation')), title TEXT NOT NULL,
 state TEXT CHECK(state IN('open','resolved','archived')),
 visibility TEXT NOT NULL CHECK(visibility IN('shared','private')),
 status TEXT NOT NULL CHECK(status IN('valid','invalid','unsupported','unresolved','conflict')),
 PRIMARY KEY(scope_key,thread_id)
) WITHOUT ROWID;
CREATE TABLE g_discussion_message(
 scope_key TEXT NOT NULL, message_id TEXT NOT NULL, version_hash TEXT NOT NULL,
 thread_id TEXT NOT NULL, reply_version_hash TEXT, previous_version_hash TEXT,
 author TEXT NOT NULL, created_at TEXT NOT NULL, body_format TEXT NOT NULL,
 body BLOB NOT NULL,
 PRIMARY KEY(scope_key,version_hash)
) WITHOUT ROWID;
CREATE INDEX g_message_identity ON g_discussion_message(scope_key,message_id,version_hash);
CREATE INDEX g_message_thread ON g_discussion_message(scope_key,thread_id,created_at,version_hash);
CREATE TABLE g_message_context(
 scope_key TEXT NOT NULL, descriptor_hash TEXT NOT NULL,
 message_id TEXT NOT NULL, version_hash TEXT NOT NULL,
 agent_run_id TEXT, role TEXT CHECK(role IN('human','agent','tool')),
 PRIMARY KEY(scope_key,descriptor_hash,version_hash)
) WITHOUT ROWID;
CREATE TABLE g_message_context_target(
 scope_key TEXT NOT NULL, descriptor_hash TEXT NOT NULL,
 version_hash TEXT NOT NULL, target_id TEXT NOT NULL,
 PRIMARY KEY(scope_key,descriptor_hash,version_hash,target_id)
) WITHOUT ROWID;
CREATE TABLE g_discussion_target(
 scope_key TEXT NOT NULL, target_id TEXT NOT NULL, thread_id TEXT NOT NULL,
 attachment_artifact_hash TEXT NOT NULL,
 repository_id TEXT NOT NULL,
 target_kind TEXT NOT NULL CHECK(target_kind IN('repository','native_file','git_file','snapshot','entity')),
 checkin_hash TEXT, relative_path TEXT, content_hash TEXT,
 git_registry_id TEXT, git_hash_algorithm TEXT, git_commit_oid TEXT, git_blob_oid TEXT,
 captured_content_hash TEXT,
 snapshot_root_kind TEXT CHECK(snapshot_root_kind IN('repository','git')),
 entity_kind TEXT, entity_key TEXT, entity_ticket_k TEXT, entity_projection_version TEXT,
 selector_kind TEXT, selector_version INTEGER,
 selector_bytes BLOB,
 start_byte INTEGER CHECK(start_byte>=0), end_byte INTEGER,
 quote TEXT, prefix TEXT, suffix TEXT,
 CHECK(end_byte IS NULL OR (start_byte IS NOT NULL AND end_byte>=start_byte)),
 PRIMARY KEY(scope_key,target_id,thread_id,attachment_artifact_hash)
) WITHOUT ROWID;
CREATE INDEX g_target_native_file ON g_discussion_target(scope_key,checkin_hash,relative_path);
CREATE INDEX g_target_git_file ON g_discussion_target(scope_key,git_registry_id,git_commit_oid,relative_path);
CREATE INDEX g_target_entity ON g_discussion_target(scope_key,entity_kind,entity_key);
CREATE TABLE g_target_entity_member(
 scope_key TEXT NOT NULL, target_id TEXT NOT NULL, native_hash TEXT NOT NULL,
 PRIMARY KEY(scope_key,target_id,native_hash)
) WITHOUT ROWID;
CREATE TABLE g_suggestion(
 scope_key TEXT NOT NULL, suggestion_id TEXT NOT NULL, message_id TEXT NOT NULL,
 target_id TEXT NOT NULL, expected_content_hash TEXT NOT NULL,
 replacement_content_hash TEXT NOT NULL,
 decision TEXT CHECK(decision IN('proposed','approved','rejected')),
 status TEXT NOT NULL CHECK(status IN('valid','invalid','unsupported','unresolved','conflict')),
 PRIMARY KEY(scope_key,suggestion_id)
) WITHOUT ROWID;
CREATE UNIQUE INDEX g_suggestion_per_message ON g_suggestion(scope_key,message_id) WHERE status='valid';
CREATE TABLE g_suggestion_application(
 scope_key TEXT NOT NULL, application_id TEXT NOT NULL, suggestion_id TEXT NOT NULL,
 before_content_hash TEXT NOT NULL, after_content_hash TEXT NOT NULL,
 result_checkin_hash TEXT NOT NULL, actor TEXT NOT NULL, created_at TEXT NOT NULL,
 PRIMARY KEY(scope_key,application_id)
) WITHOUT ROWID;
CREATE INDEX g_suggestion_applied ON g_suggestion_application(scope_key,suggestion_id);
CREATE TABLE g_code_link(
 scope_key TEXT NOT NULL, link_id TEXT NOT NULL, attachment_artifact_hash TEXT NOT NULL,
 entity_kind TEXT NOT NULL, entity_key TEXT NOT NULL, entity_ticket_k TEXT NOT NULL,
 entity_projection_version TEXT NOT NULL, target_id TEXT NOT NULL,
 relationship TEXT NOT NULL,
 PRIMARY KEY(scope_key,link_id,attachment_artifact_hash)
) WITHOUT ROWID;
CREATE INDEX g_code_link_entity ON g_code_link(scope_key,entity_kind,entity_key);
CREATE INDEX g_code_link_target ON g_code_link(scope_key,target_id);
CREATE TABLE g_record_diagnostic(
 scope_key TEXT NOT NULL, descriptor_hash TEXT NOT NULL,
 carrier_hash TEXT NOT NULL, field TEXT NOT NULL, code TEXT NOT NULL, detail TEXT NOT NULL,
 PRIMARY KEY(scope_key,descriptor_hash,carrier_hash,field,code)
) WITHOUT ROWID;
CREATE VIEW discussion_thread AS SELECT * FROM g_discussion_thread;
CREATE VIEW discussion_message AS SELECT * FROM g_discussion_message;
CREATE VIEW message_context AS
 SELECT c.*,coalesce((SELECT json_group_array(target_id) FROM
  (SELECT target_id FROM g_message_context_target t
   WHERE t.scope_key=c.scope_key AND t.descriptor_hash=c.descriptor_hash
    AND t.version_hash=c.version_hash ORDER BY target_id)), '[]') AS target_ids
 FROM g_message_context c;
CREATE VIEW discussion_target AS SELECT * FROM g_discussion_target;
CREATE VIEW suggestion AS SELECT * FROM g_suggestion;
CREATE VIEW suggestion_application AS SELECT * FROM g_suggestion_application;
-- Owner-local bindings to established registry IDs; no parallel canonical registry record.
CREATE TABLE g_git_checkout(
 owner_id TEXT NOT NULL REFERENCES g_local_owner(owner_id),
 git_registry_id TEXT NOT NULL, path TEXT NOT NULL,
 PRIMARY KEY(owner_id,git_registry_id), UNIQUE(owner_id,path)
) WITHOUT ROWID;
-- Local delivery indexes are not canonical events, entity revisions or sync payloads.
CREATE TABLE g_delivery_lane(
 lane TEXT PRIMARY KEY NOT NULL CHECK(lane IN('shared','combined')),
 stream_id TEXT NOT NULL,
 next_sequence INTEGER NOT NULL CHECK(next_sequence>=1),
 minimum_sequence INTEGER NOT NULL CHECK(minimum_sequence>=1)
) WITHOUT ROWID;
CREATE TABLE g_delivery(
 lane TEXT NOT NULL REFERENCES g_delivery_lane(lane),
 sequence INTEGER NOT NULL CHECK(sequence>=1),
 kind TEXT NOT NULL CHECK(kind IN('canonical-change','invalidate')),
 change_hash TEXT,
 CHECK((kind='canonical-change' AND change_hash IS NOT NULL) OR
       (kind='invalidate' AND change_hash IS NULL)),
 PRIMARY KEY(lane,sequence)
) WITHOUT ROWID;
