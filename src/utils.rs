use rand::Rng;
use rand::RngExt;
use rust_decimal::Decimal;

use crate::PermuteError;
pub use crate::bar::Bar;
use crate::constants::*;

/// Divide two prices into a relative move — a dimensionless ratio kept at
/// full decimal precision, never quantized to the price grid — attributing
/// an overflow to `row`, `col`.
pub fn dec_div(
    l: Decimal,
    r: Decimal,
    row: usize,
    col: usize,
) -> Result<Decimal, PermuteError> {
    l.checked_div(r).ok_or(PermuteError::DecimalOverflow { row, col })
}

/// Multiply a price by a relative move, attributing an overflow to `row`,
/// `col`.
pub fn dec_mul(
    l: Decimal,
    r: Decimal,
    row: usize,
    col: usize,
) -> Result<Decimal, PermuteError> {
    l.checked_mul(r).ok_or(PermuteError::DecimalOverflow { row, col })
}

/// Round a rebuilt price back onto the price grid of the input price it
/// replaces: `scale` decimal places, halves to even, padded out to exactly
/// `scale`.
pub fn rescale_to(value: Decimal, scale: u32) -> Decimal {
    let mut value = value.round_dp(scale);
    value.rescale(scale);
    value
}

/// Read the four prices of every bar through the [`Bar`] getters.
pub fn extract_prices(bars: &[&mut dyn Bar]) -> Vec<[Decimal; 4]> {
    bars.iter()
        .map(|bar| {
            [bar.get_open(), bar.get_high(), bar.get_low(), bar.get_close()]
        })
        .collect()
}

/// Write the permuted bars back through the [`Bar`] setters.
pub fn replace_prices(bars: &mut [&mut dyn Bar], permuted: &[[Decimal; 4]]) {
    for (bar, &prices) in bars.iter_mut().zip(permuted) {
        bar.set_open(prices[OPEN]);
        bar.set_high(prices[HIGH]);
        bar.set_low(prices[LOW]);
        bar.set_close(prices[CLOSE]);
    }
}

/// Every price must be strictly positive.
pub fn validate_prices(prices: &[[Decimal; 4]]) -> Result<(), PermuteError> {
    for (row, bar) in prices.iter().enumerate() {
        for (col, &value) in bar.iter().enumerate() {
            if value <= Decimal::ZERO {
                return Err(PermuteError::InvalidPrice { row, col, value });
            }
        }
    }
    Ok(())
}

/// Number of bars after the anchor — the length of both shuffles.
pub fn permutation_len(
    n_bars: usize,
    start_index: usize,
) -> Result<usize, PermuteError> {
    start_index
        .checked_add(1)
        .filter(|&perm_index| perm_index < n_bars)
        .map(|perm_index| n_bars - perm_index)
        .ok_or(PermuteError::NothingToPermute { n_bars, start_index })
}

/// Draw the intrabar shuffle and then the gap shuffle from `rng`. The draw
/// order is part of the contract: it fixes what a given seed produces.
pub fn draw_permutations(
    perm_n: usize,
    rng: &mut impl Rng,
) -> (Vec<usize>, Vec<usize>) {
    (random_permutation(perm_n, rng), random_permutation(perm_n, rng))
}

/// Fisher–Yates shuffle of `0..n` (the `np.random.permutation` equivalent).
pub fn random_permutation(n: usize, rng: &mut impl Rng) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        idx.swap(i, rng.random_range(0..=i));
    }
    idx
}
