import { formatBill } from './format';
test('formats', () => { expect(formatBill({ name: 'a', total: 1 })).toBe('a: 1.00'); });
