//! Charge the JSON shape of a borrowed audit value without writing any output.
//!
//! ## Recommendations
//! Charge this traversal before the bytes are counted. A later strict decode of the output
//! charges its own work, separately.
use super::{CodecWork, CommandError, error};
use serde::ser::{self, Serialize};
use std::fmt;
impl ser::Error for CommandError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        error(&message.to_string())
    }
}
/// Traverse a value against the remaining value, key and depth work, and keep no output.
///
/// # Arguments
/// * `work` - Existing cumulative codec work, retained across this traversal.
/// * `value` - Record or value whose JSON shape is checked.
///
/// # Returns
/// Unit after the borrowed value consumes its shape allowance.
///
/// # Errors
/// Propagates serialization failures. Rejects an unsupported raw JSON encoding or key shape,
/// and exhausted node or depth work of the shared ledger during the traversal.
pub(super) fn admit<T: Serialize + ?Sized>(
    work: &mut CodecWork,
    value: &T,
) -> Result<(), CommandError> {
    value.serialize(Shape {
        work,
        depth: 1,
        key: false,
    })
}
/// One JSON value, or one object key at the depth of its object.
struct Shape<'a> {
    /// Borrowed cumulative codec work shared by every nested shape.
    work: &'a mut CodecWork,
    /// Depth at which the next value or key is admitted.
    depth: usize,
    /// Whether this shape represents an object key rather than a value.
    key: bool,
}
impl<'a> Shape<'a> {
    /// Charge one value or key node, as the strict decoder charges a value in its seed and a
    /// key separately in `visit_map`.
    ///
    /// # Returns
    /// Unit after charging one value or key node at the current depth.
    ///
    /// # Errors
    /// Rejects a value or key beyond the remaining node count or the permitted depth.
    fn node(&mut self) -> Result<(), CommandError> {
        if self.depth > self.work.json.max_depth || self.work.json.remaining == 0 {
            return Err(error("local JSON node/depth limit"));
        }
        self.work.json.remaining -= 1;
        if !self.key {
            self.work.json.peak_depth = self.work.json.peak_depth.max(self.depth)
        }
        Ok(())
    }
    /// Charge one scalar value or map key against the existing node and depth limits.
    ///
    /// # Returns
    /// Unit after charging the scalar or key node.
    ///
    /// # Errors
    /// Propagates the node or depth failure of the scalar, and allocates no JSON output value.
    fn scalar(mut self) -> Result<(), CommandError> {
        self.node()
    }
    /// Admit a container node and prepare its child traversal with the same work ledger.
    ///
    /// # Returns
    /// A container traversal using the same work at the child depth.
    ///
    /// # Errors
    /// Rejects a container used as a map key, and propagates node or depth exhaustion of the
    /// shared ledger.
    fn container(mut self) -> Result<Compound<'a>, CommandError> {
        if self.key {
            return Err(error("unsupported audit JSON map key"));
        }
        self.node()?;
        Ok(Compound {
            work: self.work,
            depth: self.depth + 1,
            key_depth: self.depth,
            pending_key: false,
        })
    }
    /// Admit a tag object and its key before returning the shape for its nested value.
    ///
    /// # Returns
    /// A nested-value shape after charging the tag object and key.
    ///
    /// # Errors
    /// Rejects a tagged value used as a key, and propagates exhaustion while it charges the
    /// tag object and its key.
    fn tagged(mut self) -> Result<Shape<'a>, CommandError> {
        if self.key {
            return Err(error("unsupported audit JSON map key"));
        }
        self.node()?;
        Shape {
            work: &mut *self.work,
            depth: self.depth,
            key: true,
        }
        .node()?;
        Ok(Shape {
            work: self.work,
            depth: self.depth + 1,
            key: false,
        })
    }
}
/// Refuse the private raw and number encodings of `serde_json`, so that they cannot bypass
/// the JSON shape.
///
/// # Arguments
/// * `name` - Serde type name checked for private raw or number encodings.
///
/// # Returns
/// Unit when the type name does not use a private raw JSON encoding.
///
/// # Errors
/// Rejects a private raw or number encoding of `serde_json`, which would bypass the ordinary
/// shape traversal.
fn ordinary(name: &str) -> Result<(), CommandError> {
    if name.starts_with("$serde_json::private::") {
        Err(error("unsupported audit raw JSON encoding"))
    } else {
        Ok(())
    }
}
impl<'a> ser::Serializer for Shape<'a> {
    type Ok = ();
    type Error = CommandError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;
    fn serialize_bool(self, _: bool) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_i8(self, _: i8) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_i16(self, _: i16) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_i32(self, _: i32) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_i64(self, _: i64) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_i128(self, _: i128) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_u8(self, _: u8) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_u16(self, _: u16) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_u32(self, _: u32) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_u64(self, _: u64) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_u128(self, _: u128) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_f32(self, _: f32) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_f64(self, _: f64) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_char(self, _: char) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_str(self, _: &str) -> Result<(), CommandError> {
        self.scalar()
    }
    fn serialize_bytes(self, bytes: &[u8]) -> Result<(), CommandError> {
        let mut seq = self.container()?;
        for value in bytes {
            seq.value(value)?
        }
        Ok(())
    }
    fn serialize_none(self) -> Result<(), CommandError> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), CommandError> {
        value.serialize(self)
    }
    fn serialize_unit(mut self) -> Result<(), CommandError> {
        if self.key {
            return Err(error("unsupported audit JSON map key"));
        }
        self.node()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<(), CommandError> {
        ordinary(name)?;
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), CommandError> {
        ordinary(name)?;
        self.scalar()
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<(), CommandError> {
        ordinary(name)?;
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), CommandError> {
        ordinary(name)?;
        value.serialize(self.tagged()?)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a>, CommandError> {
        self.container()
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'a>, CommandError> {
        self.container()
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, CommandError> {
        ordinary(name)?;
        self.container()
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, CommandError> {
        ordinary(name)?;
        self.tagged()?.container()
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a>, CommandError> {
        self.container()
    }
    fn serialize_struct(self, name: &'static str, _: usize) -> Result<Compound<'a>, CommandError> {
        ordinary(name)?;
        self.container()
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, CommandError> {
        ordinary(name)?;
        self.tagged()?.container()
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, _: &T) -> Result<(), CommandError> {
        self.scalar()
    }
}
/// A borrowed container whose keys and values each consume their own nodes of the shared ledger.
struct Compound<'a> {
    /// Borrowed cumulative codec work shared by container children.
    work: &'a mut CodecWork,
    /// Depth assigned to each child value in the container.
    depth: usize,
    /// Depth assigned to keys at the containing object.
    key_depth: usize,
    /// Whether a map key still requires its corresponding value.
    pending_key: bool,
}
impl Compound<'_> {
    /// Traverse one value of the container at its child depth, with the same shape ledger.
    ///
    /// # Arguments
    /// * `value` - Record or value whose JSON shape is checked.
    ///
    /// # Returns
    /// Unit after the child value consumes its structural work.
    ///
    /// # Errors
    /// Propagates nested serialization errors, unsupported JSON shapes and node or depth
    /// exhaustion from the borrowed child traversal.
    fn value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CommandError> {
        value.serialize(Shape {
            work: self.work,
            depth: self.depth,
            key: false,
        })
    }
    /// Admit a field name at the depth of its object, then traverse its value.
    ///
    /// # Arguments
    /// * `name` - Object field name, admitted at the depth of its object.
    /// * `value` - Record or value whose JSON shape is checked.
    ///
    /// # Returns
    /// Unit after the field name and value consume their respective work.
    ///
    /// # Errors
    /// Propagates key admission or nested value serialization failures, including node or
    /// depth exhaustion of the shared ledger.
    fn field<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<(), CommandError> {
        name.serialize(Shape {
            work: self.work,
            depth: self.key_depth,
            key: true,
        })?;
        self.value(value)
    }
}
/// Implement the Serde sequence traits by forwarding each element to the shared child traversal.
macro_rules! sequence {
    ($trait:ident,$method:ident) => {
        impl ser::$trait for Compound<'_> {
            type Ok = ();
            type Error = CommandError;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CommandError> {
                self.value(value)
            }
            fn end(self) -> Result<(), CommandError> {
                Ok(())
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);
impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = CommandError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), CommandError> {
        if self.pending_key {
            return Err(error("audit map key without value"));
        }
        key.serialize(Shape {
            work: self.work,
            depth: self.key_depth,
            key: true,
        })?;
        self.pending_key = true;
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CommandError> {
        if !self.pending_key {
            return Err(error("audit map value without key"));
        }
        self.pending_key = false;
        self.value(value)
    }
    fn end(self) -> Result<(), CommandError> {
        if self.pending_key {
            Err(error("audit map key without value"))
        } else {
            Ok(())
        }
    }
}
/// Implement the Serde struct traits by admitting each named field with the shared container work.
macro_rules! structure {
    ($trait:ident) => {
        impl ser::$trait for Compound<'_> {
            type Ok = ();
            type Error = CommandError;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                name: &'static str,
                value: &T,
            ) -> Result<(), CommandError> {
                self.field(name, value)
            }
            fn end(self) -> Result<(), CommandError> {
                Ok(())
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);
