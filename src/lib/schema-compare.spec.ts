import { describe, expect, it } from 'vitest';
import { compareSchemas, type SchemaSnapshot } from './schema-compare.js';

describe('compareSchemas', () => {
	it('reports missing relations and changed columns without comparing row estimates', () => {
		const source: SchemaSnapshot = {
			relations: [
				{
					relation: { name: 'users', kind: 'table', estimatedRows: 1 },
					columns: [
						{ name: 'id', dataType: 'integer', notNull: true, default: null, primaryKey: true }
					]
				}
			]
		};
		const target: SchemaSnapshot = {
			relations: [
				{
					relation: { name: 'users', kind: 'table', estimatedRows: 100 },
					columns: [
						{ name: 'id', dataType: 'bigint', notNull: true, default: null, primaryKey: true }
					]
				},
				{ relation: { name: 'extra', kind: 'view', estimatedRows: null }, columns: [] }
			]
		};
		expect(compareSchemas(source, target)).toEqual([
			{ object: 'extra', detail: 'Only in target' },
			{ object: 'users.id', detail: 'dataType: integer → bigint' }
		]);
	});
});
