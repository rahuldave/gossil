//! Decode and encode strict local JSON, and count the cumulative work of the codec.
//!
//! A local record is JSON that Gossil writes and reads back from its own
//! storage, such as a saved command recipe. One [`CodecWork`] ledger bounds the
//! bytes, values and depth of every record that an operation reads or writes.
//! The ledger is never replenished, so a later record spends what remains after
//! the earlier records.
//!
//! ## Recommendations
//! Retain one caller ledger across all records of an operation, and decode
//! closed records with [`CodecWork::closed_record`].
mod serde_preflight;

use serde::Serialize;
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::{fmt, io, sync::Arc};

/// Largest JSON nesting depth that any codec ledger accepts.
///
/// `serde_json` refuses the 128th nested container itself, so the ledger stops one level
/// earlier and gives its own refusal first.
const MAX_DEPTH: usize = 127;

/// Finite local codec ceilings, separate from artifact, content and SQL budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodecLimits {
    /// Total serialized JSON bytes that the ledger may read or write.
    pub bytes: usize,
    /// Total JSON values and object keys that the ledger may consume.
    pub nodes: usize,
    /// Deepest JSON value nesting, counting the root value as depth one; at most 127.
    pub depth: usize,
}
impl CodecLimits {
    /// Report whether every ceiling is positive and the depth is supported.
    ///
    /// # Returns
    /// True when bytes, nodes and depth are nonzero and depth is at most 127.
    fn valid(self) -> bool {
        self.bytes != 0 && self.nodes != 0 && self.depth != 0 && self.depth <= MAX_DEPTH
    }
}

/// A codec error reports a refused local record, and it grants no authority.
///
/// The caller latches it into its writer.
#[derive(Clone)]
pub(super) struct CommandError {
    /// Exact diagnostic text, which also defines equality of two codec errors.
    message: String,
    /// Original typed failure, shared by clones without erasing its concrete type.
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}
impl fmt::Debug for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CommandError").field(&self.message).finish()
    }
}
impl PartialEq for CommandError {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message
    }
}
impl Eq for CommandError {}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}
impl CommandError {
    /// Keep a typed cause next to the unchanged diagnostic.
    ///
    /// # Arguments
    /// * `message` - Diagnostic text to display without change.
    /// * `source` - Original typed error kept in the error chain.
    ///
    /// # Returns
    /// A command error with the supplied text and original typed cause.
    pub(super) fn with_source<E>(message: String, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            message,
            source: Some(Arc::new(source)),
        }
    }
}
/// Build a codec refusal without a nested cause.
///
/// # Arguments
/// * `message` - Diagnostic text to display without change.
///
/// # Returns
/// A command error with the supplied message and no nested cause.
pub(super) fn error(message: &str) -> CommandError {
    CommandError {
        message: message.into(),
        source: None,
    }
}

/// Remaining JSON value work shared by all nested visitors of one ledger.
struct JsonBudget {
    /// Unconsumed JSON values and keys.
    remaining: usize,
    /// Deepest permitted JSON value nesting.
    max_depth: usize,
    /// Deepest JSON value nesting consumed so far.
    peak_depth: usize,
}
/// Recursive strict JSON visitor that borrows the one remaining budget.
struct Seed<'a> {
    /// Borrowed JSON accounting; nested visitors cannot replenish it.
    budget: &'a mut JsonBudget,
    /// Nesting depth of the value that this visitor decodes.
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        if self.depth > self.budget.max_depth || self.budget.remaining == 0 {
            return Err(de::Error::custom("local JSON node/depth limit"));
        }
        self.budget.remaining -= 1;
        self.budget.peak_depth = self.budget.peak_depth.max(self.depth);
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded local JSON with unique object keys")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(Seed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values
                .try_reserve(1)
                .map_err(|_| de::Error::custom("local JSON allocation failed"))?;
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate local JSON key"));
            }
            if self.budget.remaining == 0 {
                return Err(de::Error::custom("local JSON node limit"));
            }
            self.budget.remaining -= 1;
            let value = object.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

