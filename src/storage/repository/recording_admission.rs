//! Authenticate the exact catalog4 scope and private-recording family.
//!
//! ## Recommendations
//! Keep catalog1 on its original validator. Charge shared work before retaining
//! catalog4 authority, then admit only the complete recorded-history projection.
use super::recorded_history::parse_recording_budgeted;
use super::{
    AdmissionLimits, AdmissionProof, ParseWork, Principal, StorageError, Value, validate_empty_root,
};
use crate::storage::{WriteTransaction, content::Budget};
use std::{os::unix::fs::MetadataExt, path::Path};

/// Describe fixed SQL queries that retain raw text bytes and native numeric values.
struct SnapshotTable {
    /// Number of physical columns, before type/value expansion.
    columns: usize,
    /// Scalar count and total retained byte lengths, evaluated before ownership.
    bounds: &'static str,
    /// Deterministically ordered type/value pairs; no hostile TEXT decoding.
    rows: &'static str,
}
/// All nonempty authority surfaces in this exact initialized/recorded family.
/// Blob envelopes are authenticated by content reads, not copied into this proof;
/// their native type and byte length remain part of the sealed metadata.
const AUTHORITY_TABLES: &[SnapshotTable] = &[
    SnapshotTable {
        columns: 5,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"type\" AS BLOB)),0)+coalesce(length(CAST(\"name\" AS BLOB)),0)+coalesce(length(CAST(\"tbl_name\" AS BLOB)),0)+coalesce(length(CAST(\"rootpage\" AS BLOB)),0)+coalesce(length(CAST(\"sql\" AS BLOB)),0)),0) FROM \"sqlite_schema\"",
        rows: "SELECT typeof(\"type\"),CASE WHEN typeof(\"type\") IN('text','blob') THEN CAST(\"type\" AS BLOB) ELSE \"type\" END,typeof(\"name\"),CASE WHEN typeof(\"name\") IN('text','blob') THEN CAST(\"name\" AS BLOB) ELSE \"name\" END,typeof(\"tbl_name\"),CASE WHEN typeof(\"tbl_name\") IN('text','blob') THEN CAST(\"tbl_name\" AS BLOB) ELSE \"tbl_name\" END,typeof(\"rootpage\"),CASE WHEN typeof(\"rootpage\") IN('text','blob') THEN CAST(\"rootpage\" AS BLOB) ELSE \"rootpage\" END,typeof(\"sql\"),CASE WHEN typeof(\"sql\") IN('text','blob') THEN CAST(\"sql\" AS BLOB) ELSE \"sql\" END FROM \"sqlite_schema\" ORDER BY 1,2,3,4,5,6,7,8,9,10",
    },
    SnapshotTable {
        columns: 6,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"singleton\" AS BLOB)),0)+coalesce(length(CAST(\"repository_id\" AS BLOB)),0)+coalesce(length(CAST(\"storage_version\" AS BLOB)),0)+coalesce(length(CAST(\"projection_version\" AS BLOB)),0)+coalesce(length(CAST(\"policy_complete\" AS BLOB)),0)+coalesce(length(CAST(\"checkout_count\" AS BLOB)),0)),0) FROM \"g_storage\"",
        rows: "SELECT typeof(\"singleton\"),CASE WHEN typeof(\"singleton\") IN('text','blob') THEN CAST(\"singleton\" AS BLOB) ELSE \"singleton\" END,typeof(\"repository_id\"),CASE WHEN typeof(\"repository_id\") IN('text','blob') THEN CAST(\"repository_id\" AS BLOB) ELSE \"repository_id\" END,typeof(\"storage_version\"),CASE WHEN typeof(\"storage_version\") IN('text','blob') THEN CAST(\"storage_version\" AS BLOB) ELSE \"storage_version\" END,typeof(\"projection_version\"),CASE WHEN typeof(\"projection_version\") IN('text','blob') THEN CAST(\"projection_version\" AS BLOB) ELSE \"projection_version\" END,typeof(\"policy_complete\"),CASE WHEN typeof(\"policy_complete\") IN('text','blob') THEN CAST(\"policy_complete\" AS BLOB) ELSE \"policy_complete\" END,typeof(\"checkout_count\"),CASE WHEN typeof(\"checkout_count\") IN('text','blob') THEN CAST(\"checkout_count\" AS BLOB) ELSE \"checkout_count\" END FROM \"g_storage\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12",
    },
    SnapshotTable {
        columns: 2,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"singleton\" AS BLOB)),0)+coalesce(length(CAST(\"owner_id\" AS BLOB)),0)),0) FROM \"g_local_owner\"",
        rows: "SELECT typeof(\"singleton\"),CASE WHEN typeof(\"singleton\") IN('text','blob') THEN CAST(\"singleton\" AS BLOB) ELSE \"singleton\" END,typeof(\"owner_id\"),CASE WHEN typeof(\"owner_id\") IN('text','blob') THEN CAST(\"owner_id\" AS BLOB) ELSE \"owner_id\" END FROM \"g_local_owner\" ORDER BY 1,2,3,4",
    },
    SnapshotTable {
        columns: 3,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"native_hash\" AS BLOB)),0)+coalesce(length(CAST(\"validation_class\" AS BLOB)),0)+coalesce(length(CAST(\"supporting_root\" AS BLOB)),0)),0) FROM \"g_public_receipt\"",
        rows: "SELECT typeof(\"native_hash\"),CASE WHEN typeof(\"native_hash\") IN('text','blob') THEN CAST(\"native_hash\" AS BLOB) ELSE \"native_hash\" END,typeof(\"validation_class\"),CASE WHEN typeof(\"validation_class\") IN('text','blob') THEN CAST(\"validation_class\" AS BLOB) ELSE \"validation_class\" END,typeof(\"supporting_root\"),CASE WHEN typeof(\"supporting_root\") IN('text','blob') THEN CAST(\"supporting_root\" AS BLOB) ELSE \"supporting_root\" END FROM \"g_public_receipt\" ORDER BY 1,2,3,4,5,6",
    },
    SnapshotTable {
        columns: 7,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"checkout_id\" AS BLOB)),0)+coalesce(length(CAST(\"root\" AS BLOB)),0)+coalesce(length(CAST(\"shared_head\" AS BLOB)),0)+coalesce(length(CAST(\"combined_head\" AS BLOB)),0)+coalesce(length(CAST(\"generation\" AS BLOB)),0)+coalesce(length(CAST(\"pending_version\" AS BLOB)),0)+coalesce(length(CAST(\"availability\" AS BLOB)),0)),0) FROM \"g_checkout\"",
        rows: "SELECT typeof(\"checkout_id\"),CASE WHEN typeof(\"checkout_id\") IN('text','blob') THEN CAST(\"checkout_id\" AS BLOB) ELSE \"checkout_id\" END,typeof(\"root\"),CASE WHEN typeof(\"root\") IN('text','blob') THEN CAST(\"root\" AS BLOB) ELSE \"root\" END,typeof(\"shared_head\"),CASE WHEN typeof(\"shared_head\") IN('text','blob') THEN CAST(\"shared_head\" AS BLOB) ELSE \"shared_head\" END,typeof(\"combined_head\"),CASE WHEN typeof(\"combined_head\") IN('text','blob') THEN CAST(\"combined_head\" AS BLOB) ELSE \"combined_head\" END,typeof(\"generation\"),CASE WHEN typeof(\"generation\") IN('text','blob') THEN CAST(\"generation\" AS BLOB) ELSE \"generation\" END,typeof(\"pending_version\"),CASE WHEN typeof(\"pending_version\") IN('text','blob') THEN CAST(\"pending_version\" AS BLOB) ELSE \"pending_version\" END,typeof(\"availability\"),CASE WHEN typeof(\"availability\") IN('text','blob') THEN CAST(\"availability\" AS BLOB) ELSE \"availability\" END FROM \"g_checkout\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14",
    },
    SnapshotTable {
        columns: 4,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"checkout_id\" AS BLOB)),0)+coalesce(length(CAST(\"owner_id\" AS BLOB)),0)+coalesce(length(CAST(\"root_device\" AS BLOB)),0)+coalesce(length(CAST(\"root_inode\" AS BLOB)),0)),0) FROM \"g_checkout_owner\"",
        rows: "SELECT typeof(\"checkout_id\"),CASE WHEN typeof(\"checkout_id\") IN('text','blob') THEN CAST(\"checkout_id\" AS BLOB) ELSE \"checkout_id\" END,typeof(\"owner_id\"),CASE WHEN typeof(\"owner_id\") IN('text','blob') THEN CAST(\"owner_id\" AS BLOB) ELSE \"owner_id\" END,typeof(\"root_device\"),CASE WHEN typeof(\"root_device\") IN('text','blob') THEN CAST(\"root_device\" AS BLOB) ELSE \"root_device\" END,typeof(\"root_inode\"),CASE WHEN typeof(\"root_inode\") IN('text','blob') THEN CAST(\"root_inode\" AS BLOB) ELSE \"root_inode\" END FROM \"g_checkout_owner\" ORDER BY 1,2,3,4,5,6,7,8",
    },
    SnapshotTable {
        columns: 3,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"checkout_id\" AS BLOB)),0)+coalesce(length(CAST(\"name\" AS BLOB)),0)+coalesce(length(CAST(\"value\" AS BLOB)),0)),0) FROM \"g_vvar\"",
        rows: "SELECT typeof(\"checkout_id\"),CASE WHEN typeof(\"checkout_id\") IN('text','blob') THEN CAST(\"checkout_id\" AS BLOB) ELSE \"checkout_id\" END,typeof(\"name\"),CASE WHEN typeof(\"name\") IN('text','blob') THEN CAST(\"name\" AS BLOB) ELSE \"name\" END,typeof(\"value\"),CASE WHEN typeof(\"value\") IN('text','blob') THEN CAST(\"value\" AS BLOB) ELSE \"value\" END FROM \"g_vvar\" ORDER BY 1,2,3,4,5,6",
    },
    SnapshotTable {
        columns: 13,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"checkout_id\" AS BLOB)),0)+coalesce(length(CAST(\"id\" AS BLOB)),0)+coalesce(length(CAST(\"vid\" AS BLOB)),0)+coalesce(length(CAST(\"chnged\" AS BLOB)),0)+coalesce(length(CAST(\"deleted\" AS BLOB)),0)+coalesce(length(CAST(\"isexe\" AS BLOB)),0)+coalesce(length(CAST(\"islink\" AS BLOB)),0)+coalesce(length(CAST(\"rid\" AS BLOB)),0)+coalesce(length(CAST(\"mrid\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"pathname\" AS BLOB)),0)+coalesce(length(CAST(\"origname\" AS BLOB)),0)+coalesce(length(CAST(\"mhash\" AS BLOB)),0)),0) FROM \"g_vfile\"",
        rows: "SELECT typeof(\"checkout_id\"),CASE WHEN typeof(\"checkout_id\") IN('text','blob') THEN CAST(\"checkout_id\" AS BLOB) ELSE \"checkout_id\" END,typeof(\"id\"),CASE WHEN typeof(\"id\") IN('text','blob') THEN CAST(\"id\" AS BLOB) ELSE \"id\" END,typeof(\"vid\"),CASE WHEN typeof(\"vid\") IN('text','blob') THEN CAST(\"vid\" AS BLOB) ELSE \"vid\" END,typeof(\"chnged\"),CASE WHEN typeof(\"chnged\") IN('text','blob') THEN CAST(\"chnged\" AS BLOB) ELSE \"chnged\" END,typeof(\"deleted\"),CASE WHEN typeof(\"deleted\") IN('text','blob') THEN CAST(\"deleted\" AS BLOB) ELSE \"deleted\" END,typeof(\"isexe\"),CASE WHEN typeof(\"isexe\") IN('text','blob') THEN CAST(\"isexe\" AS BLOB) ELSE \"isexe\" END,typeof(\"islink\"),CASE WHEN typeof(\"islink\") IN('text','blob') THEN CAST(\"islink\" AS BLOB) ELSE \"islink\" END,typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END,typeof(\"mrid\"),CASE WHEN typeof(\"mrid\") IN('text','blob') THEN CAST(\"mrid\" AS BLOB) ELSE \"mrid\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"pathname\"),CASE WHEN typeof(\"pathname\") IN('text','blob') THEN CAST(\"pathname\" AS BLOB) ELSE \"pathname\" END,typeof(\"origname\"),CASE WHEN typeof(\"origname\") IN('text','blob') THEN CAST(\"origname\" AS BLOB) ELSE \"origname\" END,typeof(\"mhash\"),CASE WHEN typeof(\"mhash\") IN('text','blob') THEN CAST(\"mhash\" AS BLOB) ELSE \"mhash\" END FROM \"g_vfile\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26",
    },
    SnapshotTable {
        columns: 3,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"checkout_id\" AS BLOB)),0)+coalesce(length(CAST(\"path\" AS BLOB)),0)+coalesce(length(CAST(\"scope\" AS BLOB)),0)),0) FROM \"g_path_scope\"",
        rows: "SELECT typeof(\"checkout_id\"),CASE WHEN typeof(\"checkout_id\") IN('text','blob') THEN CAST(\"checkout_id\" AS BLOB) ELSE \"checkout_id\" END,typeof(\"path\"),CASE WHEN typeof(\"path\") IN('text','blob') THEN CAST(\"path\" AS BLOB) ELSE \"path\" END,typeof(\"scope\"),CASE WHEN typeof(\"scope\") IN('text','blob') THEN CAST(\"scope\" AS BLOB) ELSE \"scope\" END FROM \"g_path_scope\" ORDER BY 1,2,3,4,5,6",
    },
    SnapshotTable {
        columns: 5,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rid\" AS BLOB)),0)+coalesce(length(CAST(\"rcvid\" AS BLOB)),0)+coalesce(length(CAST(\"size\" AS BLOB)),0)+coalesce(length(CAST(\"uuid\" AS BLOB)),0)+coalesce(length(CAST(length(\"content\") AS BLOB)),0)),0) FROM \"blob\"",
        rows: "SELECT typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END,typeof(\"rcvid\"),CASE WHEN typeof(\"rcvid\") IN('text','blob') THEN CAST(\"rcvid\" AS BLOB) ELSE \"rcvid\" END,typeof(\"size\"),CASE WHEN typeof(\"size\") IN('text','blob') THEN CAST(\"size\" AS BLOB) ELSE \"size\" END,typeof(\"uuid\"),CASE WHEN typeof(\"uuid\") IN('text','blob') THEN CAST(\"uuid\" AS BLOB) ELSE \"uuid\" END,typeof(\"content\"),length(\"content\") FROM \"blob\" ORDER BY 1,2,3,4,5,6,7,8,9,10",
    },
    SnapshotTable {
        columns: 12,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"type\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"objid\" AS BLOB)),0)+coalesce(length(CAST(\"tagid\" AS BLOB)),0)+coalesce(length(CAST(\"uid\" AS BLOB)),0)+coalesce(length(CAST(\"bgcolor\" AS BLOB)),0)+coalesce(length(CAST(\"euser\" AS BLOB)),0)+coalesce(length(CAST(\"user\" AS BLOB)),0)+coalesce(length(CAST(\"ecomment\" AS BLOB)),0)+coalesce(length(CAST(\"comment\" AS BLOB)),0)+coalesce(length(CAST(\"brief\" AS BLOB)),0)+coalesce(length(CAST(\"omtime\" AS BLOB)),0)),0) FROM \"event\"",
        rows: "SELECT typeof(\"type\"),CASE WHEN typeof(\"type\") IN('text','blob') THEN CAST(\"type\" AS BLOB) ELSE \"type\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"objid\"),CASE WHEN typeof(\"objid\") IN('text','blob') THEN CAST(\"objid\" AS BLOB) ELSE \"objid\" END,typeof(\"tagid\"),CASE WHEN typeof(\"tagid\") IN('text','blob') THEN CAST(\"tagid\" AS BLOB) ELSE \"tagid\" END,typeof(\"uid\"),CASE WHEN typeof(\"uid\") IN('text','blob') THEN CAST(\"uid\" AS BLOB) ELSE \"uid\" END,typeof(\"bgcolor\"),CASE WHEN typeof(\"bgcolor\") IN('text','blob') THEN CAST(\"bgcolor\" AS BLOB) ELSE \"bgcolor\" END,typeof(\"euser\"),CASE WHEN typeof(\"euser\") IN('text','blob') THEN CAST(\"euser\" AS BLOB) ELSE \"euser\" END,typeof(\"user\"),CASE WHEN typeof(\"user\") IN('text','blob') THEN CAST(\"user\" AS BLOB) ELSE \"user\" END,typeof(\"ecomment\"),CASE WHEN typeof(\"ecomment\") IN('text','blob') THEN CAST(\"ecomment\" AS BLOB) ELSE \"ecomment\" END,typeof(\"comment\"),CASE WHEN typeof(\"comment\") IN('text','blob') THEN CAST(\"comment\" AS BLOB) ELSE \"comment\" END,typeof(\"brief\"),CASE WHEN typeof(\"brief\") IN('text','blob') THEN CAST(\"brief\" AS BLOB) ELSE \"brief\" END,typeof(\"omtime\"),CASE WHEN typeof(\"omtime\") IN('text','blob') THEN CAST(\"omtime\" AS BLOB) ELSE \"omtime\" END FROM \"event\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24",
    },
    SnapshotTable {
        columns: 2,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"tagid\" AS BLOB)),0)+coalesce(length(CAST(\"tagname\" AS BLOB)),0)),0) FROM \"tag\"",
        rows: "SELECT typeof(\"tagid\"),CASE WHEN typeof(\"tagid\") IN('text','blob') THEN CAST(\"tagid\" AS BLOB) ELSE \"tagid\" END,typeof(\"tagname\"),CASE WHEN typeof(\"tagname\") IN('text','blob') THEN CAST(\"tagname\" AS BLOB) ELSE \"tagname\" END FROM \"tag\" ORDER BY 1,2,3,4",
    },
    SnapshotTable {
        columns: 7,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"tagid\" AS BLOB)),0)+coalesce(length(CAST(\"tagtype\" AS BLOB)),0)+coalesce(length(CAST(\"srcid\" AS BLOB)),0)+coalesce(length(CAST(\"origid\" AS BLOB)),0)+coalesce(length(CAST(\"value\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"rid\" AS BLOB)),0)),0) FROM \"tagxref\"",
        rows: "SELECT typeof(\"tagid\"),CASE WHEN typeof(\"tagid\") IN('text','blob') THEN CAST(\"tagid\" AS BLOB) ELSE \"tagid\" END,typeof(\"tagtype\"),CASE WHEN typeof(\"tagtype\") IN('text','blob') THEN CAST(\"tagtype\" AS BLOB) ELSE \"tagtype\" END,typeof(\"srcid\"),CASE WHEN typeof(\"srcid\") IN('text','blob') THEN CAST(\"srcid\" AS BLOB) ELSE \"srcid\" END,typeof(\"origid\"),CASE WHEN typeof(\"origid\") IN('text','blob') THEN CAST(\"origid\" AS BLOB) ELSE \"origid\" END,typeof(\"value\"),CASE WHEN typeof(\"value\") IN('text','blob') THEN CAST(\"value\" AS BLOB) ELSE \"value\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END FROM \"tagxref\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14",
    },
    SnapshotTable {
        columns: 1,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rid\" AS BLOB)),0)),0) FROM \"leaf\"",
        rows: "SELECT typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END FROM \"leaf\" ORDER BY 1,2",
    },
    SnapshotTable {
        columns: 5,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rcvid\" AS BLOB)),0)+coalesce(length(CAST(\"uid\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"nonce\" AS BLOB)),0)+coalesce(length(CAST(\"ipaddr\" AS BLOB)),0)),0) FROM \"rcvfrom\"",
        rows: "SELECT typeof(\"rcvid\"),CASE WHEN typeof(\"rcvid\") IN('text','blob') THEN CAST(\"rcvid\" AS BLOB) ELSE \"rcvid\" END,typeof(\"uid\"),CASE WHEN typeof(\"uid\") IN('text','blob') THEN CAST(\"uid\" AS BLOB) ELSE \"uid\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"nonce\"),CASE WHEN typeof(\"nonce\") IN('text','blob') THEN CAST(\"nonce\" AS BLOB) ELSE \"nonce\" END,typeof(\"ipaddr\"),CASE WHEN typeof(\"ipaddr\") IN('text','blob') THEN CAST(\"ipaddr\" AS BLOB) ELSE \"ipaddr\" END FROM \"rcvfrom\" ORDER BY 1,2,3,4,5,6,7,8,9,10",
    },
    SnapshotTable {
        columns: 1,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rid\" AS BLOB)),0)),0) FROM \"private\"",
        rows: "SELECT typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END FROM \"private\" ORDER BY 1,2",
    },
    SnapshotTable {
        columns: 1,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rid\" AS BLOB)),0)),0) FROM \"unclustered\"",
        rows: "SELECT typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END FROM \"unclustered\" ORDER BY 1,2",
    },
    SnapshotTable {
        columns: 1,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"rid\" AS BLOB)),0)),0) FROM \"unsent\"",
        rows: "SELECT typeof(\"rid\"),CASE WHEN typeof(\"rid\") IN('text','blob') THEN CAST(\"rid\" AS BLOB) ELSE \"rid\" END FROM \"unsent\" ORDER BY 1,2",
    },
    SnapshotTable {
        columns: 5,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"pid\" AS BLOB)),0)+coalesce(length(CAST(\"cid\" AS BLOB)),0)+coalesce(length(CAST(\"isprim\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"baseid\" AS BLOB)),0)),0) FROM \"plink\"",
        rows: "SELECT typeof(\"pid\"),CASE WHEN typeof(\"pid\") IN('text','blob') THEN CAST(\"pid\" AS BLOB) ELSE \"pid\" END,typeof(\"cid\"),CASE WHEN typeof(\"cid\") IN('text','blob') THEN CAST(\"cid\" AS BLOB) ELSE \"cid\" END,typeof(\"isprim\"),CASE WHEN typeof(\"isprim\") IN('text','blob') THEN CAST(\"isprim\" AS BLOB) ELSE \"isprim\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"baseid\"),CASE WHEN typeof(\"baseid\") IN('text','blob') THEN CAST(\"baseid\" AS BLOB) ELSE \"baseid\" END FROM \"plink\" ORDER BY 1,2,3,4,5,6,7,8,9,10",
    },
    SnapshotTable {
        columns: 8,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"mid\" AS BLOB)),0)+coalesce(length(CAST(\"fid\" AS BLOB)),0)+coalesce(length(CAST(\"pmid\" AS BLOB)),0)+coalesce(length(CAST(\"pid\" AS BLOB)),0)+coalesce(length(CAST(\"fnid\" AS BLOB)),0)+coalesce(length(CAST(\"pfnid\" AS BLOB)),0)+coalesce(length(CAST(\"mperm\" AS BLOB)),0)+coalesce(length(CAST(\"isaux\" AS BLOB)),0)),0) FROM \"mlink\"",
        rows: "SELECT typeof(\"mid\"),CASE WHEN typeof(\"mid\") IN('text','blob') THEN CAST(\"mid\" AS BLOB) ELSE \"mid\" END,typeof(\"fid\"),CASE WHEN typeof(\"fid\") IN('text','blob') THEN CAST(\"fid\" AS BLOB) ELSE \"fid\" END,typeof(\"pmid\"),CASE WHEN typeof(\"pmid\") IN('text','blob') THEN CAST(\"pmid\" AS BLOB) ELSE \"pmid\" END,typeof(\"pid\"),CASE WHEN typeof(\"pid\") IN('text','blob') THEN CAST(\"pid\" AS BLOB) ELSE \"pid\" END,typeof(\"fnid\"),CASE WHEN typeof(\"fnid\") IN('text','blob') THEN CAST(\"fnid\" AS BLOB) ELSE \"fnid\" END,typeof(\"pfnid\"),CASE WHEN typeof(\"pfnid\") IN('text','blob') THEN CAST(\"pfnid\" AS BLOB) ELSE \"pfnid\" END,typeof(\"mperm\"),CASE WHEN typeof(\"mperm\") IN('text','blob') THEN CAST(\"mperm\" AS BLOB) ELSE \"mperm\" END,typeof(\"isaux\"),CASE WHEN typeof(\"isaux\") IN('text','blob') THEN CAST(\"isaux\" AS BLOB) ELSE \"isaux\" END FROM \"mlink\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16",
    },
    SnapshotTable {
        columns: 2,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"fnid\" AS BLOB)),0)+coalesce(length(CAST(\"name\" AS BLOB)),0)),0) FROM \"filename\"",
        rows: "SELECT typeof(\"fnid\"),CASE WHEN typeof(\"fnid\") IN('text','blob') THEN CAST(\"fnid\" AS BLOB) ELSE \"fnid\" END,typeof(\"name\"),CASE WHEN typeof(\"name\") IN('text','blob') THEN CAST(\"name\" AS BLOB) ELSE \"name\" END FROM \"filename\" ORDER BY 1,2,3,4",
    },
    SnapshotTable {
        columns: 3,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"name\" AS BLOB)),0)+coalesce(length(CAST(\"value\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)),0) FROM \"config\"",
        rows: "SELECT typeof(\"name\"),CASE WHEN typeof(\"name\") IN('text','blob') THEN CAST(\"name\" AS BLOB) ELSE \"name\" END,typeof(\"value\"),CASE WHEN typeof(\"value\") IN('text','blob') THEN CAST(\"value\" AS BLOB) ELSE \"value\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END FROM \"config\" ORDER BY 1,2,3,4,5,6",
    },
    SnapshotTable {
        columns: 11,
        bounds: "SELECT count(*),coalesce(sum(coalesce(length(CAST(\"uid\" AS BLOB)),0)+coalesce(length(CAST(\"login\" AS BLOB)),0)+coalesce(length(CAST(\"pw\" AS BLOB)),0)+coalesce(length(CAST(\"cap\" AS BLOB)),0)+coalesce(length(CAST(\"cookie\" AS BLOB)),0)+coalesce(length(CAST(\"ipaddr\" AS BLOB)),0)+coalesce(length(CAST(\"cexpire\" AS BLOB)),0)+coalesce(length(CAST(\"info\" AS BLOB)),0)+coalesce(length(CAST(\"mtime\" AS BLOB)),0)+coalesce(length(CAST(\"photo\" AS BLOB)),0)+coalesce(length(CAST(\"jx\" AS BLOB)),0)),0) FROM \"user\"",
        rows: "SELECT typeof(\"uid\"),CASE WHEN typeof(\"uid\") IN('text','blob') THEN CAST(\"uid\" AS BLOB) ELSE \"uid\" END,typeof(\"login\"),CASE WHEN typeof(\"login\") IN('text','blob') THEN CAST(\"login\" AS BLOB) ELSE \"login\" END,typeof(\"pw\"),CASE WHEN typeof(\"pw\") IN('text','blob') THEN CAST(\"pw\" AS BLOB) ELSE \"pw\" END,typeof(\"cap\"),CASE WHEN typeof(\"cap\") IN('text','blob') THEN CAST(\"cap\" AS BLOB) ELSE \"cap\" END,typeof(\"cookie\"),CASE WHEN typeof(\"cookie\") IN('text','blob') THEN CAST(\"cookie\" AS BLOB) ELSE \"cookie\" END,typeof(\"ipaddr\"),CASE WHEN typeof(\"ipaddr\") IN('text','blob') THEN CAST(\"ipaddr\" AS BLOB) ELSE \"ipaddr\" END,typeof(\"cexpire\"),CASE WHEN typeof(\"cexpire\") IN('text','blob') THEN CAST(\"cexpire\" AS BLOB) ELSE \"cexpire\" END,typeof(\"info\"),CASE WHEN typeof(\"info\") IN('text','blob') THEN CAST(\"info\" AS BLOB) ELSE \"info\" END,typeof(\"mtime\"),CASE WHEN typeof(\"mtime\") IN('text','blob') THEN CAST(\"mtime\" AS BLOB) ELSE \"mtime\" END,typeof(\"photo\"),CASE WHEN typeof(\"photo\") IN('text','blob') THEN CAST(\"photo\" AS BLOB) ELSE \"photo\" END,typeof(\"jx\"),CASE WHEN typeof(\"jx\") IN('text','blob') THEN CAST(\"jx\" AS BLOB) ELSE \"jx\" END FROM \"user\" ORDER BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22",
    },
];

