-- Gossil STORAGE draft v1. Local tables are never xfer payloads.
-- Native content bytes remain Fossil objects; these are policy and indexes.
PRAGMA foreign_keys=ON;
CREATE TABLE g_storage(
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 repository_id TEXT NOT NULL UNIQUE,
 storage_version INTEGER NOT NULL CHECK(storage_version=1),
 projection_version TEXT NOT NULL CHECK(projection_version='gest-ticket-v1'),
 policy_complete INTEGER NOT NULL CHECK(policy_complete IN(0,1))
);
CREATE TABLE g_local_owner(
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 owner_id TEXT NOT NULL UNIQUE CHECK(length(owner_id)>0)
);
CREATE TABLE g_checkout(
 checkout_id TEXT PRIMARY KEY NOT NULL,
 root TEXT NOT NULL UNIQUE,
 shared_head TEXT NOT NULL,
 combined_head TEXT,
 generation INTEGER NOT NULL CHECK(generation>=0),
 pending_version INTEGER NOT NULL CHECK(pending_version>=0),
 availability TEXT NOT NULL CHECK(availability IN('ready','recovering','blocked'))
) WITHOUT ROWID;
CREATE TABLE g_path_scope(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 path TEXT NOT NULL,
 scope TEXT NOT NULL CHECK(scope IN('shared','private')),
 PRIMARY KEY(checkout_id,path)
) WITHOUT ROWID;
CREATE TABLE g_selector_revision(
 owner_id TEXT NOT NULL REFERENCES g_local_owner(owner_id), kind TEXT NOT NULL,
 ticket_k TEXT NOT NULL CHECK(length(ticket_k)=40 AND ticket_k NOT GLOB '*[^0-9a-f]*'),
 version INTEGER NOT NULL CHECK(version>=0),
 mode TEXT NOT NULL CHECK(mode IN('follow_shared','private_selection')),
 base_version INTEGER,
 CHECK((mode='follow_shared' AND base_version IS NULL) OR
       (mode='private_selection' AND base_version IS NOT NULL AND base_version<=version)),
 PRIMARY KEY(owner_id,kind,ticket_k,version),
 UNIQUE(owner_id,kind,ticket_k,version,mode),
 FOREIGN KEY(owner_id,kind,ticket_k,base_version)
  REFERENCES g_selector_revision(owner_id,kind,ticket_k,version)
) WITHOUT ROWID;
CREATE TABLE g_selector(
 owner_id TEXT NOT NULL, kind TEXT NOT NULL,
 ticket_k TEXT NOT NULL,
 mode TEXT NOT NULL CHECK(mode IN('follow_shared','private_selection')),
 version INTEGER NOT NULL CHECK(version>=0),
 PRIMARY KEY(owner_id,kind,ticket_k),
 FOREIGN KEY(owner_id,kind,ticket_k,version,mode)
  REFERENCES g_selector_revision(owner_id,kind,ticket_k,version,mode)
) WITHOUT ROWID;
CREATE TABLE g_selector_member(
 owner_id TEXT NOT NULL, kind TEXT NOT NULL, ticket_k TEXT NOT NULL,
 version INTEGER NOT NULL,
 native_hash TEXT NOT NULL,
 member_role TEXT NOT NULL CHECK(member_role IN('frozen_shared_base','selected_private')),
 PRIMARY KEY(owner_id,kind,ticket_k,version,native_hash),
 FOREIGN KEY(owner_id,kind,ticket_k,version)
  REFERENCES g_selector_revision(owner_id,kind,ticket_k,version)
) WITHOUT ROWID;
CREATE TABLE g_pending_path(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 path TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN('upsert','remove')),
 selected_hash TEXT,
 selected_mode INTEGER CHECK(selected_mode IN(0,1,2)),
 selected_scope TEXT NOT NULL CHECK(selected_scope IN('shared','private')),
 old_path TEXT,
 CHECK((action='upsert' AND selected_hash IS NOT NULL AND selected_mode IS NOT NULL) OR
       (action='remove' AND selected_hash IS NULL AND selected_mode IS NULL)),
 PRIMARY KEY(checkout_id,path)
) WITHOUT ROWID;
CREATE TABLE g_public_receipt(
 native_hash TEXT NOT NULL,
 validation_class TEXT NOT NULL CHECK(validation_class IN('local-explicit-publication','independent-public-receipt','legacy-public-import')),
 supporting_root TEXT NOT NULL,
 PRIMARY KEY(native_hash,validation_class,supporting_root)
) WITHOUT ROWID;
CREATE TABLE g_journal(
 checkout_id TEXT PRIMARY KEY NOT NULL REFERENCES g_checkout(checkout_id),
 operation_id TEXT NOT NULL UNIQUE,
 expected_generation INTEGER NOT NULL,
 expected_pending_version INTEGER NOT NULL CHECK(expected_pending_version>=0),
 resulting_pending_version INTEGER NOT NULL CHECK(resulting_pending_version=expected_pending_version+1),
 resulting_generation INTEGER NOT NULL CHECK(resulting_generation=expected_generation+1),
 old_shared TEXT NOT NULL, old_combined TEXT,
 new_shared TEXT NOT NULL, new_combined TEXT,
 state TEXT NOT NULL CHECK(state IN('pending','blocked'))
) WITHOUT ROWID;
CREATE TABLE g_journal_file(
 checkout_id TEXT NOT NULL REFERENCES g_journal(checkout_id),
 path TEXT NOT NULL,
 before_hash TEXT, after_hash TEXT,
 before_mode INTEGER, after_mode INTEGER,
 phase TEXT NOT NULL CHECK(phase IN('pending','written','blocked')),
 before_kind TEXT NOT NULL CHECK(before_kind IN('absent','regular','symlink','directory')),
 after_kind TEXT NOT NULL CHECK(after_kind IN('absent','regular','symlink','directory')),
 CHECK((before_kind='regular' AND before_hash IS NOT NULL AND before_mode IS NOT NULL AND before_mode IN(0,1)) OR
       (before_kind='symlink' AND before_hash IS NOT NULL AND before_mode IS NOT NULL AND before_mode=2) OR
       (before_kind IN('absent','directory') AND before_hash IS NULL AND before_mode IS NULL)),
 CHECK((after_kind='regular' AND after_hash IS NOT NULL AND after_mode IS NOT NULL AND after_mode IN(0,1)) OR
       (after_kind='symlink' AND after_hash IS NOT NULL AND after_mode IS NOT NULL AND after_mode=2) OR
       (after_kind IN('absent','directory') AND after_hash IS NULL AND after_mode IS NULL)),
 PRIMARY KEY(checkout_id,path)
) WITHOUT ROWID;
CREATE TABLE g_local_command(
 command_id TEXT PRIMARY KEY NOT NULL,
 owner_id TEXT NOT NULL REFERENCES g_local_owner(owner_id), project_id TEXT NOT NULL,
 stack_sequence INTEGER NOT NULL CHECK(stack_sequence>=0),
 recipe_version INTEGER NOT NULL CHECK(recipe_version=1),
 recipe BLOB NOT NULL,
 undone INTEGER NOT NULL CHECK(undone IN(0,1)),
 UNIQUE(owner_id,project_id,stack_sequence)
) WITHOUT ROWID;
CREATE INDEX g_command_pending ON g_local_command(owner_id,project_id,undone,stack_sequence DESC);
CREATE TABLE g_inverse_chain(
 command_id TEXT PRIMARY KEY NOT NULL REFERENCES g_local_command(command_id),
 provenance BLOB NOT NULL
) WITHOUT ROWID;
-- Derived tables below are wholly replaceable in a rebuild transaction.
CREATE TABLE g_contributor(
 scope_key TEXT NOT NULL, ticket_k TEXT NOT NULL, native_hash TEXT NOT NULL,
 comparison_sha3 TEXT NOT NULL CHECK(length(comparison_sha3)=64),
 d_seconds INTEGER, d_nanos INTEGER CHECK(d_nanos BETWEEN 0 AND 999999999),
 status TEXT NOT NULL CHECK(status IN('valid','invalid','unsupported','unresolved')),
 PRIMARY KEY(scope_key,ticket_k,native_hash)
) WITHOUT ROWID;
CREATE INDEX g_contributor_order ON g_contributor(scope_key,ticket_k,d_seconds,d_nanos,comparison_sha3);
CREATE TABLE g_identity(
 scope_key TEXT NOT NULL, ticket_k TEXT NOT NULL,
 kind TEXT NOT NULL, identity_key TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN('valid','invalid','unsupported','unresolved','conflict')),
 PRIMARY KEY(scope_key,ticket_k)
) WITHOUT ROWID;
-- No UNIQUE on identity_key: contradictory raw contributors must remain diagnosable.
CREATE INDEX g_identity_lookup ON g_identity(scope_key,kind,identity_key);
CREATE TABLE g_dependency(
 scope_key TEXT NOT NULL, source_hash TEXT NOT NULL,
 role TEXT NOT NULL, target_hash TEXT NOT NULL,
 required INTEGER NOT NULL CHECK(required IN(0,1)),
 PRIMARY KEY(scope_key,source_hash,role,target_hash)
) WITHOUT ROWID;
CREATE INDEX g_dependency_reverse ON g_dependency(scope_key,target_hash,source_hash);
CREATE TABLE g_diagnostic(
 scope_key TEXT NOT NULL, ticket_k TEXT NOT NULL, native_hash TEXT NOT NULL,
 field TEXT NOT NULL, code TEXT NOT NULL, detail TEXT NOT NULL,
 PRIMARY KEY(scope_key,ticket_k,native_hash,field,code)
) WITHOUT ROWID;
-- Retained incomplete/raw admission is distinct from completed public authority.
CREATE TABLE g_admission(
 native_hash TEXT PRIMARY KEY NOT NULL,
 state TEXT NOT NULL CHECK(state IN('shared_pending','legacy_shared_incomplete','owner_unclassified')),
 reason TEXT NOT NULL
) WITHOUT ROWID;
