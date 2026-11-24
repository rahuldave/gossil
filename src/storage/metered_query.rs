//! Metered SQL queries that charge their results to the operation's existing allowances.
//!
//! A metered query first counts the rows and payload bytes of its result with a
//! preflight query. It charges that size to the caller's ledgers, and only then
//! runs the query itself. These helpers are internal. They wrap caller SQL in a
//! common table expression, so only fixed internal SQL may reach them.
//!
//! ## Recommendations
//! Pass fixed internal SELECT statements only, and keep one borrowed [`SqlWork`]
//! through the complete operation.
use super::{StorageError, WriteTransaction, content::Budget};
use libsql::Value;

/// Largest result column count that a metered query accepts.
const MAX_COLUMNS: usize = 128;
/// Fixed SQL payload bytes charged for each result cell, beside its text or blob bytes.
const CELL_BYTES: u64 = 16;
/// SQL payload bytes charged for the one preflight row of two integer cells.
const PREFLIGHT_BYTES: u64 = 2 * CELL_BYTES;

/// Remaining SQL result rows and payload bytes of one operation.
///
/// A metered query subtracts from these counts and never adds to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SqlWork {
    /// Result rows that the operation may still materialize, preflight rows included.
    pub rows: u64,
    /// Result payload bytes that the operation may still materialize.
    pub bytes: u64,
}
impl SqlWork {
    /// Debit rows and bytes together, or neither.
    ///
    /// # Arguments
    /// * `rows` - Result rows to subtract.
    /// * `bytes` - Result payload bytes to subtract.
    ///
    /// # Returns
    /// Unit after both counts are subtracted.
    ///
    /// # Errors
    /// Rejects a debit larger than either remaining count, and changes neither count.
    fn debit(&mut self, rows: u64, bytes: u64) -> Result<(), StorageError> {
        let rows = self
            .rows
            .checked_sub(rows)
            .ok_or_else(|| StorageError::state("metered query row limit"))?;
        let bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or_else(|| StorageError::state("metered query byte limit"))?;
        *self = Self { rows, bytes };
        Ok(())
    }
}

/// Exact result size that the preflight query reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResultSize {
    /// Number of result rows.
    rows: u64,
    /// Sum of the text and blob bytes of every result cell.
    payload: u64,
}
impl ResultSize {
    /// Decode the one preflight row of a row count and a payload sum.
    ///
    /// # Arguments
    /// * `rows` - Materialized preflight result.
    ///
    /// # Returns
    /// The nonnegative row count and payload sum.
    ///
    /// # Errors
    /// Rejects a result that is not one row of two nonnegative integers.
    fn decode(rows: &[Vec<Value>]) -> Result<Self, StorageError> {
        match rows {
            [row] => match row.as_slice() {
                [Value::Integer(count), Value::Integer(payload)] => u64::try_from(*count)
                    .ok()
                    .zip(u64::try_from(*payload).ok())
                    .map(|(rows, payload)| Self { rows, payload })
                    .ok_or_else(|| StorageError::state("metered query negative size")),
                _ => Err(StorageError::state("metered query size class")),
            },
            _ => Err(StorageError::state("metered query size unavailable")),
        }
    }
    /// Compute the SQL payload charge of this result.
    ///
    /// # Arguments
    /// * `columns` - Exact result column count.
    ///
    /// # Returns
    /// Sixteen bytes for each cell plus the text and blob bytes.
    ///
    /// # Errors
    /// Rejects arithmetic overflow.
    fn sql_bytes(self, columns: usize) -> Result<u64, StorageError> {
        self.rows
            .checked_mul(columns as u64)
            .and_then(|cells| cells.checked_mul(CELL_BYTES))
            .and_then(|bytes| bytes.checked_add(self.payload))
            .ok_or_else(|| StorageError::state("metered query size overflow"))
    }
    /// Compute the decoded content retained by the materialized result.
    ///
    /// # Arguments
    /// * `columns` - Exact result column count.
    ///
    /// # Returns
    /// The size of one `Value` for each cell plus the text and blob bytes.
    ///
    /// # Errors
    /// Rejects arithmetic overflow and a total that does not fit `usize`.
    fn retained_bytes(self, columns: usize) -> Result<usize, StorageError> {
        self.rows
            .checked_mul(columns as u64)
            .and_then(|cells| cells.checked_mul(std::mem::size_of::<Value>() as u64))
            .and_then(|bytes| bytes.checked_add(self.payload))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or_else(|| StorageError::state("metered query size overflow"))
    }
    /// Require the materialized result to have the size that the preflight charged.
    ///
    /// # Arguments
    /// * `output` - Materialized result rows.
    /// * `columns` - Exact result column count.
    ///
    /// # Returns
    /// The unchanged rows when their count, width and text and blob bytes match.
    ///
    /// # Errors
    /// Rejects a different row count, a row of another width and a different sum of
    /// text and blob bytes.
    fn admit(
        self,
        output: Vec<Vec<Value>>,
        columns: usize,
    ) -> Result<Vec<Vec<Value>>, StorageError> {
        let payload = output
            .iter()
            .flatten()
            .map(|value| match value {
                Value::Text(text) => text.len() as u64,
                Value::Blob(blob) => blob.len() as u64,
                _ => 0,
            })
            .try_fold(0u64, u64::checked_add);
        let same = output.len() as u64 == self.rows
            && output.iter().all(|row| row.len() == columns)
            && payload == Some(self.payload);
        same.then_some(output)
            .ok_or_else(|| StorageError::state("metered query inventory changed"))
    }
}

