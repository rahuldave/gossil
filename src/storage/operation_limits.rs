//! Explicit finite ceilings for one complete file operation and its saved declaration.
//!
//! [`FileOperationLimits`] joins the content, parser, filesystem and record ceilings of
//! one operation. [`DeclaredBudgets`] is the closed saved form of those eighteen
//! ceilings. A saved declaration can narrow the ceilings of a later caller, and never
//! widen them.
//!
//! ## Recommendations
//! Construct the limits once, and keep one work ledger of each kind through the
//! complete operation.
use super::StorageError;
use super::command_codec::{CodecLimits, CommandError, error};
use serde::{Deserialize, Serialize};

/// Deepest local JSON nesting that [`FileOperationLimits::codec`] derives.
const MAX_CODEC_DEPTH: usize = 127;

/// Explicit filesystem ceilings for one selected regular or executable file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileSystemLimits {
    /// Maximum normalized path and operation-name bytes.
    pub path_bytes: usize,
    /// Maximum components visited below the registered root.
    pub path_components: usize,
    /// Maximum complete journal rows, including ancestors.
    pub plan_rows: usize,
    /// Maximum retained bytes consumed by filesystem verification.
    pub retained_bytes: u64,
    /// Maximum sequential verification buffer bytes.
    pub io_buffer_bytes: usize,
    /// Maximum directory entries inspected by recovery.
    pub directory_entries: usize,
}

/// Every ceiling of one complete file admission, mutation and saved-plan recovery.
///
/// Root-only callers keep [`super::AdmissionLimits`] and cannot acquire these bounds.
#[derive(Clone, Copy)]
pub(super) struct FileOperationLimits {
    /// Native content reconstruction and work ceilings.
    content: super::Limits,
    /// Aggregate logical artifact parser ceilings.
    parser: crate::artifact::ParseLimits,
    /// Filesystem verification ceilings.
    filesystem: FileSystemLimits,
    /// Maximum encoded local recipe or provenance bytes.
    recipe_bytes: usize,
    /// Maximum retained canonical byte identities.
    closure_objects: usize,
    /// Maximum supported local or native projection rows.
    derived_rows: usize,
}
impl FileOperationLimits {
    /// Check and join complete finite caller limits, without filesystem or repository access.
    ///
    /// # Arguments
    /// * `content` - Finite per-object and cumulative native content ceilings.
    /// * `parser` - Finite artifact input, card, owned-byte and entry ceilings.
    /// * `filesystem` - Finite path, plan, retained-byte, buffer and directory-entry ceilings.
    /// * `recipe_bytes` - Maximum serialized local record and metadata bytes.
    /// * `closure_objects` - Maximum distinct canonical objects in the admitted file closure.
    /// * `derived_rows` - Maximum supported native or local projection rows.
    ///
    /// # Returns
    /// A reusable limit set after every ceiling is checked.
    ///
    /// # Errors
    /// Rejects a zero ceiling and a native output ceiling above `i32::MAX`. An operation can
    /// still exhaust a valid set.
    pub(super) fn new(
        content: super::Limits,
        parser: crate::artifact::ParseLimits,
        filesystem: FileSystemLimits,
        recipe_bytes: usize,
        closure_objects: usize,
        derived_rows: usize,
    ) -> Result<Self, StorageError> {
        let value = Self {
            content,
            parser,
            filesystem,
            recipe_bytes,
            closure_objects,
            derived_rows,
        };
        value
            .declared()
            .validate()
            .map_err(|e| StorageError::caused("complete file limits", e))?;
        // The ledgers keep their own checks, so a later change to them still applies here.
        super::content::Budget::new(content)?;
        super::repository::ParseWork::new(parser)?;
        Ok(value)
    }
    /// Capture all eighteen ceilings in their closed saved form.
    ///
    /// # Returns
    /// The exact caller ceilings as a saved declaration.
    pub(super) fn declared(self) -> DeclaredBudgets {
        let content = self.content;
        let parser = self.parser;
        let fs = self.filesystem;
        DeclaredBudgets {
            stored_bytes: content.max_stored_bytes,
            output_bytes: content.max_output_bytes,
            chain_depth: content.max_chain_depth,
            total_stored_bytes: content.max_total_stored_bytes,
            total_decoded_bytes: content.max_total_decoded_bytes,
            parse_input_bytes: parser.input_bytes,
            parse_cards: parser.cards,
            parse_owned_bytes: parser.owned_bytes,
            parse_entries: parser.entries,
            path_bytes: fs.path_bytes,
            path_components: fs.path_components,
            plan_rows: fs.plan_rows,
            retained_bytes: fs.retained_bytes,
            io_buffer_bytes: fs.io_buffer_bytes,
            directory_entries: fs.directory_entries,
            recipe_bytes: self.recipe_bytes,
            closure_objects: self.closure_objects,
            derived_rows: self.derived_rows,
        }
    }
    /// Require each saved ceiling to fit within the matching ceiling of this caller.
    ///
    /// The comparison is a resource precondition. It grants no authority over a journal
    /// or a recipe.
    ///
    /// # Arguments
    /// * `saved` - Saved declaration that must fit within every current ceiling.
    ///
    /// # Returns
    /// Unit when every saved ceiling is valid and at most the current ceiling.
    ///
    /// # Errors
    /// Rejects an invalid saved declaration and any saved ceiling above the current one.
    pub(super) fn check_saved(self, saved: &DeclaredBudgets) -> Result<(), StorageError> {
        saved
            .validate()
            .map_err(|e| StorageError::caused("saved file budget invalid", e))?;
        let current = self.declared().ceilings();
        if saved
            .ceilings()
            .iter()
            .zip(current)
            .any(|(saved, current)| *saved > current)
        {
            return Err(StorageError::state(
                "saved file operation exceeds caller limits",
            ));
        }
        Ok(())
    }
    /// Derive the local JSON ceilings of a codec ledger from the caller ceilings.
    ///
    /// # Returns
    /// The recipe bytes, the parser owned bytes as nodes, and the parser entries as depth,
    /// with the depth capped at 127.
    pub(super) fn codec(self) -> CodecLimits {
        CodecLimits {
            bytes: self.recipe_bytes,
            nodes: self.parser.owned_bytes,
            depth: self.parser.entries.min(MAX_CODEC_DEPTH),
        }
    }
    /// Give the unchanged root admission ceilings of this limit set.
    ///
    /// # Returns
    /// The content and artifact parser ceilings of this limit set.
    pub(super) fn root(self) -> super::AdmissionLimits {
        super::AdmissionLimits {
            content: self.content,
            parser: self.parser,
        }
    }
}

