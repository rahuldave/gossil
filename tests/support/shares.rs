//! Split one list of independent test cases into disjoint shares.
//!
//! A test that loops over many independent cases runs them one after another, and
//! libtest cannot run them in parallel. Each of `parts` tests takes one share
//! instead, so the threads of the test binary run the shares at once. The shares
//! are disjoint and together hold every case, so no case is left out.
//!
//! ## Recommendations
//! Give each share its own fixture and scratch directory. Write one test for each
//! share number from 0 to `parts - 1`.

/// Keep the cases whose index, modulo `parts`, is `part`.
///
/// # Arguments
/// * `cases` - Every case of the original loop, in its original order.
/// * `part` - The share that this test runs, below `parts`.
/// * `parts` - The number of tests that split the list.
///
/// # Returns
/// The cases of this share, in their original order.
///
/// # Panics
/// Panics when `part` is not below `parts`, when the share is empty, or when its
/// size differs from the size that an even split gives.
pub fn share<T>(cases: Vec<T>, part: usize, parts: usize) -> Vec<T> {
    assert!(part < parts, "share {part} of {parts} does not exist");
    let total = cases.len();
    let mine: Vec<T> = cases
        .into_iter()
        .enumerate()
        .filter(|(index, _)| index % parts == part)
        .map(|(_, case)| case)
        .collect();
    assert!(!mine.is_empty(), "share {part} of {parts} has no case");
    assert_eq!(
        mine.len(),
        total / parts + usize::from(part < total % parts)
    );
    mine
}
