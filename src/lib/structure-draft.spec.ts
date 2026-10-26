import { describe, expect, it } from 'vitest';
import {
	addDraftColumn,
	addDraftConstraint,
	addDraftIndex,
	buildRequest,
	designerReadOnlyReason,
	draftChangeCount,
	referenceLoads,
	referenceOptions,
	referenceTables,
	removeDraftColumn,
	renameDraftTable,
	toggleConstraintColumn,
	toggleIndexColumn,
	type TableStructure
} from './structure.js';

function blank(name = ''): TableStructure {
	return {
		schema: 'public',
		name,
		comment: null,
		kind: 'r',
		columns: [],
		constraints: [],
		indexes: []
	};
}

/** items(id, owner) with a primary key, a foreign key on owner and an index on both. */
function items(): { draft: TableStructure; names: (string | null)[]; auto: WeakSet<object> } {
	const draft = blank('items');
	const names: (string | null)[] = [];
	const auto = new WeakSet<object>();
	addDraftColumn(draft, names).name = 'id';
	addDraftColumn(draft, names).name = 'owner';
	const pk = addDraftConstraint(draft, 'public', 'primaryKey');
	auto.add(pk);
	toggleConstraintColumn(draft, pk, 'id', [], auto);
	const fk = addDraftConstraint(draft, 'public', 'foreignKey');
	auto.add(fk);
	fk.refTable = 'users';
	toggleConstraintColumn(draft, fk, 'owner', [{ name: 'id', primaryKey: true }], auto);
	const idx = addDraftIndex(draft);
	auto.add(idx);
	toggleIndexColumn(draft, idx, 'id', auto);
	toggleIndexColumn(draft, idx, 'owner', auto);
	return { draft, names, auto };
}

describe('designer draft mutations', () => {
	it('adds empty columns, constraints and indexes with suggested names', () => {
		const { draft, names } = items();
		expect(names).toEqual([null, null]);
		expect(draft.columns[1]).toMatchObject({ name: 'owner', dataType: 'text', nullable: true });
		expect(addDraftColumn(blank(), [])).toMatchObject({ dataType: 'text', identity: '' });
		expect(draft.constraints.map((c) => [c.name, c.columns, c.refColumns])).toEqual([
			['items_pkey', ['id'], []],
			['items_owner_fkey', ['owner'], ['id']]
		]);
		expect(draft.constraints[1].refSchema).toBe('public');
		expect(draft.constraints[0].refSchema).toBeNull();
		expect(draft.indexes[0]).toMatchObject({
			name: 'items_id_owner_idx',
			method: 'btree',
			managed: false
		});
	});

	it('makes primary key columns not null', () => {
		const { draft } = items();
		expect(draft.columns[0].nullable).toBe(false);
		expect(draft.columns[1].nullable).toBe(true);
	});

	it('keeps a typed name once the item is no longer auto-named', () => {
		const { draft, auto } = items();
		const idx = draft.indexes[0];
		idx.name = 'mine';
		auto.delete(idx);
		toggleIndexColumn(draft, idx, 'owner', auto);
		expect(idx).toMatchObject({ name: 'mine', columns: ['id'] });
	});

	it('removes a column from keys and indexes and drops the ones left empty', () => {
		const { draft, names } = items();
		draft.constraints.push({
			...draft.constraints[0],
			name: 'items_check',
			kind: 'check',
			columns: [],
			expression: 'owner > 0'
		});
		removeDraftColumn(draft, names, 1);
		expect(draft.columns.map((c) => c.name)).toEqual(['id']);
		expect(names).toEqual([null]);
		expect(draft.constraints.map((c) => c.name)).toEqual(['items_pkey', 'items_check']);
		expect(draft.indexes[0].columns).toEqual(['id']);
		removeDraftColumn(draft, names, 0);
		expect(draft.constraints.map((c) => c.name)).toEqual(['items_check']);
		expect(draft.indexes).toEqual([]);
	});

	it('keeps referenced columns aligned when removing a column of a composite foreign key', () => {
		const { draft, names, auto } = items();
		const fk = draft.constraints[1];
		toggleConstraintColumn(draft, fk, 'id', [{ name: 'id' }, { name: 'tenant' }], auto);
		fk.refColumns = ['id', 'tenant'];
		removeDraftColumn(draft, names, 1);
		expect(fk.columns).toEqual(['id']);
		expect(fk.refColumns).toEqual(['tenant']);
	});

	it('renames self references and auto-named items with the table', () => {
		const { draft, auto } = items();
		const self = addDraftConstraint(draft, 'public', 'foreignKey');
		self.refTable = 'items';
		const elsewhere = addDraftConstraint(draft, 'public', 'foreignKey');
		elsewhere.refSchema = 'other';
		elsewhere.refTable = 'items';
		renameDraftTable(draft, 'public', 'orders', auto);
		expect(draft.name).toBe('orders');
		expect(self.refTable).toBe('orders');
		expect(elsewhere.refTable).toBe('items');
		expect(draft.constraints[0].name).toBe('orders_pkey');
		expect(draft.indexes[0].name).toBe('orders_id_owner_idx');
		expect(self.name).toBe('items_fkey');
	});
});

