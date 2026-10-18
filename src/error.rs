use std::fmt;

use rust_decimal::Decimal;

/// Errors produced by the permutation core and returned by
/// [`get_permutation`](crate::get_permutation).
#[derive(Debug, Clone, PartialEq)]
pub enum PermuteError {
    /// Fewer than one bar follows `start_index`, so there is nothing to
    /// permute.
    NothingToPermute { n_bars: usize, start_index: usize },
    /// Prices must be strictly positive. `value` is the offending price and
    /// `col` is the offset within the `[open, high, low, close]` bar.
    InvalidPrice { row: usize, col: usize, value: Decimal },
    /// A relative move or a permuted price does not fit [`Decimal`] — its
    /// 96-bit mantissa holds values below about 7.9e28 at a scale of at most
    /// 28. `col` is the offset within the `[open, high, low, close]` bar.
    DecimalOverflow { row: usize, col: usize },
}

impl fmt::Display for PermuteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingToPermute { n_bars, start_index } => write!(
                f,
                "nothing to permute: start_index {start_index} leaves no \
                 bars (n_bars = {n_bars}, need at least start_index + 2)"
            ),
            Self::InvalidPrice { row, col, value } => write!(
                f,
                "invalid price {value} at row {row}, column {col} \
                 (prices must be > 0)"
            ),
            Self::DecimalOverflow { row, col } => write!(
                f,
                "a relative move or permuted price at row {row}, column \
                 {col} does not fit Decimal (96-bit mantissa, scale up to 28)"
            ),
        }
    }
}

impl std::error::Error for PermuteError {}
