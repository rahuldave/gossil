-- Proposed exact checkout namespace replacement for native localdb schema.
-- Native fields retain spelling, affinity, defaults and nullable behavior.
-- Explicit differences: checkout FK; composite scoped row IDs and uniques;
-- native documentary REFERENCES omitted; vmerge sentinel IDs preserved.
CREATE TABLE g_vvar(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 name TEXT NOT NULL, value CLOB,
 PRIMARY KEY(checkout_id,name),
 CHECK(typeof(name)='text' AND length(name)>=1)
) WITHOUT ROWID;
CREATE TABLE g_vfile(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 id INTEGER NOT NULL, vid INTEGER, chnged INT DEFAULT 0,
 deleted BOOLEAN DEFAULT 0, isexe BOOLEAN, islink BOOLEAN,
 rid INTEGER, mrid INTEGER, mtime INTEGER, pathname TEXT, origname TEXT, mhash TEXT,
 PRIMARY KEY(checkout_id,id), UNIQUE(checkout_id,pathname,vid)
) WITHOUT ROWID;
CREATE INDEX g_vfile_nocase ON g_vfile(checkout_id,pathname COLLATE nocase);
CREATE TABLE g_vmerge(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 id INTEGER, merge INTEGER, mhash TEXT
);
CREATE UNIQUE INDEX g_vmergex1 ON g_vmerge(checkout_id,id,mhash);
CREATE TRIGGER g_vmerge_ck1 AFTER INSERT ON g_vmerge
WHEN new.mhash IS NULL BEGIN
 SELECT raise(FAIL,'trying to update a newer check-out with an older version of Fossil');
END;
CREATE TABLE g_stash(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 stashid INTEGER NOT NULL, vid INTEGER, hash TEXT, comment TEXT, ctime TIMESTAMP,
 PRIMARY KEY(checkout_id,stashid)
) WITHOUT ROWID;
CREATE TABLE g_stashfile(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 stashid INTEGER, isAdded BOOLEAN, isRemoved BOOLEAN, isExec BOOLEAN,
 isLink BOOLEAN, rid INTEGER, hash TEXT, origname TEXT, newname TEXT, delta BLOB,
 UNIQUE(checkout_id,newname,stashid)
);
-- Native rowid table's composite PRIMARY KEY permits NULL; UNIQUE preserves it.
-- It is intentionally not a WITHOUT ROWID composite PK strengthening nullability.
CREATE TABLE g_file_undo(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 pathname TEXT, redoflag BOOLEAN, existsflag BOOLEAN,
 isExe BOOLEAN, isLink BOOLEAN, content BLOB,
 UNIQUE(checkout_id,pathname)
);
-- Native CTAS snapshots carry affinity, but no source PK/UNIQUE/defaults.
CREATE TABLE g_undo_vfile(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 id INT, vid INT, chnged INT, deleted NUM, isexe NUM, islink NUM,
 rid INT, mrid INT, mtime INT, pathname TEXT, origname TEXT, mhash TEXT
);
CREATE INDEX g_undo_vfile_checkout ON g_undo_vfile(checkout_id);
CREATE TABLE g_undo_vmerge(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 id INT, merge INT, mhash TEXT
);
CREATE INDEX g_undo_vmerge_checkout ON g_undo_vmerge(checkout_id);
CREATE TABLE g_undo_stash(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 stashid INT, vid INT, hash TEXT, comment TEXT, ctime NUM
);
CREATE INDEX g_undo_stash_checkout ON g_undo_stash(checkout_id);
CREATE TABLE g_undo_stashfile(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 stashid INT, isAdded NUM, isRemoved NUM, isExec NUM, isLink NUM,
 rid INT, hash TEXT, origname TEXT, newname TEXT, delta
);
CREATE INDEX g_undo_stashfile_checkout ON g_undo_stashfile(checkout_id);
CREATE TABLE g_mergestat(
 checkout_id TEXT NOT NULL REFERENCES g_checkout(checkout_id),
 op TEXT, fnp TEXT, ridp INT, fn TEXT, ridv INT, sz INT, fnm TEXT,
 ridm INT, fnr TEXT, nc INT DEFAULT 0, msg TEXT
);
CREATE INDEX g_mergestat_checkout ON g_mergestat(checkout_id);
