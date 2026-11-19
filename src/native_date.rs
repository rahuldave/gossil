//! Represent Fossil artifact creation dates as Gregorian UTC integer nanoseconds.
//!
//! Here, native dates are the creation dates in Fossil artifact D cards.
//! The supported range is years 0001 through 9999. This date domain is separate from Gest
//! business timestamp parsing in [`crate::gest_task_values`]. Display always uses
//! nine fractional digits without a UTC suffix.
use chrono::{Datelike, NaiveDate, Timelike};
use std::fmt;
/// Validated time in years 0001 through 9999 with nanosecond precision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NativeDate(i128);
/// Input is outside the supported typed native date grammar or range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DateError;
impl fmt::Display for DateError {
    /// Describe a rejected native date without echoing its input.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unsupported typed native date")
    }
}
impl std::error::Error for DateError {}
impl NativeDate {
    /// Parse exactly `YYYY-MM-DDTHH:MM:SS[.fraction][Z]`, rejecting other spellings.
    /// A missing suffix means UTC. A present fraction has one through nine digits.
    ///
    /// # Arguments
    /// * `input` - ASCII native UTC date bytes.
    /// # Returns
    /// The validated instant.
    /// # Examples
    /// ```
    /// use gossil::native_date::NativeDate;
    /// let date = NativeDate::parse(b"1969-12-31T23:59:59.999999999Z")?;
    /// assert_eq!(date.epoch_nanos(), -1);
    /// assert_eq!(date.to_string(), "1969-12-31T23:59:59.999999999");
    /// # Ok::<(), gossil::native_date::DateError>(())
    /// ```
    /// # Errors
    /// Rejects invalid grammar or calendars, year zero, offsets, leap seconds
    /// and fractions over nine digits.
    pub fn parse(input: &[u8]) -> Result<Self, DateError> {
        let b = input.strip_suffix(b"Z").unwrap_or(input);
        if b.len() < 19
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
        {
            return Err(DateError);
        }
        /// Read fixed ASCII decimal digits with checked arithmetic.
        ///
        /// # Errors
        /// Returns [`DateError`] for non-digits or arithmetic overflow.
        fn digits(b: &[u8]) -> Result<u32, DateError> {
            b.iter().try_fold(0u32, |n, c| {
                if c.is_ascii_digit() {
                    n.checked_mul(10)
                        .and_then(|n| n.checked_add(u32::from(c - b'0')))
                        .ok_or(DateError)
                } else {
                    Err(DateError)
                }
            })
        }
        let year = digits(&b[..4])?;
        if year == 0 {
            return Err(DateError);
        }
        let date = NaiveDate::from_ymd_opt(year as i32, digits(&b[5..7])?, digits(&b[8..10])?)
            .ok_or(DateError)?;
        let fraction = if b.len() == 19 {
            0
        } else {
            if !(21..=29).contains(&b.len()) || b[19] != b'.' {
                return Err(DateError);
            }
            digits(&b[20..])?
                .checked_mul(10u32.pow((29 - b.len()) as u32))
                .ok_or(DateError)?
        };
        let time = date
            .and_hms_nano_opt(
                digits(&b[11..13])?,
                digits(&b[14..16])?,
                digits(&b[17..19])?,
                fraction,
            )
            .ok_or(DateError)?;
        let nanos = i128::from(time.and_utc().timestamp())
            .checked_mul(1_000_000_000)
            .and_then(|n| n.checked_add(i128::from(fraction)))
            .ok_or(DateError)?;
        Self::from_epoch_nanos(nanos)
    }
    /// Construct a supported instant from signed epoch nanoseconds.
    /// # Arguments
    /// * `nanos` - Signed nanoseconds since the Unix epoch.
    /// # Returns
    /// The validated instant.
    /// # Errors
    /// Rejects instants outside years 0001 through 9999.
    pub fn from_epoch_nanos(nanos: i128) -> Result<Self, DateError> {
        let seconds = i64::try_from(nanos.div_euclid(1_000_000_000)).map_err(|_| DateError)?;
        let dt = chrono::DateTime::from_timestamp(seconds, nanos.rem_euclid(1_000_000_000) as u32)
            .ok_or(DateError)?;
        if !(1..=9999).contains(&dt.year()) {
            return Err(DateError);
        }
        Ok(Self(nanos))
    }
    /// Return signed epoch nanoseconds, without floating-point conversion.
    /// # Returns
    /// Signed nanoseconds since the Unix epoch.
    pub fn epoch_nanos(self) -> i128 {
        self.0
    }
}
impl fmt::Display for NativeDate {
    /// Format canonical UTC with nine fractional digits and no suffix.
    ///
    /// # Errors
    /// Returns a formatter error if conversion or writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Private construction guarantees this conversion is in range.
        let dt = chrono::DateTime::from_timestamp(
            self.0.div_euclid(1_000_000_000) as i64,
            self.0.rem_euclid(1_000_000_000) as u32,
        )
        .ok_or(fmt::Error)?;
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            dt.nanosecond()
        )
    }
}
