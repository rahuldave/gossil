//! Compute Fossil byte digests independently of repository hash policy.

/// Hash bytes with Fossil's collision-detecting SHA-1 safe-hash mode.
///
/// On an attack collision, return the mitigated digest. Like Fossil's
/// `sha1sum_blob`, this byte helper ignores the separate collision indication.
/// # Arguments
/// * `input` - Complete byte content.
/// # Returns
/// The twenty binary digest bytes.
/// # Examples
/// ```
/// assert_eq!(gossil::content_hash::sha1(b"abc").len(), 20);
/// ```
pub fn sha1(input: &[u8]) -> [u8; 20] {
    let mut hasher = sha1collisiondetection::Sha1CD::default();
    hasher.update(input);
    let mut output = sha1collisiondetection::Output::default();
    let _ = hasher.finalize_into_dirty_cd(&mut output);
    output.into()
}

/// Hash bytes with Fossil's SHA3 padding and width-dependent capacity.
///
/// This small sponge adapter ports `SHA3Init`, `SHA3Update` and `SHA3Final`;
/// the maintained `keccak` crate supplies the complete permutation. It does
/// not truncate a standard-width digest to produce a nonstandard width.
/// # Arguments
/// * `input` - Complete byte content.
/// * `bits` - A multiple of 32 from 128 through 512.
/// # Returns
/// Binary digest bytes, or `None` for an unsupported width.
/// # Examples
/// ```
/// assert_eq!(gossil::content_hash::sha3(b"abc", 160).unwrap().len(), 20);
/// ```
pub fn sha3(input: &[u8], bits: u16) -> Option<Vec<u8>> {
    if !(128..=512).contains(&bits) || !bits.is_multiple_of(32) {
        return None;
    }
    let size = usize::from(bits / 8);
    let rate = 200 - 2 * size;
    let mut state = [0_u64; 25];
    let mut loaded = 0;
    for &byte in input {
        state[loaded / 8] ^= u64::from(byte) << (8 * (loaded % 8));
        loaded += 1;
        if loaded == rate {
            keccak::f1600(&mut state);
            loaded = 0;
        }
    }
    state[loaded / 8] ^= 0x06_u64 << (8 * (loaded % 8));
    state[(rate - 1) / 8] ^= 0x80_u64 << (8 * ((rate - 1) % 8));
    keccak::f1600(&mut state);
    Some(
        (0..size)
            .map(|i| (state[i / 8] >> (8 * (i % 8))) as u8)
            .collect(),
    )
}

/// Render binary digest bytes as lowercase hexadecimal.
/// # Arguments
/// * `bytes` - Digest bytes in their original order.
/// # Returns
/// Two lowercase ASCII characters per byte.
/// # Examples
/// ```
/// assert_eq!(gossil::content_hash::hex(&[0, 255]), "00ff");
/// ```
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                char::from(DIGITS[usize::from(b >> 4)]),
                char::from(DIGITS[usize::from(b & 15)]),
            ]
        })
        .collect()
}
