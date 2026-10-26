import { describe, expect, it } from 'vitest';
import { groupByKind } from './palette';

describe('groupByKind', () => {
	it('groups kinds in order of their first item and keeps the order inside a group', () => {
		const items = [
			{ kind: 'Table', id: 1 },
			{ kind: 'Action', id: 2 },
			{ kind: 'Table', id: 3 },
			{ kind: 'Tab', id: 4 },
			{ kind: 'Action', id: 5 }
		];
		expect(groupByKind(items).map((i) => i.id)).toEqual([1, 3, 2, 5, 4]);
	});

	it('leaves an empty list empty', () => {
		expect(groupByKind([])).toEqual([]);
	});
});
