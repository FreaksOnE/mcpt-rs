# mcpt-rs

[![crates.io](https://img.shields.io/crates/v/mcpt-rs.svg)](https://crates.io/crates/mcpt-rs)
[![docs.rs](https://img.shields.io/docsrs/mcpt-rs)](https://docs.rs/mcpt-rs)

Monte-Carlo permutation tests for OHLC price data. `get_permutation` destroys
the serial structure of OHLC bars while preserving the distribution of
within-bar and gap moves, which is used as the null distribution in
Monte-Carlo permutation tests.

Score a strategy or statistic on the real series, then on many permuted
series: the permuted results form the null distribution the real one is
measured against. Prices are `rust_decimal::Decimal` end to end — no casts,
no floats.

## How it works

The permutation recombines relative moves, expressed as ratios:

1. Rows before `start_index` are carried over unchanged; the bar at
   `start_index` anchors the permutation.
2. For every later bar the relative moves are extracted — the gap
   (`open / prev_close`) and the intrabar ratios (`high`/`low`/`close` vs
   `open`).
3. The intrabar ratios are shuffled with one random permutation and the gaps
   with a second, independent one.
4. The bars are rebuilt from the anchor by multiplying the shuffled ratios.

Each permuted price is rounded back onto the grid of the input price it
replaces, so the series keeps its own decimal scale; the relative moves in
between are computed at full decimal precision and never quantized to the
price grid.

## Usage

Implement `Bar` on any record type carrying `open, high, low, close` prices.
`get_permutation` touches only its eight methods, so every other field of the
record survives a permutation.

```rust
use rust_decimal::Decimal;
use rust_decimal::dec;

use mcpt_rs::{Bar, get_permutation};

fn d(s: &str) -> Decimal {
    s.parse().expect("valid decimal literal")
}

#[derive(Debug)]
struct Candle {
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal, // never touched by the permutation
}

impl Bar for Candle {
    fn get_open(&self) -> Decimal { self.open }
    fn get_high(&self) -> Decimal { self.high }
    fn get_low(&self) -> Decimal { self.low }
    fn get_close(&self) -> Decimal { self.close }
    fn set_open(&mut self, value: Decimal) { self.open = value; }
    fn set_high(&mut self, value: Decimal) { self.high = value; }
    fn set_low(&mut self, value: Decimal) { self.low = value; }
    fn set_close(&mut self, value: Decimal) { self.close = value; }
}

fn main() {
    let mut a = Candle {
        open: dec!(100.0), high: dec!(101.0), low: dec!(99.0),
        close: dec!(100.5), volume: dec!(12.5),
    };
    let mut b = Candle {
        open: dec!(100.5), high: dec!(102.0), low: dec!(100.0),
        close: dec!(101.5), volume: dec!(8.0),
    };
    let mut c = Candle {
        open: dec!(101.5), high: dec!(103.0), low: dec!(101.0),
        close: dec!(102.5), volume: dec!(15.25),
    };

    let mut bars: Vec<&mut dyn Bar> = vec![&mut a, &mut b, &mut c];
    get_permutation(&mut bars, 1, Some(42)).unwrap();

    // Rows through the anchor bar are carried over unchanged.
    assert_eq!(a.close, dec(100.5));
    assert_eq!(b.open, dec(100.5));
    // Everything outside the four prices survives.
    assert_eq!(c.volume, dec(15.25));
}
```

## Determinism

- `seed = None` draws entropy from the OS.
- A fixed `seed` yields reproducible output. One pair of shuffles is drawn per
  call, so the same seed applies the same shuffles to every slice of equal
  length — handy for building ensembles of permuted series that share one
  reordering.

## Errors

`get_permutation` returns `Result<(), PermuteError>`:

- `PermuteError::NothingToPermute` — `start_index` leaves no bar after it to
  permute.
- `PermuteError::InvalidPrice` — a price is not strictly positive.
- `PermuteError::DecimalOverflow` — a relative move or a permuted price does
  not fit `Decimal` (96-bit mantissa, scale up to 28).

On error the bars are left unchanged: nothing is written before the whole
permutation has computed successfully.

## Example: permuting a kline CSV

`examples/permute_csv.rs` loads a Binance kline CSV, permutes it five times
with fixed seeds 1..=5, and writes the results — every non-price column
carried over verbatim:

```sh
cargo run --example permute_csv -p mcpt-rs [input.csv] [output_dir] [start_index]
```

Defaults: the bundled `examples/BTCUSDT-1m-2026-10-06.csv`, an
`examples/permutations` directory next to it, and `start_index = 0`.

## Installation

```sh
cargo add mcpt-rs
```

Requires Rust 1.85 or later (edition 2024).

## License

MIT. See [LICENSE](LICENSE).
