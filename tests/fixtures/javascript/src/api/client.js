// Talks to the payments server.
export async function fetchBills() {
  const r = await fetch('/api/bills');
  return r.json();
}
