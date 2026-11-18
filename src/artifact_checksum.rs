//! Compute legacy MD5 checksums for Fossil artifact bytes, separate from content identity.
//!
//! This byte primitive supports future artifact validation. It parses no artifact
//! and exposes no filesystem or CLI adapter. MD5 supplies legacy compatibility,
//! not a security hash.

use md5::{Digest, Md5};

/// Compute an artifact checksum incrementally with owned state and fixed memory use.
///
/// Each value owns its state. Finishing consumes it. Create a fresh value to
/// begin another computation. No C string scanning or global reset is exposed.
/// # Examples
/// ```
/// use gossil::artifact_checksum::{md5, Md5Checksum};
/// let mut checksum = Md5Checksum::new();
/// checksum.update(b"a");
/// checksum.update(b"bc");
/// assert_eq!(checksum.finish(), md5(b"abc"));
/// ```
#[derive(Default)]
pub struct Md5Checksum(Md5);

impl Md5Checksum {
    /// Start a fresh checksum.
    /// # Returns
    /// An independent computation, initially containing no bytes.
    pub fn new() -> Self {
        Self::default()
    }
    /// Append all bytes, including zero and non-UTF-8 bytes.
    ///
    /// Empty updates leave the computation unchanged. No input is retained.
    /// # Arguments
    /// * `bytes` - Bounded bytes to append in order.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    /// Consume this computation and return sixteen binary digest bytes.
    /// # Returns
    /// MD5 of all appended bytes in order, including the empty input.
    pub fn finish(self) -> [u8; 16] {
        self.0.finalize().into()
    }
}

/// Compute the legacy MD5 checksum of the complete bounded byte slice.
///
/// This checksum is separate from repository content identity policy.
/// # Arguments
/// * `bytes` - Complete content, with no text encoding or terminator rules.
/// # Returns
/// The sixteen binary digest bytes.
/// # Examples
/// ```
/// assert_eq!(gossil::artifact_checksum::md5(b"abc"),
///     [0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0,
///      0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1, 0x7f, 0x72]);
/// ```
pub fn md5(bytes: &[u8]) -> [u8; 16] {
    let mut checksum = Md5Checksum::new();
    checksum.update(bytes);
    checksum.finish()
}