/// Fallible output buffer that checks the byte ceiling before each allocation.
struct BoundedWriter {
    /// Serialized bytes written so far.
    bytes: Vec<u8>,
    /// Largest number of bytes that the buffer may hold.
    limit: usize,
}
impl io::Write for BoundedWriter {
    fn write(&mut self, part: &[u8]) -> io::Result<usize> {
        self.bytes
            .len()
            .checked_add(part.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| io::Error::other("local JSON output limit"))?;
        self.bytes
            .try_reserve(part.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(part);
        Ok(part.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Cumulative work that one codec ledger has consumed so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodecUsage {
    /// Serialized bytes charged so far, for input and for output.
    pub bytes: usize,
    /// JSON values and keys charged so far.
    pub nodes: usize,
    /// Deepest JSON value nesting seen so far, counting the root as one.
    pub peak_depth: usize,
    /// Current depth ceiling; [`CodecWork::constrain`] can lower it but never raise it.
    pub depth_limit: usize,
}

/// A ledger of the work that remains for the local records of one operation.
///
/// The operation keeps the ledger across all of its records. The caller maps explicit
/// operation ceilings into this ledger. It grants no authority.
pub(super) struct CodecWork {
    /// Effective ceilings. Lowering them keeps the consumed work exactly.
    ceiling: CodecLimits,
    /// Remaining serialized bytes for input and output together.
    bytes: usize,
    /// Remaining JSON values and keys, and the depth ceiling.
    json: JsonBudget,
}
impl CodecWork {
    /// Start a ledger after checking each ceiling once.
    ///
    /// # Arguments
    /// * `limits` - Explicit finite byte, node and depth ceilings.
    ///
    /// # Returns
    /// A new ledger whose remaining work equals the ceilings.
    ///
    /// # Errors
    /// Rejects a zero byte, node or depth ceiling and a depth above 127.
    pub(super) fn new(limits: CodecLimits) -> Result<Self, CommandError> {
        if !limits.valid() {
            return Err(error("invalid aggregate codec limits"));
        }
        Ok(Self {
            ceiling: limits,
            bytes: limits.bytes,
            json: JsonBudget {
                remaining: limits.nodes,
                max_depth: limits.depth,
                peak_depth: 0,
            },
        })
    }
    /// Lower the ceilings to saved ones, and keep every charge made before.
    ///
    /// A larger saved ceiling neither replenishes work nor raises the depth. On an
    /// error the ledger stays unchanged, and the owner must abort its operation.
    ///
    /// # Arguments
    /// * `limits` - Saved ceilings to intersect with the current ceilings.
    ///
    /// # Returns
    /// Unit after the ceilings are lowered and the consumed work is kept.
    ///
    /// # Errors
    /// Rejects invalid saved limits, and saved limits below work or depth already consumed.
    pub(super) fn constrain(&mut self, limits: CodecLimits) -> Result<(), CommandError> {
        if !limits.valid() {
            return Err(error("invalid saved codec limits"));
        }
        let used = self.usage();
        let effective = CodecLimits {
            bytes: self.ceiling.bytes.min(limits.bytes),
            nodes: self.ceiling.nodes.min(limits.nodes),
            depth: self.json.max_depth.min(limits.depth),
        };
        if used.bytes > effective.bytes
            || used.nodes > effective.nodes
            || used.peak_depth > effective.depth
        {
            return Err(error("saved codec limits below consumed work"));
        }
        // New ceiling minus new remaining work equals the old consumed work.
        self.ceiling = effective;
        self.bytes = effective.bytes - used.bytes;
        self.json.remaining = effective.nodes - used.nodes;
        self.json.max_depth = effective.depth;
        Ok(())
    }
    /// Observe the cumulative work without changing the ledger.
    ///
    /// # Returns
    /// The consumed bytes and nodes, the deepest nesting seen and the depth ceiling.
    pub(super) fn usage(&self) -> CodecUsage {
        CodecUsage {
            bytes: self.ceiling.bytes - self.bytes,
            nodes: self.ceiling.nodes - self.json.remaining,
            peak_depth: self.json.peak_depth,
            depth_limit: self.json.max_depth,
        }
    }
    /// Report the unused bytes, so that the owner can charge an outer ledger.
    ///
    /// # Returns
    /// The serialized byte allowance that no record has consumed yet.
    pub(super) fn remaining_bytes(&self) -> usize {
        self.bytes
    }
    /// Report the unused JSON work, so that the owner can charge an outer ledger.
    ///
    /// # Returns
    /// The JSON value and key allowance that no record has consumed yet.
    pub(super) fn remaining_nodes(&self) -> usize {
        self.json.remaining
    }
    /// Charge bytes before the caller allocates an input or output buffer of that size.
    ///
    /// # Arguments
    /// * `bytes` - Input, decode or output bytes to charge.
    ///
    /// # Returns
    /// Unit after the bytes are subtracted from the remaining work.
    ///
    /// # Errors
    /// Rejects a charge larger than the remaining byte allowance, and changes nothing.
    pub(super) fn charge(&mut self, bytes: usize) -> Result<(), CommandError> {
        self.bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or_else(|| error("aggregate codec byte limit"))?;
        Ok(())
    }
    /// Decode one JSON document, and refuse duplicate keys at every depth.
    ///
    /// The ledger charges the bytes of the document first, then one node for each value and key.
    /// A refused document keeps the work that it consumed before the refusal.
    ///
    /// # Arguments
    /// * `bytes` - Serialized JSON input.
    ///
    /// # Returns
    /// The decoded JSON value.
    ///
    /// # Errors
    /// Rejects exhausted byte, node or depth work, malformed JSON, duplicate object keys
    /// and trailing input. Keeps the decoder error as the source.
    pub(super) fn document(&mut self, bytes: &[u8]) -> Result<Value, CommandError> {
        self.charge(bytes.len())?;
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let value = Seed {
            budget: &mut self.json,
            depth: 1,
        }
        .deserialize(&mut decoder)
        .map_err(|e| CommandError::with_source(format!("invalid aggregate local JSON: {e}"), e))?;
        decoder
            .end()
            .map_err(|e| CommandError::with_source(format!("trailing local JSON: {e}"), e))?;
        Ok(value)
    }
    /// Decode a closed record, which is a JSON object with exactly the listed top-level fields.
    ///
    /// The ledger first decodes the document with [`Self::document`]. A missing field,
    /// including an absent optional field, and an unknown field are both refused. Nested
    /// objects are closed only when their Rust types use `#[serde(deny_unknown_fields)]`.
    ///
    /// # Arguments
    /// * `bytes` - Serialized JSON input.
    /// * `fields` - Every field name that the record must hold, and no other.
    ///
    /// # Returns
    /// The typed record after the strict decode and the field check.
    ///
    /// # Errors
    /// Propagates every refusal of [`Self::document`], and rejects a value that is not an
    /// object, an unknown or missing field, and a field of the wrong type.
    pub(super) fn closed_record<T: DeserializeOwned>(
        &mut self,
        bytes: &[u8],
        fields: &[&str],
    ) -> Result<T, CommandError> {
        let value = self.document(bytes)?;
        let object = value
            .as_object()
            .ok_or_else(|| error("closed local record is not an object"))?;
        if let Some(name) = object.keys().find(|name| !fields.contains(&name.as_str())) {
            return Err(error(&format!(
                "unknown closed local record field {name:?}"
            )));
        }
        if let Some(name) = fields.iter().find(|name| !object.contains_key(**name)) {
            return Err(error(&format!(
                "missing closed local record field {name:?}"
            )));
        }
        T::deserialize(value)
            .map_err(|e| CommandError::with_source(format!("invalid closed local record: {e}"), e))
    }
    /// Charge the JSON shape of a value before any output is allocated.
    ///
    /// The traversal keeps no output. It charges the same values, keys and depth that a
    /// later strict decode of the output charges again, as separate work.
    ///
    /// # Arguments
    /// * `value` - Record or value whose JSON shape is charged.
    ///
    /// # Returns
    /// Unit after the value consumes its shape allowance.
    ///
    /// # Errors
    /// Propagates serialization failures, and rejects raw JSON encodings, keys that are not
    /// strings, numbers or booleans, and exhausted node or depth work.
    pub(super) fn admit_serialized<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), CommandError> {
        serde_preflight::admit(self, value)
    }
    /// Serialize a record within the remaining bytes, then strictly decode the output.
    ///
    /// # Arguments
    /// * `record` - Typed record to serialize.
    ///
    /// # Returns
    /// The serialized bytes, after their strict decode consumed its work.
    ///
    /// # Errors
    /// Rejects output larger than the remaining bytes without a charge, and propagates
    /// exhausted work during the strict decode of the output.
    pub(super) fn encode_record<T: Serialize>(
        &mut self,
        record: &T,
    ) -> Result<Vec<u8>, CommandError> {
        let mut writer = BoundedWriter {
            bytes: Vec::new(),
            limit: self.bytes,
        };
        serde_json::to_writer(&mut writer, record)
            .map_err(|e| CommandError::with_source(format!("bounded local encode: {e}"), e))?;
        self.document(&writer.bytes)?;
        Ok(writer.bytes)
    }
}

#[cfg(test)]
#[path = "../../tests/support/storage_command_codec.rs"]
mod tests;
