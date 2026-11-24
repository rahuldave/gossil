//! The `authors` table: the humans and agents who create notes, events and other records.
//!
//! Ported from `src/store/model/author.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::{AuthorType, Id};
use crate::gest_author_values::AuthorValues;

/// An author who can create notes, events, and other actions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// Whether the author is a human or an automated agent.
    author_type: AuthorType,
    /// When the author was first recorded.
    created_at: DateTime<Utc>,
    /// Optional contact email address.
    email: Option<String>,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Display name.
    name: String,
    /// When the author record was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// Create a new author with a fresh [`Id`] and timestamps set to now.
    ///
    /// # Arguments
    ///
    /// * `name` - the display name
    /// * `author_type` - whether the author is a human or an agent
    ///
    /// # Returns
    ///
    /// The author, with no email address.
    ///
    /// # Panics
    ///
    /// Panics when the system clock is before 1970 or the operating system
    /// gives no random bytes, as upstream does.
    pub fn new(name: impl Into<String>, author_type: AuthorType) -> Self {
        let now = super::now();
        Self {
            author_type,
            created_at: now,
            email: None,
            id: Id::new(),
            name: name.into(),
            updated_at: now,
        }
    }

    /// Set the email address.
    ///
    /// # Arguments
    ///
    /// * `email` - the address, which replaces any earlier one
    ///
    /// # Returns
    ///
    /// The same author, with the email address set.
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Whether the author is a human or an agent.
    ///
    /// # Returns
    ///
    /// A copy of the author type.
    pub fn author_type(&self) -> AuthorType {
        self.author_type
    }

    /// When the author was first recorded.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The contact email address.
    ///
    /// # Returns
    ///
    /// The exact address, or `None` when the author has none. An empty address is not `None`.
    pub fn email(&self) -> &Option<String> {
        &self.email
    }

    /// The stable identifier of the author.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The display name.
    ///
    /// # Returns
    ///
    /// The exact name, which may be empty.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// When the author record was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds an author from a record that [`crate::gest_author_values::author_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<AuthorValues> for Model {
    /// Move each validated field into the model.
    fn from(values: AuthorValues) -> Self {
        Self {
            author_type: values.author_type,
            created_at: values.created_at,
            email: values.email,
            id: values.gest_id,
            name: values.name,
            updated_at: values.updated_at,
        }
    }
}
