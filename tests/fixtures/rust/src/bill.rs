//! What a bill is and how to add bills up.
pub struct Bill { pub amount: u64 }

pub fn total(bills: &[Bill]) -> u64 { bills.iter().map(|b| b.amount).sum() }
