import { describe, expect, it } from 'vitest';
import type { ForeignKey } from './api.js';
import { referenceQuery, singleColumnReferences } from './foreign-key-picker.js';

const key: ForeignKey = {
	name: 'fk',
	fromTable: 'orders',
	fromColumns: ['customer'],
	toTable: 'people',
	toColumns: ['id']
};

describe('foreign key picker', () => {
	it('offers unambiguous single-column keys for the source table', () => {
		const composite = { ...key, fromColumns: ['a', 'b'], toColumns: ['x', 'y'] };
		expect(
			singleColumnReferences([key, composite, { ...key, fromTable: 'other' }], 'orders').customer
		).toEqual(key);
		expect(
			singleColumnReferences([key, { ...key, name: 'other' }], 'orders').customer
		).toBeUndefined();
	});
	it('quotes identifiers and bounds the reference query', () => {
		expect(referenceQuery('sch"ema', { ...key, toTable: 'people"x', toColumns: ['i"d'] })).toBe(
			'SELECT "i""d" FROM "sch""ema"."people""x" ORDER BY 1 LIMIT 101'
		);
	});
});