impl WriteTransaction<'_> {
    /// Charge the complete result before a fixed authority query materializes rows.
    /// # Arguments
    /// * `table` - Private fixed SQL surface whose complete values are retained.
    /// * `budget` - Borrowed operation ledger, including row and cell overhead.
    /// # Returns
    /// Expected row count after bounded accounting.
    /// # Errors
    /// Refuses scalar corruption, arithmetic overflow, allocation-scale work or SQL failure.
    async fn preflight_authority_table(
        &mut self,
        table: &SnapshotTable,
        budget: &mut Budget,
    ) -> Result<usize, StorageError> {
        let rows = self.query_bound(table.bounds, vec![]).await?;
        charge_authority_bounds(&rows, table.columns, budget)
    }

    /// Load OS-opened local identity only after bounding its raw provider bytes.
    /// # Arguments
    /// * `budget` - Original admission work ledger.
    /// # Returns
    /// Sealed identity for subsequent exact catalog/policy authentication.
    /// # Errors
    /// Refuses extra/missing rows, malformed text or exhausted retained work.
    pub(super) async fn read_recording_principal_budgeted(
        &mut self,
        budget: &mut Budget,
    ) -> Result<Principal, StorageError> {
        self.preflight_authority_table(&AUTHORITY_TABLES[1], budget)
            .await?;
        self.preflight_authority_table(&AUTHORITY_TABLES[2], budget)
            .await?;
        let rows=self.query_bound("SELECT CAST(s.repository_id AS BLOB),CAST(o.owner_id AS BLOB),typeof(s.repository_id),typeof(o.owner_id) FROM g_storage s CROSS JOIN g_local_owner o WHERE s.singleton=1 AND o.singleton=1 LIMIT 2",vec![]).await?;
        recording_principal_from_rows(&rows)
    }

    /// Authenticate schema, sealed owner and native markers with precharged ownership.
    /// # Arguments
    /// * `principal` - OS-established sealed local identity, never caller-selected authority.
    /// * `budget` - Shared operation work retained by the caller.
    /// # Returns
    /// Success after exact schema/policy/owner/marker authentication.
    /// # Errors
    /// Refuses unbounded catalog or policy material, schema drift and identity changes.
    pub(in crate::storage) async fn validate_recording_principal_budgeted(
        &mut self,
        principal: &Principal,
        budget: &mut Budget,
    ) -> Result<(), StorageError> {
        let result = async {
            // validate_principal owns schema rows and these complete policy tables.
            // Precharge the full surfaces before its existing fixed queries run.
            for index in [0, 1, 2, 21] {
                self.preflight_authority_table(&AUTHORITY_TABLES[index], budget)
                    .await?;
            }
            if self
                .query_bound(
                    "SELECT singleton FROM g_local_owner ORDER BY singleton LIMIT 2",
                    vec![],
                )
                .await?
                != vec![vec![Value::Integer(1)]]
            {
                return Err(StorageError::state(
                    "unsupported catalog4 owner cardinality",
                ));
            }
            self.validate_principal(principal).await
        }
        .await;
        self.record(result)
    }

    /// Retain every supported authority fact under one borrowed work budget.
    /// # Arguments
    /// * `budget` - Existing operation accounting; no replenishment occurs here.
    /// # Returns
    /// Exact typed metadata with deterministic ordering and raw text bytes.
    /// # Errors
    /// Refuses exhausted work, invalid preflight counts or SQL/provider failures.
    async fn recording_authority_snapshot(
        &mut self,
        budget: &mut Budget,
    ) -> Result<Vec<Vec<Vec<Value>>>, StorageError> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(AUTHORITY_TABLES.len())
            .map_err(|e| StorageError::caused("authority table allocation", e))?;
        for table in AUTHORITY_TABLES {
            let expected = self.preflight_authority_table(table, budget).await?;
            let rows = self.query_bound(table.rows, vec![]).await?;
            validate_authority_rows(&rows, expected, table.columns)?;
            result.push(rows);
        }
        Ok(result)
    }

    /// Seal produced-state authority after complete family validation.
    /// # Arguments
    /// * `budget` - Shared operation ledger carrying the caller's original limits.
    /// # Returns
    /// Proof retained only after the caller obtains successful COMMIT.
    /// # Errors
    /// Refuses exhausted authority ownership or failed provider observations.
    pub(super) async fn capture_admission_budgeted(
        &mut self,
        budget: &mut Budget,
    ) -> Result<AdmissionProof, StorageError> {
        let result = async {
            self.observe_recording_provider()?;
            let authority = self.recording_authority_snapshot(budget).await?;
            let data_version = self.query_bound("PRAGMA data_version", vec![]).await?;
            Ok(AdmissionProof {
                authority,
                data_version,
                total_changes: self.connection().total_changes(),
                content_limits: Some(budget.limits()),
            })
        }
        .await;
        self.record(result)
    }

    /// Reauthenticate retained authority without resetting any operation budget.
    /// # Arguments
    /// * `principal` - Sealed owner binding of the opened repository.
    /// * `proof` - Last committed exact authority snapshot.
    /// * `budget` - Shared caller work ledger.
    /// # Returns
    /// Success only for the unchanged admitted provider/policy/state.
    /// # Errors
    /// Rejects stale connection/external counters, changed authority or exhausted work.
    pub(super) async fn check_admission_budgeted(
        &mut self,
        principal: &Principal,
        proof: &AdmissionProof,
        budget: &mut Budget,
    ) -> Result<(), StorageError> {
        let result = async {
            if self.connection().total_changes() != proof.total_changes
                || self.query_bound("PRAGMA data_version", vec![]).await? != proof.data_version
            {
                return Err(StorageError::state(
                    "repository admission stale; reopen with explicit admission limits",
                ));
            }
            self.observe_recording_provider()?;
            self.validate_recording_principal_budgeted(principal, budget)
                .await?;
            self.require_native_checkout().await?;
            if self.recording_authority_snapshot(budget).await? != proof.authority {
                return Err(StorageError::state(
                    "repository authority changed; explicit readmission required",
                ));
            }
            Ok(())
        }
        .await;
        self.record(result)
    }

    /// Observe the retained provider, allowing only the existing private memory fixtures.
    /// # Returns
    /// Success for the original provider namespace and regular-file identity.
    /// # Errors
    /// Preserves cooperative namespace/identity observation failures.
    fn observe_recording_provider(&self) -> Result<(), StorageError> {
        if let Some(identity) = &self.store.identity {
            identity.observe()?;
        }
        Ok(())
    }

    /// Authenticate immutable S and local boundaries before the history decoder.
    /// # Arguments
    /// * `principal` - Sealed owner binding.
    /// * `limits` - Complete native and registration ceilings.
    /// * `budget` - Shared content/retained-work ledger.
    /// * `parse` - Shared parser ledger for S and every private manifest.
    /// # Returns
    /// Success for exact initialized, tracked/scoped or repeated private-recorded state.
    /// # Errors
    /// Refuses foreign schema/policy/provider, corrupted S, unsupported tables,
    /// invalid registration or any history/projection/fingerprint mismatch.
    pub(super) async fn validate_recording_repository(
        &mut self,
        principal: &Principal,
        limits: AdmissionLimits,
        budget: &mut Budget,
        parse: &mut ParseWork,
    ) -> Result<(), StorageError> {
        let result = async {
            self.observe_recording_provider()?;
            self.validate_recording_principal_budgeted(principal,budget).await?;
            self.require_native_checkout().await?;
            if self.query_bound("SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",vec![]).await? != vec![vec![Value::Integer(0)]] {return Err(StorageError::state("repository foreign key integrity failure"));}
            // Bound every potentially populated surface before legacy typed reads
            // or the separately owned exact recorded-history validator.
            for table in AUTHORITY_TABLES { self.preflight_authority_table(table,budget).await?; }
            self.require_recording_empty_tables().await?;
            let roots=self.query_bound("SELECT CAST(native_hash AS BLOB),CAST(supporting_root AS BLOB),CAST(validation_class AS BLOB) FROM g_public_receipt ORDER BY native_hash,supporting_root LIMIT 2",vec![]).await?;
            let [row]=roots.as_slice() else {return Err(StorageError::state("unsupported required public graph closure"));};
            let [Value::Blob(hash),Value::Blob(root),Value::Blob(class)]=row.as_slice() else {return Err(StorageError::state("corrupt public root binding"));};
            let root=std::str::from_utf8(root).map_err(|e|StorageError::caused("public root UTF-8",e))?;
            if hash.as_slice()!=root.as_bytes() || class.as_slice()!=b"local-explicit-publication" || !native_hash(root) {return Err(StorageError::state("unsupported required public graph closure"));}
            let root_type=self.query_bound("SELECT typeof(native_hash),typeof(supporting_root),typeof(validation_class) FROM g_public_receipt",vec![]).await?;
            if root_type!=vec![vec![Value::Text("text".into());3]] {return Err(StorageError::state("corrupt public root binding"));}
            let root_row=self.query_bound("SELECT rid,rcvid,typeof(uuid),typeof(content) FROM blob WHERE uuid=?1",vec![Value::Text(root.into())]).await?;
            if root_row!=vec![vec![Value::Integer(1),Value::Integer(1),Value::Text("text".into()),Value::Text("blob".into())]] {return Err(StorageError::state("immutable shared root provenance differs"));}
            let bytes=self.read_recording_content_budget(principal,root,budget).await?;
            let parsed=parse_recording_budgeted(self,&bytes,parse,budget).await?;
            validate_empty_root(&parsed)?;
            let date=parsed.date().ok_or_else(||StorageError::state("shared root date unavailable"))?;
            let date=super::native_stored_date(date);
            let user=parsed.user().ok_or_else(||StorageError::state("shared root user unavailable"))?;
            let user=std::str::from_utf8(&user.bytes[..user.c_string_len]).map_err(|e|StorageError::caused("shared root user UTF-8",e))?;
            let event=self.query_bound("SELECT objid,type,mtime,user,comment,omtime,tagid,uid,bgcolor,euser,ecomment,brief FROM event WHERE objid=1",vec![]).await?;
            if event!=vec![vec![Value::Integer(1),Value::Text("ci".into()),date.clone(),Value::Text(user.into()),Value::Text("initial empty check-in".into()),date.clone(),Value::Null,Value::Null,Value::Null,Value::Null,Value::Null,Value::Null]] {return Err(StorageError::state("root event differs from canonical bytes"));}
            let root_tags=self.query_bound("SELECT tagid,tagtype,srcid,origid,value,mtime,rid FROM tagxref WHERE rid=1 ORDER BY tagid",vec![]).await?;
            if root_tags!=vec![vec![Value::Integer(8),Value::Integer(2),Value::Integer(1),Value::Integer(1),Value::Text("trunk".into()),date.clone(),Value::Integer(1)],vec![Value::Integer(12),Value::Integer(2),Value::Integer(1),Value::Integer(1),Value::Null,date.clone(),Value::Integer(1)]] {return Err(StorageError::state("root branch crosslinks differ from canonical bytes"));}
            let receive=self.query_bound("SELECT rcvid,uid,mtime,nonce,ipaddr FROM rcvfrom WHERE rcvid=1",vec![]).await?;
            if receive!=vec![vec![Value::Integer(1),Value::Integer(0),date.clone(),Value::Null,Value::Null]] {return Err(StorageError::state("native root receive facts differ"));}
            let tags=self.query_bound("SELECT tagid,tagname FROM tag WHERE tagid<=12 ORDER BY tagid",vec![]).await?;
            let expected:Vec<Vec<Value>>=["bgcolor","comment","user","date","hidden","private","cluster","branch","closed","parent","note","sym-trunk"].into_iter().enumerate().map(|(i,name)|vec![Value::Integer((i+1) as i64),Value::Text(name.into())]).collect();
            if tags!=expected {return Err(StorageError::state("unsupported native tag definitions"));}
            let server=self.query_bound("SELECT CAST(value AS BLOB),typeof(value) FROM config WHERE name='server-code'",vec![]).await?;
            if !matches!(server.as_slice(),[row] if matches!(row.as_slice(),[Value::Blob(value),Value::Text(kind)] if kind=="text" && value.len()==40 && value.iter().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(b)))) {return Err(StorageError::state("native server identity unavailable"));}
            let counts=self.query_bound("SELECT (SELECT count(*) FROM g_checkout),(SELECT count(*) FROM g_checkout_owner),checkout_count FROM g_storage WHERE singleton=1",vec![]).await?;
            if counts==vec![vec![Value::Integer(0);3]] {
                // Without a registration, reject private history and scoped or native rows.
                self.validate_empty_catalog4(1,root,&date,principal,limits).await?;
            } else {
                if counts!=vec![vec![Value::Integer(1);3]] {return Err(StorageError::state("unsupported native checkout inventory"));}
                self.validate_recording_registration(principal,root,limits).await?;
                self.validate_recorded_history(principal,root,limits,budget,parse).await?;
            }
            self.observe_recording_provider()?;
            Ok(())
        }.await;
        self.record(result)
    }

    /// Check exact owner/root and scalar boundaries independently of private history.
    /// # Arguments
    /// * `principal` - Sealed owner identity.
    /// * `shared` - Authenticated immutable S native hash.
    /// * `limits` - Caller registration text ceilings.
    /// # Returns
    /// Success for one ready exact-root registration with bounded nonnegative counters.
    /// # Errors
    /// Refuses type/null/length/spelling, foreign owner/head or replaced ancestry.
    async fn validate_recording_registration(
        &mut self,
        principal: &Principal,
        shared: &str,
        limits: AdmissionLimits,
    ) -> Result<(), StorageError> {
        let valid=self.query_bound("SELECT typeof(c.checkout_id)='text' AND length(CAST(c.checkout_id AS BLOB)) BETWEEN 1 AND ?1 AND typeof(c.root)='text' AND length(CAST(c.root AS BLOB)) BETWEEN 1 AND ?2 AND typeof(c.shared_head)='text' AND c.shared_head IS ?3 AND (typeof(c.combined_head)='null' OR (typeof(c.combined_head)='text' AND length(CAST(c.combined_head AS BLOB)) IN(40,64))) AND typeof(c.generation)='integer' AND c.generation>=0 AND typeof(c.pending_version)='integer' AND c.pending_version>=0 AND typeof(c.availability)='text' AND c.availability IS 'ready' AND typeof(o.checkout_id)='text' AND o.checkout_id IS c.checkout_id AND typeof(o.owner_id)='text' AND o.owner_id IS ?4 AND typeof(o.root_device)='text' AND length(CAST(o.root_device AS BLOB)) BETWEEN 1 AND 20 AND typeof(o.root_inode)='text' AND length(CAST(o.root_inode AS BLOB)) BETWEEN 1 AND 20 FROM g_checkout c JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id",vec![Value::Integer(limits.parser.input_bytes.min(1024) as i64),Value::Integer(limits.parser.input_bytes.min(4096) as i64),Value::Text(shared.into()),Value::Text(principal.owner.clone())]).await?;
        if valid != vec![vec![Value::Integer(1)]] {
            return Err(StorageError::state("unsupported recording checkout state"));
        }
        let rows=self.query_bound("SELECT CAST(c.checkout_id AS BLOB),CAST(c.root AS BLOB),CAST(o.root_device AS BLOB),CAST(o.root_inode AS BLOB) FROM g_checkout c JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id",vec![]).await?;
        validate_registration_rows(&rows)
    }

    /// Retain every unrelated catalog4 empty-table guard.
    /// # Returns
    /// Success only when no unsupported local/native state is present.
    /// # Errors
    /// Refuses any populated guard surface or SQL error; never repairs it.
    async fn require_recording_empty_tables(&mut self) -> Result<(), StorageError> {
        /// Enumerate every unrelated native surface that must remain empty.
        const GUARDS: &[(&str, &str)] = &[
            ("accesslog", "SELECT EXISTS(SELECT 1 FROM accesslog)"),
            ("admin_log", "SELECT EXISTS(SELECT 1 FROM admin_log)"),
            ("attachment", "SELECT EXISTS(SELECT 1 FROM attachment)"),
            ("backlink", "SELECT EXISTS(SELECT 1 FROM backlink)"),
            ("chat", "SELECT EXISTS(SELECT 1 FROM chat)"),
            ("cherrypick", "SELECT EXISTS(SELECT 1 FROM cherrypick)"),
            ("concealed", "SELECT EXISTS(SELECT 1 FROM concealed)"),
            ("delta", "SELECT EXISTS(SELECT 1 FROM delta)"),
            ("forumpost", "SELECT EXISTS(SELECT 1 FROM forumpost)"),
            ("g_admission", "SELECT EXISTS(SELECT 1 FROM g_admission)"),
            (
                "g_checkout_project",
                "SELECT EXISTS(SELECT 1 FROM g_checkout_project)",
            ),
            ("g_code_link", "SELECT EXISTS(SELECT 1 FROM g_code_link)"),
            (
                "g_contributor",
                "SELECT EXISTS(SELECT 1 FROM g_contributor)",
            ),
            ("g_delivery", "SELECT EXISTS(SELECT 1 FROM g_delivery)"),
            (
                "g_delivery_lane",
                "SELECT EXISTS(SELECT 1 FROM g_delivery_lane)",
            ),
            ("g_dependency", "SELECT EXISTS(SELECT 1 FROM g_dependency)"),
            ("g_descriptor", "SELECT EXISTS(SELECT 1 FROM g_descriptor)"),
            ("g_diagnostic", "SELECT EXISTS(SELECT 1 FROM g_diagnostic)"),
            (
                "g_discussion_message",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_message)",
            ),
            (
                "g_discussion_target",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_target)",
            ),
            (
                "g_discussion_thread",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_thread)",
            ),
            ("g_file_undo", "SELECT EXISTS(SELECT 1 FROM g_file_undo)"),
            (
                "g_gest_artifacts",
                "SELECT EXISTS(SELECT 1 FROM g_gest_artifacts)",
            ),
            (
                "g_gest_authors",
                "SELECT EXISTS(SELECT 1 FROM g_gest_authors)",
            ),
            (
                "g_gest_entity_tags",
                "SELECT EXISTS(SELECT 1 FROM g_gest_entity_tags)",
            ),
            (
                "g_gest_iteration_tasks",
                "SELECT EXISTS(SELECT 1 FROM g_gest_iteration_tasks)",
            ),
            (
                "g_gest_iterations",
                "SELECT EXISTS(SELECT 1 FROM g_gest_iterations)",
            ),
            ("g_gest_notes", "SELECT EXISTS(SELECT 1 FROM g_gest_notes)"),
            (
                "g_gest_projects",
                "SELECT EXISTS(SELECT 1 FROM g_gest_projects)",
            ),
            (
                "g_gest_relationships",
                "SELECT EXISTS(SELECT 1 FROM g_gest_relationships)",
            ),
            ("g_gest_tags", "SELECT EXISTS(SELECT 1 FROM g_gest_tags)"),
            ("g_gest_tasks", "SELECT EXISTS(SELECT 1 FROM g_gest_tasks)"),
            (
                "g_git_checkout",
                "SELECT EXISTS(SELECT 1 FROM g_git_checkout)",
            ),
            ("g_identity", "SELECT EXISTS(SELECT 1 FROM g_identity)"),
            (
                "g_inverse_chain",
                "SELECT EXISTS(SELECT 1 FROM g_inverse_chain)",
            ),
            ("g_journal", "SELECT EXISTS(SELECT 1 FROM g_journal)"),
            (
                "g_journal_file",
                "SELECT EXISTS(SELECT 1 FROM g_journal_file)",
            ),
            (
                "g_local_command",
                "SELECT EXISTS(SELECT 1 FROM g_local_command)",
            ),
            ("g_mergestat", "SELECT EXISTS(SELECT 1 FROM g_mergestat)"),
            (
                "g_message_context",
                "SELECT EXISTS(SELECT 1 FROM g_message_context)",
            ),
            (
                "g_message_context_target",
                "SELECT EXISTS(SELECT 1 FROM g_message_context_target)",
            ),
            (
                "g_pending_path",
                "SELECT EXISTS(SELECT 1 FROM g_pending_path)",
            ),
            (
                "g_project_binding",
                "SELECT EXISTS(SELECT 1 FROM g_project_binding)",
            ),
            (
                "g_project_workspaces",
                "SELECT EXISTS(SELECT 1 FROM g_project_workspaces)",
            ),
            (
                "g_record_diagnostic",
                "SELECT EXISTS(SELECT 1 FROM g_record_diagnostic)",
            ),
            ("g_selector", "SELECT EXISTS(SELECT 1 FROM g_selector)"),
            (
                "g_selector_member",
                "SELECT EXISTS(SELECT 1 FROM g_selector_member)",
            ),
            (
                "g_selector_revision",
                "SELECT EXISTS(SELECT 1 FROM g_selector_revision)",
            ),
            ("g_stash", "SELECT EXISTS(SELECT 1 FROM g_stash)"),
            ("g_stashfile", "SELECT EXISTS(SELECT 1 FROM g_stashfile)"),
            ("g_suggestion", "SELECT EXISTS(SELECT 1 FROM g_suggestion)"),
            (
                "g_suggestion_application",
                "SELECT EXISTS(SELECT 1 FROM g_suggestion_application)",
            ),
            (
                "g_target_entity_member",
                "SELECT EXISTS(SELECT 1 FROM g_target_entity_member)",
            ),
            ("g_undo_stash", "SELECT EXISTS(SELECT 1 FROM g_undo_stash)"),
            (
                "g_undo_stashfile",
                "SELECT EXISTS(SELECT 1 FROM g_undo_stashfile)",
            ),
            ("g_undo_vfile", "SELECT EXISTS(SELECT 1 FROM g_undo_vfile)"),
            (
                "g_undo_vmerge",
                "SELECT EXISTS(SELECT 1 FROM g_undo_vmerge)",
            ),
            ("g_vmerge", "SELECT EXISTS(SELECT 1 FROM g_vmerge)"),
            ("modreq", "SELECT EXISTS(SELECT 1 FROM modreq)"),
            ("orphan", "SELECT EXISTS(SELECT 1 FROM orphan)"),
            ("phantom", "SELECT EXISTS(SELECT 1 FROM phantom)"),
            ("shun", "SELECT EXISTS(SELECT 1 FROM shun)"),
            ("ticket", "SELECT EXISTS(SELECT 1 FROM ticket)"),
            ("ticketchng", "SELECT EXISTS(SELECT 1 FROM ticketchng)"),
            ("unversioned", "SELECT EXISTS(SELECT 1 FROM unversioned)"),
        ];
        for &(table, sql) in GUARDS {
            if self.query_bound(sql, vec![]).await? != vec![vec![Value::Integer(0)]] {
                return Err(StorageError::state(format!(
                    "unsupported nonempty catalog4 table: {table}"
                )));
            }
        }
        Ok(())
    }
}

