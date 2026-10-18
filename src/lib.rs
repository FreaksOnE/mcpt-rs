//! Monte-Carlo permutation tests for OHLC price data.
//!
//! [`get_permutation`] destroys the serial structure of OHLC bars while
//! preserving the distribution of within-bar and gap moves, which is used as
//! the null distribution in Monte-Carlo permutation tests.
//!
//! The caller supplies the bars as a slice of [`Bar`] trait objects — any
//! record type carrying `open, high, low, close` [`Decimal`] prices with
//! setters for them. The permutation reads the prices through the getters
//! and rewrites them in place through the setters; every other field of the
//! records is left untouched, and a failed call changes nothing.
//!
//! The permutation recombines relative moves, expressed as ratios:
//!
//! 1. rows before `start_index` are carried over unchanged, the bar at
//!    `start_index` anchors the permutation;
//! 2. for every later bar the relative moves are extracted — the gap
//!    (`open / prev_close`) and the intrabar ratios (`high/low/close` vs
//!    `open`);
//! 3. the intrabar ratios are shuffled with one random permutation and the
//!    gaps with a second, independent one (one pair per call — the same seed
//!    applies the same pair to every slice of equal length);
//! 4. the bars are rebuilt from the anchor by multiplying the shuffled
//!    ratios.

mod bar;
mod constants;
mod error;
mod utils;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rust_decimal::Decimal;

pub use crate::bar::Bar;
use crate::constants::*;
pub use crate::error::PermuteError;

/// Permute the OHLC prices of `bars` in place.
///
/// `bars` supplies the prices through [`Bar`]'s getters and receives the
/// permuted prices through its setters; every other field of the records is
/// left untouched. The prices must be strictly positive. Each permuted price
/// is rounded back onto the grid of the input price it replaces, keeping the
/// series on its own decimal scale; the relative moves in between are
/// computed at full decimal precision and never quantized to the price grid.
///
/// Rows before `start_index` are carried over unchanged, the bar at
/// `start_index` anchors the permutation, and every later bar is rebuilt
/// from a gap and intrabar ratios drawn from other bars. One pair of shuffles
/// is drawn per call, so the same `seed` applies the same shuffles to every
/// slice of equal length.
///
/// `seed = None` draws entropy from the OS; a fixed seed yields reproducible
/// output.
///
/// # Errors
///
/// Returns an error when a price is not strictly positive; when a relative
/// move or a permuted price does not fit [`Decimal`] (a 96-bit mantissa at a
/// scale of at most 28); or when `start_index` leaves no bar after it to
/// permute. On error `bars` is left unchanged: nothing is written before the
/// whole permutation has computed successfully.
///
/// # Example
///
/// ```
/// use rust_decimal::Decimal;
/// use rust_decimal::dec; // using macros feature
///
/// use mcpt_rs::Bar;
/// use mcpt_rs::get_permutation;
///
/// #[derive(Debug, PartialEq)]
/// struct Candle {
///     open: Decimal,
///     high: Decimal,
///     low: Decimal,
///     close: Decimal,
/// }
///
/// impl Bar for Candle {
///     fn get_open(&self) -> Decimal {
///         self.open
///     }
///     fn get_high(&self) -> Decimal {
///         self.high
///     }
///     fn get_low(&self) -> Decimal {
///         self.low
///     }
///     fn get_close(&self) -> Decimal {
///         self.close
///     }
///     fn set_open(&mut self, value: Decimal) {
///         self.open = value;
///     }
///     fn set_high(&mut self, value: Decimal) {
///         self.high = value;
///     }
///     fn set_low(&mut self, value: Decimal) {
///         self.low = value;
///     }
///     fn set_close(&mut self, value: Decimal) {
///         self.close = value;
///     }
/// }
///
/// let mut a = Candle {
///     open: dec!(100.0),
///     high: dec!(101.0),
///     low: dec!(99.0),
///     close: dec!(100.5),
/// };
/// let mut b = Candle {
///     open: dec!(100.5),
///     high: dec!(102.0),
///     low: dec!(100.0),
///     close: dec!(101.5),
/// };
/// let mut c = Candle {
///     open: dec!(101.5),
///     high: dec!(103.0),
///     low: dec!(101.0),
///     close: dec!(102.5),
/// };
///
/// let mut bars: Vec<&mut dyn Bar> = vec![&mut a, &mut b, &mut c];
/// get_permutation(&mut bars, 1, Some(42)).unwrap();
///
/// // Rows through the anchor bar are carried over unchanged.
/// assert_eq!(a.close, dec!(100.5));
/// assert_eq!(b.open, dec!(100.5));
/// ```
pub fn get_permutation(
    bars: &mut [&mut dyn Bar],
    start_index: usize,
    seed: Option<u64>,
) -> Result<(), PermuteError> {
    let perm_n = utils::permutation_len(bars.len(), start_index)?;
    let (intrabar_perm, gap_perm) = seed.map_or_else(
        || utils::draw_permutations(perm_n, &mut rand::rng()),
        |seed| {
            let mut rng = StdRng::seed_from_u64(seed);
            utils::draw_permutations(perm_n, &mut rng)
        },
    );
    permute_with(bars, start_index, &intrabar_perm, &gap_perm)
}

