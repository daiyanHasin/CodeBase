// Turns a bill into one readable line.
export function formatBill(bill) {
  return `${bill.name}: ${bill.total.toFixed(2)}`;
}
