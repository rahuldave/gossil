//! Emit bounded Fossil task ticket bytes for task creation.
//!
//! See the [task ticket guide](https://rahuldave.com/gossil/task-ticket.html)
//! for the format, caller responsibilities and a library example.
//!
//! The shared emitter of [`crate::gest_ticket`] writes the bytes. This module
//! gives it the task's eleven model fields and the kind `task`, and re-exports
//! its [`TicketError`](crate::gest_ticket::TicketError).
//!
//! Card order and token/checksum construction follow Fossil tkt.c:1837-1874
//! (b3a1c8efb2731dffbd5cc38c58ac349746de6da6). Fossil is Copyright D. Richard
//! Hipp and contributors; its BSD two-clause license is preserved in the frozen
//! source evidence. This module performs no storage, identity generation or admission.
pub use crate::gest_ticket::TicketError;
use crate::{
    gest_task_values::{TaskValues, canonical_task_fields},
    native_date::NativeDate,
};
/// Emit complete uncompressed Fossil ticket artifact bytes for a validated task creation.
///
/// The caller supplies the K card's ticket identity, actor bytes and a validated creation date.
/// K is forty lowercase ASCII hexadecimal bytes, independent of the 32-character
/// Gest ID in the task. Actor bytes need not be UTF-8 or a Gest author ID.
/// [`crate::gest_task_values`] supplies the normalized status spelling and UTC timestamp strings.
///
/// All eleven model fields use JSON followed by native token escaping. The three
/// additional J fields are `gossil_deleted=false`, raw `gossil_kind=task` and raw
/// `gossil_schema=1`. The fourteen J fields are sorted by name.
/// No append or parent card is emitted. The caller manages history and enforces
/// any requirement that artifact creation dates increase.
///
/// `max_output` bounds complete encoded bytes, including the final checksum and
/// newline, with an effective ceiling of `isize::MAX`. It is not a heap budget.
/// The emitter counts token-escaped JSON bytes before retaining serialized JSON
/// or reserving output bytes.
///
/// # Arguments
/// * `task` - Complete validated task snapshot.
/// * `ticket_id` - Independent ticket identity bytes for the K card.
/// * `actor` - Actor bytes for the U card.
/// * `date` - Validated creation instant for the D card.
/// * `max_output` - Maximum complete encoded artifact length in bytes.
/// # Returns
/// Complete uncompressed creation artifact bytes.
/// # Examples
/// Construct `task` with [`crate::gest_task_values::task_from_values`]. The
/// [task values example](https://rahuldave.com/gossil/gest-task-values.html#use-the-library)
/// shows all eleven input fields.
/// ```
/// use gossil::{gest_task_values::TaskValues, native_date::NativeDate,
///     task_ticket::{task_creation_bytes, TicketError}};
/// fn emit(task: &TaskValues) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
///     let date = NativeDate::parse(b"2026-11-18T12:00:00Z")?;
///     Ok(task_creation_bytes(task,
///         b"0123456789abcdef0123456789abcdef01234567", b"actor", date, 65_536)?)
/// }
/// ```
/// # Errors
/// Returns an error for malformed K, byte limits, allocation, token encoding
/// or JSON serialization failure.
pub fn task_creation_bytes(
    task: &TaskValues,
    ticket_id: &[u8],
    actor: &[u8],
    date: NativeDate,
    max_output: usize,
) -> Result<Vec<u8>, TicketError> {
    crate::gest_ticket::creation_bytes(
        b"task",
        &canonical_task_fields(task),
        ticket_id,
        actor,
        date,
        max_output,
    )
}
