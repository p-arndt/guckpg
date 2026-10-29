import { describe, expect, it } from 'vitest';
import { isComplete, takesValue } from './table-filter.js';

describe('table filter conditions', () => {
	it('needs a column and, unless the operator takes none, a value', () => {
		expect(isComplete({ column: 'a', op: 'contains', value: 'x', connector: 'AND' })).toBe(true);
		expect(isComplete({ column: 'a', op: 'contains', value: '', connector: 'AND' })).toBe(false);
		expect(isComplete({ column: '', op: 'isNull', connector: 'AND' })).toBe(false);
		expect(isComplete({ column: 'a', op: 'isNull', connector: 'OR' })).toBe(true);
		expect(takesValue('isNotNull')).toBe(false);
		expect(takesValue('gte')).toBe(true);
	});
});
