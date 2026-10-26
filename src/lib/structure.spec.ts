import { describe, expect, it } from 'vitest';
import {
	buildRequest,
	changeStatus,
	checkExpression,
	columnRenames,
	defaultPresets,
	destructive,
	matchReferences,
	namedStatus,
	removedItems,
	renameDraftColumn,
	renameReferences,
	restoreColumn,
	restored,
	suggestName,
	supportsIdentity,
	toggleForeignKeyColumn,
	togglePrimaryKey,
	type StructureConstraint,
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

describe('designer helpers', () => {
	it('marks altered and dropped objects from the request', () => {
		const source = structuredClone(original);
		source.columns.push({ ...source.columns[0], name: 'other' });
		const draft = structuredClone(source);
		draft.columns[0].comment = 'changed';
		draft.columns.pop();
		draft.constraints[0].columns = [];
		const status = changeStatus(buildRequest('public', source, draft, ['id']));
		expect([...status.alteredColumns]).toEqual(['id']);
		expect(status.droppedColumns).toEqual(['other']);
		expect(status.droppedConstraints).toEqual(['items_key']);
		expect(namedStatus('items_key', source.constraints, status.addedConstraints)).toBe('changed');
		expect(namedStatus('fresh', source.constraints, status.addedConstraints)).toBe('new');
		expect(removedItems(source.constraints, draft.constraints, status.droppedConstraints)).toEqual(
			[]
		);
		draft.constraints = [];
		expect(
			removedItems(source.constraints, draft.constraints, ['items_key']).map((c) => c.name)
		).toEqual(['items_key']);
	});
	it('toggles columns in and out of the primary key', () => {
		const draft = structuredClone(original);
		draft.constraints = [];
		draft.columns[0].nullable = true;
		togglePrimaryKey(draft, draft.columns[0]);
		expect(draft.constraints).toMatchObject([
			{ name: 'items_pkey', kind: 'primaryKey', columns: ['id'] }
		]);
		expect(draft.columns[0].nullable).toBe(false);
		togglePrimaryKey(draft, draft.columns[0]);
		expect(draft.constraints).toEqual([]);
	});
	it('suggests unique postgres-style names', () => {
		const draft = structuredClone(original);
		draft.constraints[0].name = 'items_id_key';
		expect(suggestName(draft, 'unique', ['id'])).toBe('items_id_key1');
		expect(suggestName(draft, 'unique', ['id'], draft.constraints[0])).toBe('items_id_key');
		expect(suggestName(draft, 'index', ['a', 'b'])).toBe('items_a_b_idx');
		expect(suggestName({ ...draft, name: '' }, 'primaryKey', ['id'])).toBe('table_pkey');
	});
	it('offers identity and defaults that fit the type', () => {
		expect(supportsIdentity('bigint')).toBe(true);
		expect(supportsIdentity('text')).toBe(false);
		expect(defaultPresets('uuid').map((p) => p.value)).toEqual(['gen_random_uuid()', null]);
		expect(defaultPresets('timestamp with time zone')[0].value).toBe('CURRENT_TIMESTAMP');
		expect(defaultPresets('boolean').map((p) => p.value)).toEqual(['true', 'false', null]);
		expect(defaultPresets('geometry')).toEqual([{ label: 'No default', value: null }]);
	});
	it('restores dropped columns in place and follows renames for constraints', () => {
		const source = structuredClone(original);
		source.columns.push({ ...source.columns[0], name: 'other' });
		const draft = structuredClone(source);
		const names: (string | null)[] = ['id', 'other'];
		draft.columns.shift();
		names.shift();
		restoreColumn(source, draft, names, 'id');
		expect(draft.columns.map((c) => c.name)).toEqual(['id', 'other']);
		expect(names).toEqual(['id', 'other']);
		expect(() => restoreColumn(source, draft, names, 'id')).toThrow('already exists');
		draft.columns[0].name = 'item_id';
		expect(restored(source.constraints[0], columnRenames(draft, names)).columns).toEqual([
			'item_id'
		]);
	});
	it('keeps foreign key columns paired with referenced columns', () => {
		const fk: StructureConstraint = {
			...original.constraints[0],
			kind: 'foreignKey',
			columns: [],
			refColumns: []
		};
		const options = [{ name: 'code' }, { name: 'id', primaryKey: true }];
		toggleForeignKeyColumn(fk, 'owner', options);
		toggleForeignKeyColumn(fk, 'code', options);
		expect(fk.columns).toEqual(['owner', 'code']);
		expect(fk.refColumns).toEqual(['id', 'code']);
		toggleForeignKeyColumn(fk, 'owner', options);
		expect(fk.refColumns).toEqual(['code']);
		expect(matchReferences(['owner', 'code'], options)).toEqual(['id', 'code']);
		expect(matchReferences(['a', 'b'], options)).toEqual(['id', '']);
	});
	it('builds a simple check expression', () => {
		expect(checkExpression('price', '>=', '0')).toBe('"price" >= 0');
		expect(checkExpression('state', '<>', "it's")).toBe(`"state" <> 'it''s'`);
		expect(checkExpression('', '>', '1')).toBeNull();
	});
});