/// The exact declared ceilings of an operation, kept as part of an immutable saved request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclaredBudgets {
    /// Maximum bytes of one stored native envelope.
    pub stored_bytes: u64,
    /// Maximum bytes of one reconstructed native object.
    pub output_bytes: u32,
    /// Maximum native delta-chain depth.
    pub chain_depth: u32,
    /// Aggregate stored-envelope bytes for the operation.
    pub total_stored_bytes: u64,
    /// Aggregate decoded and reconstructed bytes for the operation.
    pub total_decoded_bytes: u64,
    /// Aggregate logical artifact input bytes, including any signature envelope.
    pub parse_input_bytes: usize,
    /// Aggregate artifact card count, including Z cards.
    pub parse_cards: usize,
    /// Aggregate logical artifact allocation charges, not physical allocator bytes.
    pub parse_owned_bytes: usize,
    /// Aggregate logical artifact list-entry charges.
    pub parse_entries: usize,
    /// Maximum normalized path or operation-name bytes.
    pub path_bytes: usize,
    /// Maximum path components inspected below the registered root.
    pub path_components: usize,
    /// Maximum complete filesystem-plan rows, including existing ancestors.
    pub plan_rows: usize,
    /// Maximum retained bytes consumed by filesystem verification.
    pub retained_bytes: u64,
    /// Maximum sequential filesystem verification buffer bytes.
    pub io_buffer_bytes: usize,
    /// Maximum directory entries inspected during recovery.
    pub directory_entries: usize,
    /// Maximum encoded local recipe or provenance bytes.
    pub recipe_bytes: usize,
    /// Maximum retained canonical byte identities.
    pub closure_objects: usize,
    /// Maximum supported local or native projection rows.
    pub derived_rows: usize,
}
impl DeclaredBudgets {
    /// Field names of the closed saved form, in declaration order.
    pub(super) const FIELDS: [&'static str; 18] = [
        "stored_bytes",
        "output_bytes",
        "chain_depth",
        "total_stored_bytes",
        "total_decoded_bytes",
        "parse_input_bytes",
        "parse_cards",
        "parse_owned_bytes",
        "parse_entries",
        "path_bytes",
        "path_components",
        "plan_rows",
        "retained_bytes",
        "io_buffer_bytes",
        "directory_entries",
        "recipe_bytes",
        "closure_objects",
        "derived_rows",
    ];
    /// List the eighteen ceilings in the order of [`Self::FIELDS`].
    ///
    /// # Returns
    /// Each ceiling widened to `u64`, which is lossless on every supported target.
    fn ceilings(&self) -> [u64; 18] {
        [
            self.stored_bytes,
            u64::from(self.output_bytes),
            u64::from(self.chain_depth),
            self.total_stored_bytes,
            self.total_decoded_bytes,
            self.parse_input_bytes as u64,
            self.parse_cards as u64,
            self.parse_owned_bytes as u64,
            self.parse_entries as u64,
            self.path_bytes as u64,
            self.path_components as u64,
            self.plan_rows as u64,
            self.retained_bytes,
            self.io_buffer_bytes as u64,
            self.directory_entries as u64,
            self.recipe_bytes as u64,
            self.closure_objects as u64,
            self.derived_rows as u64,
        ]
    }
    /// Require every saved ceiling to be positive and supported.
    ///
    /// # Returns
    /// Unit when every ceiling is positive and the output ceiling fits `i32`.
    ///
    /// # Errors
    /// Rejects a zero ceiling and a native output ceiling above `i32::MAX`.
    pub(super) fn validate(&self) -> Result<(), CommandError> {
        if self.ceilings().contains(&0) || self.output_bytes > i32::MAX as u32 {
            return Err(error("invalid declared operation budget"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/support/storage_operation_limits.rs"]
mod tests;