/// Build the preflight query that counts the rows and payload bytes of `sql`.
///
/// # Arguments
/// * `sql` - Fixed internal SELECT statement, without a trailing semicolon.
/// * `columns` - Exact result column count, from 1 to 128.
///
/// # Returns
/// A query of one row, with the row count and the sum of text and blob bytes.
///
/// # Errors
/// Rejects a column count of zero or above 128.
fn preflight_sql(sql: &str, columns: usize) -> Result<String, StorageError> {
    if columns == 0 || columns > MAX_COLUMNS {
        return Err(StorageError::state("metered query column limit"));
    }
    let names = (0..columns)
        .map(|i| format!("c{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let lengths = (0..columns)
        .map(|i| {
            format!(
                "CASE WHEN typeof(c{i}) IN ('text','blob') THEN length(CAST(c{i} AS BLOB)) ELSE 0 END"
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    Ok(format!(
        "WITH gossil_metered({names}) AS ({sql}) SELECT count(*),coalesce(sum({lengths}),0) FROM gossil_metered"
    ))
}

/// Compute the decoded content retained by bound parameters and the preflight row.
///
/// # Arguments
/// * `params` - Bound parameters, which the preflight clones.
///
/// # Returns
/// One `Value` for each parameter and preflight cell, plus the parameter text and blob bytes.
///
/// # Errors
/// Rejects arithmetic overflow.
fn parameter_bytes(params: &[Value]) -> Result<usize, StorageError> {
    let cell = std::mem::size_of::<Value>();
    params
        .iter()
        .try_fold(2 * cell, |total, value| {
            let payload = match value {
                Value::Text(text) => text.len(),
                Value::Blob(blob) => blob.len(),
                _ => 0,
            };
            total.checked_add(cell)?.checked_add(payload)
        })
        .ok_or_else(|| StorageError::state("metered parameter size overflow"))
}

impl WriteTransaction<'_> {
    /// Run the preflight query and charge its own row before the result is sized.
    ///
    /// # Arguments
    /// * `sql` - Fixed internal SELECT statement.
    /// * `params` - Bound parameters, cloned for the preflight.
    /// * `columns` - Exact result column count.
    /// * `work` - The operation's remaining SQL allowance.
    ///
    /// # Returns
    /// The exact size of the result of `sql`.
    ///
    /// # Errors
    /// Rejects an invalid column count, exhausted SQL work and engine failures.
    async fn preflight(
        &mut self,
        sql: &str,
        params: &[Value],
        columns: usize,
        work: &mut SqlWork,
    ) -> Result<ResultSize, StorageError> {
        let query = preflight_sql(sql, columns)?;
        work.debit(1, PREFLIGHT_BYTES)?;
        let sizes = self.query_bound(&query, params.to_vec()).await?;
        ResultSize::decode(&sizes)
    }

    /// Charge the result of a fixed query to the SQL allowance, then materialize it.
    ///
    /// Use it when the caller already charged the retained content of the result.
    ///
    /// # Arguments
    /// * `sql` - Fixed internal SELECT statement, without a trailing semicolon.
    /// * `params` - Bound parameters.
    /// * `columns` - Exact result column count.
    /// * `work` - The operation's remaining SQL allowance.
    ///
    /// # Returns
    /// Every result row, after the SQL allowance admitted them.
    ///
    /// # Errors
    /// Returns an earlier latched failure, and latches an invalid column count, exhausted
    /// work and engine failures.
    pub(super) async fn query_bound_sql_work(
        &mut self,
        sql: &str,
        params: Vec<Value>,
        columns: usize,
        work: &mut SqlWork,
    ) -> Result<Vec<Vec<Value>>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let size = self.preflight(sql, &params, columns, work).await?;
            work.debit(size.rows, size.sql_bytes(columns)?)?;
            let output = self.query_bound(sql, params).await?;
            size.admit(output, columns)
        }
        .await;
        self.record(result)
    }

    /// Charge the result of a fixed query to the SQL and content allowances, then
    /// materialize it.
    ///
    /// # Arguments
    /// * `sql` - Fixed internal SELECT statement, without a trailing semicolon.
    /// * `params` - Bound parameters, charged to `content` before the preflight clones them.
    /// * `columns` - Exact result column count.
    /// * `work` - The operation's remaining SQL allowance.
    /// * `content` - The operation's decoded content budget, with no reset.
    ///
    /// # Returns
    /// Every result row, after both allowances admitted them.
    ///
    /// # Errors
    /// Returns an earlier latched failure, and latches an invalid column count, exhausted
    /// SQL or content work and engine failures.
    pub(super) async fn query_bound_work(
        &mut self,
        sql: &str,
        params: Vec<Value>,
        columns: usize,
        work: &mut SqlWork,
        content: &mut Budget,
    ) -> Result<Vec<Vec<Value>>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            content.charge_decoded(parameter_bytes(&params)?)?;
            let size = self.preflight(sql, &params, columns, work).await?;
            work.debit(size.rows, size.sql_bytes(columns)?)?;
            content.charge_decoded(size.retained_bytes(columns)?)?;
            let output = self.query_bound(sql, params).await?;
            size.admit(output, columns)
        }
        .await;
        self.record(result)
    }
}

#[cfg(test)]
#[path = "../../tests/support/storage_metered_query.rs"]
mod tests;