describe('foreign key references', () => {
	it('offers the draft columns for a self reference of a new table', () => {
		const { draft } = items();
		const fk = draft.constraints[1];
		fk.refTable = 'items';
		expect(referenceOptions(draft, null, 'public', {}, fk)).toEqual([
			{ name: 'id', primaryKey: true },
			{ name: 'owner', primaryKey: false }
		]);
	});

	it('offers loaded columns otherwise', () => {
		const { draft } = items();
		const fk = draft.constraints[1];
		const loaded = { 'public.users': [{ name: 'id', primaryKey: true }] };
		expect(referenceOptions(draft, null, 'public', loaded, fk)).toEqual(loaded['public.users']);
		fk.refTable = 'items';
		expect(referenceOptions(draft, draft, 'public', {}, fk)).toEqual([]);
	});

	it('fetches each referenced schema and table once', () => {
		const { draft } = items();
		const again = addDraftConstraint(draft, 'public', 'foreignKey');
		again.refTable = 'users';
		const remote = addDraftConstraint(draft, 'public', 'foreignKey');
		remote.refSchema = 'auth';
		remote.refTable = 'accounts';
		addDraftConstraint(draft, 'public', 'foreignKey');
		const needed = referenceLoads(draft, 'public');
		expect(needed.schemas).toEqual(['auth']);
		expect(needed.foreignKeys).toEqual([draft.constraints[1], remote]);
	});

	it('lists tables of the referenced schema with the draft table first', () => {
		const { draft } = items();
		const fk = draft.constraints[1];
		const relations = { public: [{ name: 'users' }], other: [{ name: 'tags' }] };
		expect(referenceTables(draft, 'public', relations, fk)).toEqual(['items', 'users']);
		expect(referenceTables(draft, 'public', { public: [{ name: 'items' }] }, fk)).toEqual([
			'items'
		]);
		fk.refSchema = 'other';
		expect(referenceTables(draft, 'public', relations, fk)).toEqual(['tags']);
		expect(referenceTables(draft, 'public', {}, fk)).toEqual([]);
	});
});

describe('designer status', () => {
	it('counts nothing for an untouched create form or after a failed reload', () => {
		const draft = blank();
		const request = buildRequest('public', null, draft, []);
		expect(draftChangeCount(draft, request, undefined, false)).toBe(0);
		draft.name = 'items';
		const named = buildRequest('public', null, draft, []);
		expect(draftChangeCount(draft, named, undefined, false)).toBe(named.changes.length);
		expect(named.changes.length).toBeGreaterThan(0);
		expect(draftChangeCount(draft, named, undefined, true)).toBe(0);
	});

	it('explains why editing is locked, most urgent reason first', () => {
		const base = { reloadRequired: false, readOnly: false, table: 'items', original: blank() };
		expect(designerReadOnlyReason({ ...base, original: { ...blank(), kind: 'r' } })).toBe('');
		expect(designerReadOnlyReason({ ...base, reloadRequired: true, readOnly: true })).toBe(
			'Reload to keep editing.'
		);
		expect(designerReadOnlyReason({ ...base, readOnly: undefined })).toBe('Read-only connection.');
		expect(designerReadOnlyReason({ ...base, original: null })).toBe(
			'The structure could not be loaded.'
		);
		expect(designerReadOnlyReason({ ...base, original: { ...blank(), kind: 'v' } })).toBe(
			'Only plain tables can be edited here.'
		);
		expect(designerReadOnlyReason({ ...base, table: undefined, original: null })).toBe('');
	});
});
