use rust_decimal::Decimal;

/// An OHLC price bar: the four prices [`get_permutation`] reads and rewrites.
///
/// Implement it on any record type; [`get_permutation`] touches only these
/// eight methods, so the other fields of the record survive a permutation.
pub trait Bar {
    /// Open price of the bar.
    fn get_open(&self) -> Decimal;
    /// High price of the bar.
    fn get_high(&self) -> Decimal;
    /// Low price of the bar.
    fn get_low(&self) -> Decimal;
    /// Close price of the bar.
    fn get_close(&self) -> Decimal;
    /// Replace the open price of the bar.
    fn set_open(&mut self, value: Decimal);
    /// Replace the high price of the bar.
    fn set_high(&mut self, value: Decimal);
    /// Replace the low price of the bar.
    fn set_low(&mut self, value: Decimal);
    /// Replace the close price of the bar.
    fn set_close(&mut self, value: Decimal);
}