/// Decode and charge the scalar preflight before retaining authority rows.
/// # Arguments
/// * `rows` - Provider result from the fixed aggregate query.
/// * `columns` - Number of physical columns in the reviewed authority surface.
/// * `budget` - Borrowed operation ledger, never replenished here.
/// # Returns
/// Exact expected row count after charging bytes and row overhead.
/// # Errors
/// Refuses missing, ambiguous or malformed scalars, overflow and exhausted work.
fn charge_authority_bounds(
    rows: &[Vec<Value>],
    columns: usize,
    budget: &mut Budget,
) -> Result<usize, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("authority bounds unavailable"));
    };
    let [Value::Integer(count), Value::Integer(bytes)] = row.as_slice() else {
        return Err(StorageError::state("authority bounds invalid"));
    };
    let count =
        usize::try_from(*count).map_err(|e| StorageError::caused("authority count overflow", e))?;
    let bytes = usize::try_from(*bytes)
        .map_err(|e| StorageError::caused("authority byte count overflow", e))?;
    // Two Values per column plus owned row/vector overhead. The fixed charge
    // bounds even inventories containing only NULL or empty values.
    let overhead = columns
        .checked_mul(128)
        .and_then(|v| v.checked_add(64))
        .and_then(|v| v.checked_mul(count))
        .ok_or_else(|| StorageError::state("authority allocation accounting overflow"))?;
    budget.charge_decoded(
        bytes
            .checked_add(overhead)
            .ok_or_else(|| StorageError::state("authority allocation accounting overflow"))?,
    )?;
    Ok(count)
}

