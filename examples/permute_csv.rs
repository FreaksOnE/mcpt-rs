//! Example: load a Binance kline CSV and write five permuted copies of it.
//!
//! The four price columns are parsed as [`rust_decimal::Decimal`] — the type
//! [`mcpt_rs::get_permutation`] works in — so no cast happens anywhere: the
//! prices are read, permuted, and printed back as decimals.
//!
//! Usage:
//!
//! ```text
//! cargo run --example permute_csv -p mcpt-rs [input.csv] [output_dir] [start_index]
//! ```
//!
//! Defaults: the bundled `examples/BTCUSDT-1m-2026-10-06.csv`, an
//! `examples/permutations` directory next to it, and `start_index = 0`.
//! The five permutations use fixed seeds 1..=5, so reruns are reproducible;
//! pass `None` to [`get_permutation`] to draw entropy from the OS instead.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use rust_decimal::Decimal;

use mcpt_rs::Bar;
use mcpt_rs::get_permutation;

/// The four price columns, in the order [`Bar`] reads them.
const OHLC: [&str; 4] = ["open", "high", "low", "close"];

/// Number of permutations to write.
const N_PERMUTATIONS: u64 = 5;

/// One kline row: the raw CSV fields plus the four parsed prices
/// [`get_permutation`] rewrites.
#[derive(Clone)]
struct Kline {
    fields: Vec<String>,
    price_cols: [usize; 4],
    prices: [Decimal; 4],
}

impl Kline {
    /// The row re-rendered as CSV, with the permuted prices spliced into
    /// their columns and every other field carried over verbatim.
    fn to_csv(&self) -> String {
        let mut fields = self.fields.clone();
        for (k, &col) in self.price_cols.iter().enumerate() {
            fields[col] = self.prices[k].to_string();
        }
        fields.join(",")
    }
}

impl Bar for Kline {
    fn get_open(&self) -> Decimal {
        self.prices[0]
    }
    fn get_high(&self) -> Decimal {
        self.prices[1]
    }
    fn get_low(&self) -> Decimal {
        self.prices[2]
    }
    fn get_close(&self) -> Decimal {
        self.prices[3]
    }
    fn set_open(&mut self, value: Decimal) {
        self.prices[0] = value;
    }
    fn set_high(&mut self, value: Decimal) {
        self.prices[1] = value;
    }
    fn set_low(&mut self, value: Decimal) {
        self.prices[2] = value;
    }
    fn set_close(&mut self, value: Decimal) {
        self.prices[3] = value;
    }
}

/// Load `path`, parsing the four price columns directly as decimals — no
/// cast. Returns the header line alongside the rows.
fn load_klines(path: &Path) -> Result<(String, Vec<Kline>), Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    let mut lines = content.lines();
    let header = lines.next().ok_or("empty CSV")?.to_string();
    let names: Vec<&str> = header.split(',').collect();
    let mut price_cols = [0; 4];
    for (k, name) in OHLC.iter().enumerate() {
        price_cols[k] = names
            .iter()
            .position(|col| col == name)
            .ok_or_else(|| format!("missing price column '{name}'"))?;
    }
    let klines = lines
        .map(|line| -> Result<Kline, Box<dyn Error>> {
            let fields: Vec<String> =
                line.split(',').map(str::to_string).collect();
            let mut prices = [Decimal::ZERO; 4];
            for (k, &col) in price_cols.iter().enumerate() {
                prices[k] = fields[col].parse()?;
            }
            Ok(Kline { fields, price_cols, prices })
        })
        .collect::<Result<_, _>>()?;
    Ok((header, klines))
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args().skip(1);
    let input = args.next().map_or_else(
        || manifest_dir.join("examples/BTCUSDT-1m-2026-10-06.csv"),
        PathBuf::from,
    );
    let output_dir = args.next().map_or_else(
        || manifest_dir.join("examples/permutations"),
        PathBuf::from,
    );
    let start_index = args.next().map_or(Ok(0), |v| {
        v.parse().map_err(|_| format!("invalid start_index: {v}"))
    })?;

    let (header, klines) = load_klines(&input)?;
    println!("loaded {} bars from {}", klines.len(), input.display());

    fs::create_dir_all(&output_dir)?;
    for i in 1..=N_PERMUTATIONS {
        let mut permuted = klines.clone();
        let mut bars: Vec<&mut dyn Bar> =
            permuted.iter_mut().map(|kline| kline as &mut dyn Bar).collect();
        get_permutation(&mut bars, start_index, Some(i))?;
        drop(bars);

        let path = output_dir.join(format!("permutation-{i}.csv"));
        let mut out = String::new();
        out.push_str(&header);
        out.push('\n');
        for kline in &permuted {
            out.push_str(&kline.to_csv());
            out.push('\n');
        }
        fs::write(&path, out)?;
        println!("wrote {} (seed = {i})", path.display());
    }

    Ok(())
}
