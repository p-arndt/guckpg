import { describe, expect, it } from 'vitest';
import type { Relation, SchemaObject } from './api.js';
import { groupRelations, groupSchemaObjects } from './objects.js';

const object = (oid: number, kind: SchemaObject['kind'], identity: string): SchemaObject => ({
	oid,
	name: 'shared_name',
	kind,
	identity
});

describe('schema browser groups', () => {
	it('keeps overloaded routines and same-named triggers distinct, with their full identities', () => {
		const objects = [
			object(1, 'function', 'shared_name(integer)'),
			object(2, 'function', 'shared_name(text)'),
			object(3, 'procedure', 'shared_name()'),
			object(4, 'trigger', 'shared_name on first_table'),
			object(5, 'trigger', 'shared_name on second_table'),
			object(6, 'sequence', 'shared_name')
		];
		const groups = groupSchemaObjects(objects);
		expect(groups.map((group) => group.label)).toEqual([
			'Functions',
			'Procedures',
			'Triggers',
			'Sequences'
		]);
		expect(groups.flatMap((group) => group.items)).toEqual(objects);
		expect(groups[0].items.map((item) => item.identity)).toEqual([
			'shared_name(integer)',
			'shared_name(text)'
		]);
		expect(groups[2].items.map((item) => item.oid)).toEqual([4, 5]);
	});

	it('does not show empty object groups', () => {
		expect(groupSchemaObjects([])).toEqual([]);
		expect(
			groupSchemaObjects([object(1, 'sequence', 'counter')]).map((group) => group.label)
		).toEqual(['Sequences']);
	});

	it('separates materialized views from tables while keeping foreign and partitioned tables', () => {
		const relations = ['table', 'partitioned', 'foreign', 'view', 'materialized'].map((kind) => ({
			name: kind,
			kind,
			estimatedRows: 0
		})) as Relation[];
		const groups = groupRelations(relations);
		expect(groups.map((group) => group.label)).toEqual(['Tables', 'Views', 'Materialized views']);
		expect(groups.map((group) => group.items.map((item) => item.name))).toEqual([
			['table', 'partitioned', 'foreign'],
			['view'],
			['materialized']
		]);
		expect(groupRelations([])).toEqual([]);
	});
});