/// Decode the precharged raw local identity binding.
/// # Arguments
/// * `rows` - Exact singleton raw identity and storage-class result.
/// # Returns
/// Validated owned repository and owner identity.
/// # Errors
/// Refuses row shape, storage class, UTF-8, NUL and unsupported identity spelling.
fn recording_principal_from_rows(rows: &[Vec<Value>]) -> Result<Principal, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("local repository binding unavailable"));
    };
    let [
        Value::Blob(repository),
        Value::Blob(owner),
        Value::Text(a),
        Value::Text(b),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("corrupt local repository binding"));
    };
    if a != "text" || b != "text" {
        return Err(StorageError::state("corrupt local repository binding"));
    }
    let repository = std::str::from_utf8(repository)
        .map_err(|e| StorageError::caused("repository identity UTF-8", e))?
        .to_owned();
    let owner = std::str::from_utf8(owner)
        .map_err(|e| StorageError::caused("owner identity UTF-8", e))?
        .to_owned();
    if owner.contains('\0')
        || owner.is_empty()
        || repository.len() != 40
        || !native_hash(&repository)
    {
        return Err(StorageError::state(
            "invalid local repository identity binding",
        ));
    }
    Ok(Principal { repository, owner })
}

/// Require the authority rows to match their precharged shape.
/// # Arguments
/// * `rows` - Complete raw result of the fixed authority query.
/// * `expected` - Row count admitted by the scalar preflight.
/// * `columns` - Physical columns expanded into type/value pairs.
/// # Returns
/// Success for the exact retained result shape.
/// # Errors
/// Refuses changed row counts or incomplete type/value rows.
fn validate_authority_rows(
    rows: &[Vec<Value>],
    expected: usize,
    columns: usize,
) -> Result<(), StorageError> {
    if rows.len() != expected || rows.iter().any(|row| row.len() != columns * 2) {
        return Err(StorageError::state("authority inventory changed"));
    }
    Ok(())
}

