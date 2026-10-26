import { describe, expect, it } from 'vitest';
import {
	buildRequest,
	destructive,
	renameDraftColumn,
	renameReferences,
	type TableStructure
} from './structure.js';

const original: TableStructure = {
	schema: 'public',
	name: 'items',
	comment: null,
	kind: 'table',
	columns: [
		{
			name: 'id',
			dataType: 'integer',
			nullable: false,
			default: null,
			identity: '',
			comment: null,
			generated: false
		}
	],
	constraints: [
		{
			name: 'items_key',
			kind: 'unique',
			columns: ['id'],
			refSchema: null,
			refTable: null,
			refColumns: [],
			onUpdate: null,
			onDelete: null,
			expression: null,
			definition: 'UNIQUE (id)'
		}
	],
	indexes: []
};
describe('structure change queue', () => {
	it('rejects duplicate committed names without corrupting references', () => {
		const draft = structuredClone(original);
		draft.columns.push({ ...draft.columns[0], name: 'other' });
		expect(() => renameDraftColumn(draft, draft.columns[0], 'other')).toThrow('already exists');
		expect(draft.constraints[0].columns).toEqual(['id']);
		expect(draft.columns.map((column) => column.name)).toEqual(['id', 'other']);
		renameDraftColumn(draft, draft.columns[0], 'other_id');
		expect(draft.constraints[0].columns).toEqual(['other_id']);
		expect(
			buildRequest('public', original, draft, ['id', null]).changes.map((change) => change.op)
		).toEqual(['alterColumn', 'addColumn']);
	});
	it('is empty when unchanged and preserves column rename as an alteration', () => {
		expect(buildRequest('public', original, structuredClone(original)).changes).toEqual([]);
		const draft = structuredClone(original);
		draft.columns[0].name = 'item_id';
		renameReferences(draft, 'id', 'item_id');
		const changes = buildRequest('public', original, draft, { item_id: 'id' }).changes;
		expect(changes).toEqual([
			{
				op: 'alterColumn',
				name: 'id',
				column: {
					name: 'item_id',
					dataType: 'integer',
					nullable: false,
					default: null,
					identity: '',
					comment: null
				}
			}
		]);
		expect(destructive(changes)).toBe(true);
	});
	it('keeps primary keys, self references, and indexes through a rename', () => {
		const source = structuredClone(original);
		source.constraints[0].kind = 'primaryKey';
		source.constraints.push({
			...source.constraints[0],
			name: 'self_fk',
			kind: 'foreignKey',
			refSchema: 'public',
			refTable: 'items',
			refColumns: ['id']
		});
		source.indexes.push({
			name: 'id_idx',
			columns: ['id'],
			unique: false,
			method: 'btree',
			definition: '',
			managed: false
		});
		const draft = structuredClone(source);
		draft.columns[0].name = 'renamed';
		renameReferences(draft, 'id', 'renamed');
		expect(draft.constraints[1].refColumns).toEqual(['renamed']);
		expect(
			buildRequest('public', source, draft, { renamed: 'id' }).changes.map((c) => c.op)
		).toEqual(['alterColumn']);
	});
	it('retains row identities across two renames including temporary duplicate names', () => {
		const source = structuredClone(original);
		source.constraints = [];
		source.columns.push({ ...source.columns[0], name: 'other' });
		const draft = structuredClone(source);
		draft.columns[0].name = 'other';
		draft.columns[1].name = 'third';
		draft.columns[0].name = 'first';
		const changes = buildRequest('public', source, draft, ['id', 'other']).changes;
		expect(changes.map((c) => c.op)).toEqual(['alterColumn', 'alterColumn']);
		expect(changes.map((c) => (c.op === 'alterColumn' ? [c.name, c.column.name] : []))).toEqual([
			['id', 'first'],
			['other', 'third']
		]);
	});
	it('queues a column edit and a removal', () => {
		const source = structuredClone(original);
		source.constraints = [];
		source.columns.push({ ...source.columns[0], name: 'other' });
		const draft = structuredClone(source);
		draft.columns[0].nullable = true;
		draft.columns.pop();
		expect(buildRequest('public', source, draft).changes.map((c) => c.op)).toEqual([
			'dropColumn',
			'alterColumn'
		]);
	});
	it('drops changed constraints before replacing columns and recreating constraints', () => {
		const draft = structuredClone(original);
		draft.columns = [];
		draft.constraints[0].columns = [];
		expect(buildRequest('public', original, draft).changes.map((c) => c.op)).toEqual([
			'dropConstraint',
			'dropColumn',
			'addConstraint'
		]);
	});
	it('creates the table before its indexes and constraints', () => {
		const changes = buildRequest('public', null, original).changes;
		expect(changes.map((c) => c.op)).toEqual(['createTable', 'addConstraint']);
		expect(destructive(changes)).toBe(false);
	});
});