/// Core permutation, taking the two shuffles explicitly: rewrite the prices
/// of `bars` from shuffled relative moves, leaving every other field of the
/// records untouched. Nothing is written unless the whole permutation
/// computed successfully.
///
/// `intrabar_perm` reorders the `high/low/close` relative offsets and
/// `gap_perm` the open-vs-previous-close gaps; both must be a permutation of
/// `0..n_bars - start_index - 1`.
fn permute_with(
    bars: &mut [&mut dyn Bar],
    start_index: usize,
    intrabar_perm: &[usize],
    gap_perm: &[usize],
) -> Result<(), PermuteError> {
    let prices = utils::extract_prices(bars);
    let permuted =
        permute_prices(&prices, start_index, intrabar_perm, gap_perm)?;
    utils::replace_prices(bars, &permuted);
    Ok(())
}

/// Permute one market's `[open, high, low, close]` price bars by recombining
/// shuffled relative moves, expressed as ratios.
///
/// `prices` holds the input bars; `intrabar_perm` reorders the
/// `high/low/close` ratios and `gap_perm` the open-vs-previous-close gaps;
/// both must be a permutation of `0..n_bars - start_index - 1`.
/// Each rebuilt price is rounded back onto the grid of the input price at
/// the same row and column.
///
/// # Errors
///
/// Returns an error when a price is not strictly positive, when a relative
/// move or a permuted price does not fit [`Decimal`], or when
/// `start_index` leaves no bar after it to permute.
fn permute_prices(
    prices: &[[Decimal; 4]],
    start_index: usize,
    intrabar_perm: &[usize],
    gap_perm: &[usize],
) -> Result<Vec<[Decimal; 4]>, PermuteError> {
    let n_bars = prices.len();
    let perm_n = utils::permutation_len(n_bars, start_index)?;
    assert_eq!(intrabar_perm.len(), perm_n, "intrabar permutation length");
    assert_eq!(gap_perm.len(), perm_n, "gap permutation length");
    utils::validate_prices(prices)?;

    let perm_index = start_index + 1;
    let mut relative_open = Vec::with_capacity(perm_n);
    let mut relative_high = Vec::with_capacity(perm_n);
    let mut relative_low = Vec::with_capacity(perm_n);
    let mut relative_close = Vec::with_capacity(perm_n);
    for k in 0..perm_n {
        let i = perm_index + k;
        relative_open.push(utils::dec_div(
            prices[i][OPEN],
            prices[i - 1][CLOSE],
            i,
            OPEN,
        )?);
        relative_high.push(utils::dec_div(
            prices[i][HIGH],
            prices[i][OPEN],
            i,
            HIGH,
        )?);
        relative_low.push(utils::dec_div(
            prices[i][LOW],
            prices[i][OPEN],
            i,
            LOW,
        )?);
        relative_close.push(utils::dec_div(
            prices[i][CLOSE],
            prices[i][OPEN],
            i,
            CLOSE,
        )?);
    }

    // Shuffle the intrabar ratios and the gaps separately
    let shuffled_open: Vec<Decimal> =
        gap_perm.iter().map(|&k| relative_open[k]).collect();
    let shuffled_high: Vec<Decimal> =
        intrabar_perm.iter().map(|&k| relative_high[k]).collect();
    let shuffled_low: Vec<Decimal> =
        intrabar_perm.iter().map(|&k| relative_low[k]).collect();
    let shuffled_close: Vec<Decimal> =
        intrabar_perm.iter().map(|&k| relative_close[k]).collect();

    let mut permuted: Vec<[Decimal; 4]> = Vec::with_capacity(n_bars);
    // Carry over real data up to and including the anchor bar.
    permuted.extend_from_slice(&prices[..=start_index]);
    for i in perm_index..n_bars {
        let k = i - perm_index;
        let open = utils::rescale_to(
            utils::dec_mul(permuted[i - 1][CLOSE], shuffled_open[k], i, OPEN)?,
            prices[i][OPEN].scale(),
        );
        permuted.push([
            open,
            utils::rescale_to(
                utils::dec_mul(open, shuffled_high[k], i, HIGH)?,
                prices[i][HIGH].scale(),
            ),
            utils::rescale_to(
                utils::dec_mul(open, shuffled_low[k], i, LOW)?,
                prices[i][LOW].scale(),
            ),
            utils::rescale_to(
                utils::dec_mul(open, shuffled_close[k], i, CLOSE)?,
                prices[i][CLOSE].scale(),
            ),
        ]);
    }
    Ok(permuted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal [`Bar`] record: the four prices plus one field the
    /// permutation must not touch.
    #[derive(Clone, Debug, PartialEq)]
    struct TestBar {
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: i64,
    }

    impl Bar for TestBar {
        fn get_open(&self) -> Decimal {
            self.open
        }
        fn get_high(&self) -> Decimal {
            self.high
        }
        fn get_low(&self) -> Decimal {
            self.low
        }
        fn get_close(&self) -> Decimal {
            self.close
        }
        fn set_open(&mut self, value: Decimal) {
            self.open = value;
        }
        fn set_high(&mut self, value: Decimal) {
            self.high = value;
        }
        fn set_low(&mut self, value: Decimal) {
            self.low = value;
        }
        fn set_close(&mut self, value: Decimal) {
            self.close = value;
        }
    }

    /// 7 bars of pseudo-random OHLC, generated with `numpy.RandomState(1)`.
    const INPUT_A: &str = "\
101.25150854851499 102.09959080683379 100.78894835396898 101.6376095745095
101.17900460665173 101.62512887783664 100.65408249913676 101.01773297988676
100.36037982216911 100.55492250321888 100.15886032398545 100.48559239470953
100.14260412723853 100.49486635010226 99.0555939871368 99.41317714665028
99.24961539930187 100.2941735472781 98.9785473335949 100.277239805635
97.83782637836757 98.2243965544848 97.78974494662015 97.99567657036445
99.52921378371813 100.16050561573691 99.15736456384121 99.720520545271";

    /// Expected output produced by running the original
    /// `mcpt.bar_permute.get_permutation` with the same input and the same
    /// fixed permutations (monkeypatched `np.random.permutation`).
    const EXPECTED_SINGLE_START1: &str = "\
101.251508548515 102.09959080683375 100.78894835396898 101.63760957450954
101.17900460665174 101.6251288778366 100.65408249913675 101.0177329798868
100.85153130126031 101.49121030139206 100.47474179582665 101.04537971136443
102.6266418185102 102.82557751486652 102.42057175982586 102.75468184641815
102.08602583336474 102.48938120670367 102.0358566660277 102.25073001143676
101.90171678577443 102.26016687384065 100.79561214224861 101.15947663494546
98.69860130494092 99.73736027415363 98.42903815513854 99.72052054527099";

    const EXPECTED_START0: &str = "\
101.251508548515 102.09959080683375 100.78894835396898 101.63760957450954
101.29068911768502 101.93315359890254 100.91225888300984 101.48538164035145
103.07352935057342 103.52800706893747 102.53877834698277 102.90923799566838
102.44489523927291 103.52308228927039 102.16510030523375 103.50560337631283
102.83206088813955 103.03139477725496 102.62557835644641 102.96035720294282
100.45567240782941 100.85258603941902 100.40630446155231 100.61774619629885
100.45220260485071 100.80555387316988 99.36183189022177 99.72052054527099";

    /// Parse whitespace-separated rows of four prices into bars.
    fn parse(s: &str) -> Vec<TestBar> {
        s.lines()
            .map(|line| {
                let prices: Vec<Decimal> = line
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
                let prices: [Decimal; 4] = prices.try_into().unwrap();
                TestBar {
                    open: prices[0],
                    high: prices[1],
                    low: prices[2],
                    close: prices[3],
                    volume: 0,
                }
            })
            .collect()
    }

    /// Borrow `bars` as the trait-object slice [`get_permutation`] takes.
    fn dyn_bars(bars: &mut [TestBar]) -> Vec<&mut dyn Bar> {
        bars.iter_mut().map(|bar| bar as &mut dyn Bar).collect()
    }

    /// The `[open, high, low, close]` prices of `bars`.
    fn prices_of(bars: &[TestBar]) -> Vec<[Decimal; 4]> {
        bars.iter()
            .map(|bar| {
                [bar.get_open(), bar.get_high(), bar.get_low(), bar.get_close()]
            })
            .collect()
    }

    /// Assert element-wise closeness: `diff <= magnitude / 1e9`.
    fn assert_close(got: &[[Decimal; 4]], want: &[[Decimal; 4]]) {
        assert_eq!(got.len(), want.len());
        let pairs = got.iter().flatten().zip(want.iter().flatten());
        for (i, (x, y)) in pairs.enumerate() {
            let diff = (*x - *y).abs();
            let mag = x.abs().max(y.abs()).max(Decimal::ONE);
            assert!(
                diff <= mag / Decimal::from(1_000_000_000_u64),
                "element {i}: {x} vs {y}"
            );
        }
    }

    #[test]
    fn matches_single_market() {
        let mut bars = parse(INPUT_A);
        permute_with(
            &mut dyn_bars(&mut bars),
            1,
            &[4, 0, 3, 1, 2],
            &[2, 4, 0, 1, 3],
        )
        .unwrap();
        let want = prices_of(&parse(EXPECTED_SINGLE_START1));
        assert_close(&prices_of(&bars), &want);
    }

    #[test]
    fn matches_start_index_zero() {
        let mut bars = parse(INPUT_A);
        permute_with(
            &mut dyn_bars(&mut bars),
            0,
            &[5, 0, 3, 1, 4, 2],
            &[2, 5, 0, 1, 4, 3],
        )
        .unwrap();
        let want = prices_of(&parse(EXPECTED_START0));
        assert_close(&prices_of(&bars), &want);
    }

    #[test]
    fn seeded_is_reproducible_and_unseeded_varies() {
        let run = |seed| {
            let mut bars = parse(INPUT_A);
            get_permutation(&mut dyn_bars(&mut bars), 1, seed).unwrap();
            bars
        };
        assert_eq!(run(Some(42)), run(Some(42)));
        assert_ne!(run(None), run(None));
    }

    #[test]
    fn preserves_gap_and_intrabar_ratio_multisets() {
        // The permutation recombines gaps (open vs previous close) and
        // intrabar ratios (high/low/close vs open) drawn from other bars, so
        // close-to-close returns are NOT preserved; but the multiset of gap
        // ratios and of each intrabar ratio series is.
        let original = parse(INPUT_A);
        let mut bars = original.clone();
        let start_index = 2;
        get_permutation(&mut dyn_bars(&mut bars), start_index, Some(3))
            .unwrap();

        let original = prices_of(&original);
        let permuted = prices_of(&bars);

        let compare_sorted =
            |mut x: Vec<Decimal>, mut y: Vec<Decimal>, what: &str| {
                x.sort_unstable();
                y.sort_unstable();
                for (x, y) in x.iter().zip(&y) {
                    let diff = (*x - *y).abs();
                    let mag = x.abs().max(y.abs());
                    assert!(
                        diff <= mag / Decimal::from(1_000_000_000_u64),
                        "{what}: {x} vs {y}"
                    );
                }
            };

        let n_bars = original.len();
        let gaps = |bars: &[[Decimal; 4]]| -> Vec<Decimal> {
            (start_index + 1..n_bars)
                .map(|i| bars[i][OPEN].checked_div(bars[i - 1][CLOSE]).unwrap())
                .collect()
        };
        compare_sorted(gaps(&original), gaps(&permuted), "gap");

        for col in [HIGH, LOW, CLOSE] {
            let ratios = |bars: &[[Decimal; 4]]| -> Vec<Decimal> {
                (start_index + 1..n_bars)
                    .map(|i| bars[i][col].checked_div(bars[i][OPEN]).unwrap())
                    .collect()
            };
            compare_sorted(ratios(&original), ratios(&permuted), "intrabar");
        }
    }

    #[test]
    fn rejects_invalid_input() {
        let bars = prices_of(&parse(INPUT_A));

        // n_bars = 7, start_index = 6 leaves nothing to permute; the shuffles
        // are never reached.
        assert_eq!(
            permute_prices(&bars, 6, &[], &[]),
            Err(PermuteError::NothingToPermute { n_bars: 7, start_index: 6 })
        );

        let mut with_zero = parse(INPUT_A);
        with_zero[2].close = Decimal::ZERO;
        assert_eq!(
            permute_prices(
                &prices_of(&with_zero),
                1,
                &[4, 0, 3, 1, 2],
                &[2, 4, 0, 1, 3],
            ),
            Err(PermuteError::InvalidPrice {
                row: 2,
                col: CLOSE,
                value: Decimal::ZERO,
            })
        );
    }

    #[test]
    fn rejects_decimal_overflow() {
        // A 7e56 gap does not fit the decimal type.
        let tiny = Decimal::new(1, 28); // 1e-28
        let huge: Decimal = "70000000000000000000000000000".parse().unwrap();
        let bars = [[tiny, tiny, tiny, tiny], [huge, huge, tiny, tiny]];
        assert_eq!(
            permute_prices(&bars, 0, &[0], &[0]),
            Err(PermuteError::DecimalOverflow { row: 1, col: OPEN })
        );

        // Compounding a 1x gap with a 3x intrabar ratio overshoots the
        // largest decimal price. The third bar's moves are shuffled onto the
        // second one.
        let big: Decimal = "39000000000000000000000000000".parse().unwrap();
        let small: Decimal = "7800000000000000000000000000".parse().unwrap();
        let triple: Decimal = "23400000000000000000000000000".parse().unwrap();
        let bars = [
            [big, big, big, big],
            [small, small, small, small],
            [small, triple, small, small],
        ];
        assert_eq!(
            permute_prices(&bars, 0, &[1, 0], &[1, 0]),
            Err(PermuteError::DecimalOverflow { row: 1, col: HIGH })
        );
    }

    #[test]
    fn get_permutation_matches_core() {
        let mut bars = parse(INPUT_A);
        // The seeded path must draw the intrabar shuffle first, then the gap shuffle.
        let perm_n = utils::permutation_len(bars.len(), 1).unwrap();
        let mut rng = StdRng::seed_from_u64(9);
        let intrabar_perm = utils::random_permutation(perm_n, &mut rng);
        let gap_perm = utils::random_permutation(perm_n, &mut rng);
        let mut expected = bars.clone();
        permute_with(
            &mut dyn_bars(&mut expected),
            1,
            &intrabar_perm,
            &gap_perm,
        )
        .unwrap();
        get_permutation(&mut dyn_bars(&mut bars), 1, Some(9)).unwrap();
        assert_eq!(bars, expected);
    }

    #[test]
    fn preserves_other_fields_and_carried_rows() {
        let mut bars = parse(INPUT_A);
        for (i, bar) in bars.iter_mut().enumerate() {
            bar.volume = i as i64 + 1;
        }
        let before = bars.clone();

        get_permutation(&mut dyn_bars(&mut bars), 1, Some(7)).unwrap();

        // Only the prices change; the other fields survive untouched.
        for (got, want) in bars.iter().zip(&before) {
            assert_eq!(got.volume, want.volume);
        }
        let got = prices_of(&bars);
        let want = prices_of(&before);
        assert_ne!(got[2..], want[2..]);

        // Rows through the anchor bar are carried over unchanged.
        assert_eq!(bars[..=1], before[..=1]);
    }

    #[test]
    fn keeps_the_input_scale() {
        // Prices of any scale come back at that same scale, with rows through
        // the anchor bar untouched.
        let price = |mantissa: i128| Decimal::from_i128_with_scale(mantissa, 8);
        let mut bars = vec![
            TestBar {
                open: price(10_000_000_000),
                high: price(10_100_000_000),
                low: price(9_900_000_000),
                close: price(10_050_000_000),
                volume: 0,
            },
            TestBar {
                open: price(10_050_000_000),
                high: price(10_200_000_000),
                low: price(10_000_000_000),
                close: price(10_150_000_000),
                volume: 0,
            },
            TestBar {
                open: price(10_150_000_000),
                high: price(10_300_000_000),
                low: price(10_100_000_000),
                close: price(10_250_000_000),
                volume: 0,
            },
        ];
        let before = bars.clone();
        get_permutation(&mut dyn_bars(&mut bars), 1, Some(42)).unwrap();

        for bar in &bars {
            for value in [bar.open, bar.high, bar.low, bar.close] {
                assert_eq!(value.scale(), 8);
            }
        }
        // Rows through the anchor bar keep their exact mantissas: nothing is
        // rescaled on the way in or out.
        assert_eq!(bars[..=1], before[..=1]);
    }

    #[test]
    fn moves_survive_a_coarse_price_scale() {
        // Two-decimal prices: if the relative moves were quantized to the price scale,
        // every ratio would round to 1.00 and the permuted bars would collapse into
        // flat candles.
        let price = |mantissa: i128| Decimal::from_i128_with_scale(mantissa, 2);
        let mut bars = vec![
            TestBar {
                open: price(8_571_720),
                high: price(8_575_000),
                low: price(8_570_000),
                close: price(8_574_000),
                volume: 0,
            },
            TestBar {
                open: price(8_580_000),
                high: price(8_585_000),
                low: price(8_578_000),
                close: price(8_579_000),
                volume: 0,
            },
            TestBar {
                open: price(8_565_000),
                high: price(8_570_000),
                low: price(8_560_000),
                close: price(8_568_000),
                volume: 0,
            },
            TestBar {
                open: price(8_575_000),
                high: price(8_580_000),
                low: price(8_572_000),
                close: price(8_578_000),
                volume: 0,
            },
        ];
        get_permutation(&mut dyn_bars(&mut bars), 1, Some(5)).unwrap();

        // Every bar keeps its intrabar structure and gaps: moves of ~0.05%
        // survive, they do not round away.
        for (row, bar) in bars.iter().enumerate() {
            for value in [bar.open, bar.high, bar.low, bar.close] {
                assert_eq!(value.scale(), 2);
            }
            assert!(
                bar.low < bar.open && bar.open < bar.high,
                "row {row}: {bar:?}"
            );
        }
        for i in 2..bars.len() {
            assert_ne!(bars[i].open, bars[i - 1].close, "row {i}");
        }
    }

    #[test]
    fn get_permutation_errors_leave_bars_unchanged() {
        // Non-positive price.
        let mut bars = parse(INPUT_A);
        bars[2].close = Decimal::ZERO;
        let before = bars.clone();
        assert_eq!(
            get_permutation(&mut dyn_bars(&mut bars), 1, Some(9)),
            Err(PermuteError::InvalidPrice {
                row: 2,
                col: CLOSE,
                value: Decimal::ZERO,
            })
        );
        assert_eq!(bars, before);

        // start_index = 6 leaves nothing to permute.
        let mut bars = parse(INPUT_A);
        assert_eq!(
            get_permutation(&mut dyn_bars(&mut bars), 6, Some(9)),
            Err(PermuteError::NothingToPermute { n_bars: 7, start_index: 6 })
        );
    }
}