/// Decode precharged registration facts and authenticate their live root.
/// # Arguments
/// * `rows` - Raw identity, path, device and inode after scalar state validation.
/// # Returns
/// Success for an exact canonical directory with unchanged filesystem identity.
/// # Errors
/// Refuses malformed rows/text, NUL, missing or noncanonical roots and changed authority.
fn validate_registration_rows(rows: &[Vec<Value>]) -> Result<(), StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("checkout owner binding unavailable"));
    };
    let [
        Value::Blob(id),
        Value::Blob(path),
        Value::Blob(device),
        Value::Blob(inode),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("checkout owner binding invalid"));
    };
    let text = |bytes: &[u8]| {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| StorageError::caused("registration UTF-8", e))
    };
    let id = text(id)?;
    let path = text(path)?;
    let device = text(device)?;
    let inode = text(inode)?;
    if id.contains('\0') || path.contains('\0') {
        return Err(StorageError::state("registration contains NUL"));
    }
    let path = Path::new(&path);
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| StorageError::caused("checkout root unavailable", e))?;
    if canonical.as_os_str() != path.as_os_str() {
        return Err(StorageError::state(
            "registered checkout root or authority differs",
        ));
    }
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|e| StorageError::caused("checkout root component unavailable", e))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(StorageError::state(
                "registered checkout root or authority differs",
            ));
        }
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| StorageError::caused("registered checkout root unavailable", e))?;
    if device != metadata.dev().to_string() || inode != metadata.ino().to_string() {
        return Err(StorageError::state(
            "registered checkout root identity differs",
        ));
    }
    Ok(())
}

