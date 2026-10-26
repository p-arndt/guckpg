import { describe, expect, it } from 'vitest';
import { formattedJson, rowChanges, type RowField } from './row-form.js';
import { GridEdits, buildChanges } from './state/edits.svelte.js';

const fields: RowField[] = [
	{ name: 'id', type: 'integer', value: '7', editable: true, primaryKey: true },
	{ name: 'name', type: 'text', value: 'Original', editable: true, primaryKey: false },
	{ name: 'computed', type: 'text', value: 'ORIGINAL', editable: false, primaryKey: false }
];
describe('row form', () => {
	it('preserves exact values and excludes computed fields from changes', () => {
		expect(rowChanges(fields, ['7', 'Original', 'ORIGINAL'], false)).toEqual([]);
		expect(rowChanges(fields, ['7', 'Line 1\nLine 2', 'changed'], false)).toEqual([
			{ column: 1, value: 'Line 1\nLine 2' }
		]);
		expect(rowChanges(fields, ['7'], false)).toEqual([]);
	});
	it('distinguishes NULL, empty text and DEFAULT for inserted rows', () => {
		const inserted = fields.map((field) => ({ ...field, editable: true, value: undefined }));
		expect(rowChanges(inserted, [undefined, null, ''], true)).toEqual([
			{ column: 1, value: null },
			{ column: 2, value: '' }
		]);
		expect(rowChanges(fields, [undefined, undefined, undefined], false)).toEqual([]);
	});
	it('batches form changes into one undo step and keeps the original primary key', () => {
		const edits = new GridEdits();
		const rows = [['7', 'Original', 'ORIGINAL']];
		const changes = rowChanges(fields, ['8', 'New', 'ignored'], false);
		edits.batch(() => {
			for (const { column, value } of changes)
				if (value !== undefined) edits.setCell(0, column, value, rows[0][column]);
		});
		expect(
			buildChanges(edits, rows, {
				schema: 'public',
				table: 'items',
				columns: ['id', 'name', null],
				key: [0]
			})
		).toEqual([
			{
				op: 'update',
				key: [{ column: 'id', value: '7' }],
				values: [
					{ column: 'id', value: '8' },
					{ column: 'name', value: 'New' }
				]
			}
		]);
		edits.undo();
		expect(edits.count).toBe(0);
		edits.redo();
		expect(edits.value(0, 1, 'Original')).toBe('New');
	});
	it('restores an inserted field to its database default without sending NULL', () => {
		const edits = new GridEdits();
		edits.addRow(1);
		edits.setNewCell(0, 0, '5');
		edits.setNewCell(0, 0, undefined);
		expect(
			buildChanges(edits, [], { schema: 'public', table: 'items', columns: ['id'], key: [0] })
		).toEqual([{ op: 'insert', values: [] }]);
		edits.undo();
		expect(edits.inserted[0][0]).toBe('5');
	});
	it('formats JSON without rounding numbers or changing escaped strings', () => {
		const json = '{"id":9007199254740993,"big":1e400,"text":"x,{}\\n\\"","array":[true,null,{}]}';
		const formatted = formattedJson(json)!;
		expect(formatted).toContain('9007199254740993');
		expect(formatted).toContain('1e400');
		expect(formatted).toContain('"x,{}\\n\\""');
		expect(formattedJson(formatted)).toBe(formatted);
		expect(formattedJson('not JSON')).toBeNull();
		expect(formattedJson('[]')).toBe('[]');
	});
});
