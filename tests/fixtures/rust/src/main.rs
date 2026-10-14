//! Entry point: reads bills and prints the total.
mod bill;
use crate::bill::Bill;

fn main() {
    let b = Bill { amount: 3 };
    println!("{}", bill::total(&[b]));
}