/// Recognize an exact supported native hash without accepting embedded NUL.
/// # Arguments
/// * `value` - Raw-decoded persisted hash token.
/// # Returns
/// Whether this is lowercase hexadecimal SHA1 or SHA3-256.
fn native_hash(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
/// Check exact admission projection comparisons and retained snapshot bounds.
mod tests {
    use super::*;
    use crate::storage::{Limits, LocalStore};

    /// Give a private metadata boundary one explicit retained-work ceiling.
    fn budget(bytes: u64) -> Budget {
        Budget::new(Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 4,
            max_total_stored_bytes: 4096,
            max_total_decoded_bytes: bytes,
        })
        .unwrap()
    }

    /// Raw authority retains storage classes and exact hostile text bytes without decoding them.
    #[tokio::test]
    async fn scope_authority_snapshot_retains_raw_text_and_storage_class() {
        let mut store = LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE g_path_scope(checkout_id TEXT,path TEXT,scope TEXT)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_path_scope VALUES('work',CAST(x'610062' AS TEXT),'private')")
            .await
            .unwrap();
        let table = &AUTHORITY_TABLES[8];
        let mut work = budget(4096);
        assert_eq!(
            tx.preflight_authority_table(table, &mut work)
                .await
                .unwrap(),
            1
        );
        let before = tx.query_bound(table.rows, vec![]).await.unwrap();
        assert_eq!(before[0][2], Value::Text("text".into()));
        assert_eq!(before[0][3], Value::Blob(b"a\0b".to_vec()));
        tx.execute("UPDATE g_path_scope SET path=x'610062'")
            .await
            .unwrap();
        let after = tx.query_bound(table.rows, vec![]).await.unwrap();
        assert_eq!(after[0][2], Value::Text("blob".into()));
        assert_eq!(after[0][3], Value::Blob(b"a\0b".to_vec()));
        assert_ne!(before, after);
        tx.execute("UPDATE g_path_scope SET path=CAST(x'ff' AS TEXT)")
            .await
            .unwrap();
        let invalid = tx.query_bound(table.rows, vec![]).await.unwrap();
        assert_eq!(invalid[0][3], Value::Blob(vec![0xff]));
        tx.rollback().await.unwrap();
    }

    /// Repeated metadata reads consume the same ledger, including empty-valued row overhead.
    #[tokio::test]
    async fn authority_preflight_consumes_shared_work_before_ownership() {
        let mut store = LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE g_path_scope(checkout_id TEXT,path TEXT,scope TEXT)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_path_scope VALUES('','','')")
            .await
            .unwrap();
        let table = &AUTHORITY_TABLES[8];
        let charge = 3 * 128 + 64;
        let mut work = budget(charge * 2 - 1);
        assert_eq!(
            tx.preflight_authority_table(table, &mut work)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            tx.preflight_authority_table(table, &mut work)
                .await
                .unwrap_err()
                .operation,
            "total decoded content limit"
        );
        let mut enough = budget(charge * 2);
        assert!(
            tx.preflight_authority_table(table, &mut enough)
                .await
                .is_ok()
        );
        assert!(
            tx.preflight_authority_table(table, &mut enough)
                .await
                .is_ok()
        );
        tx.rollback().await.unwrap();
    }
    /// Scalar shape errors and raw binding corruption remain independently fail-closed.
    #[test]
    fn authority_and_principal_decoders_reject_every_malformed_boundary() {
        for rows in [vec![], vec![vec![], vec![]]] {
            assert_eq!(
                charge_authority_bounds(&rows, 2, &mut budget(4096))
                    .unwrap_err()
                    .operation,
                "authority bounds unavailable"
            );
        }
        assert_eq!(
            charge_authority_bounds(
                &[vec![Value::Null, Value::Integer(0)]],
                2,
                &mut budget(4096)
            )
            .unwrap_err()
            .operation,
            "authority bounds invalid"
        );
        for (count, bytes, error) in [
            (-1, 0, "authority count overflow"),
            (0, -1, "authority byte count overflow"),
        ] {
            assert_eq!(
                charge_authority_bounds(
                    &[vec![Value::Integer(count), Value::Integer(bytes)]],
                    2,
                    &mut budget(4096)
                )
                .unwrap_err()
                .operation,
                error
            );
        }
        let exact = vec![vec![Value::Text("text".into()), Value::Blob(b"x".to_vec())]];
        assert!(validate_authority_rows(&exact, 1, 1).is_ok());
        assert_eq!(
            validate_authority_rows(&exact, 0, 1).unwrap_err().operation,
            "authority inventory changed"
        );
        assert_eq!(
            validate_authority_rows(&exact, 1, 2).unwrap_err().operation,
            "authority inventory changed"
        );
        let valid = vec![
            Value::Blob(b"0123456789abcdef0123456789abcdef01234567".to_vec()),
            Value::Blob(b"owner".to_vec()),
            Value::Text("text".into()),
            Value::Text("text".into()),
        ];
        let principal = recording_principal_from_rows(std::slice::from_ref(&valid)).unwrap();
        assert_eq!(principal.owner, "owner");
        for index in 0..4 {
            let mut row = valid.clone();
            row[index] = Value::Null;
            assert_eq!(
                recording_principal_from_rows(&[row])
                    .err()
                    .unwrap()
                    .operation,
                "corrupt local repository binding"
            );
        }
        for index in [2, 3] {
            let mut row = valid.clone();
            row[index] = Value::Text("blob".into());
            assert_eq!(
                recording_principal_from_rows(&[row])
                    .err()
                    .unwrap()
                    .operation,
                "corrupt local repository binding"
            );
        }
        for (index, bytes) in [
            (0, b"bad".as_slice()),
            (1, b"".as_slice()),
            (1, b"a\0b".as_slice()),
        ] {
            let mut row = valid.clone();
            row[index] = Value::Blob(bytes.to_vec());
            assert_eq!(
                recording_principal_from_rows(&[row])
                    .err()
                    .unwrap()
                    .operation,
                "invalid local repository identity binding"
            );
        }
        for (index, expected) in [
            (0, "repository identity UTF-8"),
            (1, "owner identity UTF-8"),
        ] {
            let mut row = valid.clone();
            row[index] = Value::Blob(vec![0xff]);
            assert_eq!(
                recording_principal_from_rows(&[row])
                    .err()
                    .unwrap()
                    .operation,
                expected
            );
        }
    }

    /// The raw registration decoder validates real filesystem authority after scalar admission.
    #[test]
    fn registration_decoder_refuses_shape_and_non_directory_roots() {
        assert_eq!(
            validate_registration_rows(&[]).unwrap_err().operation,
            "checkout owner binding unavailable"
        );
        assert_eq!(
            validate_registration_rows(&[vec![Value::Null; 4]])
                .unwrap_err()
                .operation,
            "checkout owner binding invalid"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let metadata = std::fs::symlink_metadata(&root).unwrap();
        let row = |path: &Path, device: u64, inode: u64| {
            vec![
                Value::Blob(b"work".to_vec()),
                Value::Blob(path.as_os_str().as_encoded_bytes().to_vec()),
                Value::Blob(device.to_string().into_bytes()),
                Value::Blob(inode.to_string().into_bytes()),
            ]
        };
        assert!(validate_registration_rows(&[row(&root, metadata.dev(), metadata.ino())]).is_ok());
        for (column, bytes, expected) in [
            (0, b"a\0b".as_slice(), "registration contains NUL"),
            (1, b"\xff".as_slice(), "registration UTF-8"),
        ] {
            let mut invalid = row(&root, metadata.dev(), metadata.ino());
            invalid[column] = Value::Blob(bytes.to_vec());
            assert_eq!(
                validate_registration_rows(&[invalid])
                    .unwrap_err()
                    .operation,
                expected
            );
        }
        let file = root.join("regular-file");
        std::fs::write(&file, b"content").unwrap();
        let metadata = std::fs::symlink_metadata(&file).unwrap();
        assert_eq!(
            validate_registration_rows(&[row(&file, metadata.dev(), metadata.ino())])
                .unwrap_err()
                .operation,
            "registered checkout root or authority differs"
        );
    }

    /// Private memory fixtures have no disk namespace to authenticate.
    #[tokio::test]
    async fn provider_observation_accepts_only_the_existing_memory_fixture_path() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let tx = store.begin_write().await.unwrap();
        tx.observe_recording_provider().unwrap();
        tx.rollback().await.unwrap();
    }

    /// The real raw principal reader rejects stored type and grammar corruption without repair.
    #[tokio::test]
    async fn principal_reader_preserves_rejected_raw_owner_rows() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE g_storage(singleton INTEGER,repository_id TEXT,storage_version INTEGER,projection_version TEXT,policy_complete INTEGER,checkout_count INTEGER)").await.unwrap();
        tx.execute("CREATE TABLE g_local_owner(singleton INTEGER,owner_id TEXT)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_storage VALUES(1,'0123456789abcdef0123456789abcdef01234567',4,'gest-ticket-v1',1,0)").await.unwrap();
        tx.execute("INSERT INTO g_local_owner VALUES(1,'owner')")
            .await
            .unwrap();
        assert_eq!(
            tx.read_recording_principal_budgeted(&mut budget(4096))
                .await
                .unwrap()
                .owner,
            "owner"
        );
        for (value, expected) in [
            (Value::Null, "corrupt local repository binding"),
            (
                Value::Blob(b"owner".to_vec()),
                "corrupt local repository binding",
            ),
            (
                Value::Text(String::new()),
                "invalid local repository identity binding",
            ),
            (
                Value::Text("a\0b".into()),
                "invalid local repository identity binding",
            ),
        ] {
            tx.execute_bound("UPDATE g_local_owner SET owner_id=?1", vec![value])
                .await
                .unwrap();
            let before = tx
                .query_bound(
                    "SELECT typeof(owner_id),CAST(owner_id AS BLOB) FROM g_local_owner",
                    vec![],
                )
                .await
                .unwrap();
            assert_eq!(
                tx.read_recording_principal_budgeted(&mut budget(4096))
                    .await
                    .err()
                    .unwrap()
                    .operation,
                expected
            );
            assert_eq!(
                tx.query_bound(
                    "SELECT typeof(owner_id),CAST(owner_id AS BLOB) FROM g_local_owner",
                    vec![]
                )
                .await
                .unwrap(),
                before
            );
        }
        tx.rollback().await.unwrap();
    }
}
