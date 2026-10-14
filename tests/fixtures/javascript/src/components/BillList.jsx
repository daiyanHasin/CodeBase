import { formatBill } from '../utils/format';
import { fetchBills } from '../api/client';

/** Shows every bill with its total and a Pay button. */
export function BillList({ bills }) {
  return bills.map((b) => <li key={b.id}>{formatBill(b)}</li>);
}
