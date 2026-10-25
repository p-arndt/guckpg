import { describe, expect, it } from 'vitest';
import type { SourceInfo } from '#lib/api.js';
import { GridEdits, buildChanges } from './edits.svelte.js';

// Result of `SELECT name AS label, id, upper(name) FROM users`: key is id.
const source: SourceInfo = {
	schema: 'public',
	table: 'users',
	columns: ['name', 'id', null],
	key: [1]
};
const rows = [
	['ann', '1', 'ANN'],
	['bob', '2', 'BOB'],
	['cid', '3', 'CID']
];

describe('GridEdits', () => {
	it('tracks edited cells and forgets edits back to the original', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'anna', 'ann');
		expect(e.value(0, 0, 'ann')).toBe('anna');
		expect(e.isEdited(0, 0)).toBe(true);
		expect(e.count).toBe(1);

		e.setCell(0, 0, 'ann', 'ann');
		expect(e.isEdited(0, 0)).toBe(false);
		expect(e.count).toBe(0);
	});

	it('distinguishes NULL from the original value', () => {
		const e = new GridEdits();
		e.setCell(1, 0, null, 'bob');
		expect(e.value(1, 0, 'bob')).toBeNull();
		expect(e.count).toBe(1);
	});

	it('toggles deletes and handles new rows', () => {
		const e = new GridEdits();
		e.toggleDelete(2);
		e.addRow(3);
		e.addRow(3);
		expect(e.count).toBe(3);
		e.toggleDelete(2);
		e.removeNewRow(0);
		expect(e.count).toBe(1);
		e.reset();
		expect(e.count).toBe(0);
	});
});

describe('GridEdits history', () => {
	it('undoes and redoes edits', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'a1', 'ann');
		e.setCell(0, 0, 'a2', 'ann');
		e.toggleDelete(1);
		expect(e.canUndo).toBe(true);

		e.undo();
		expect(e.deleted[1]).toBeUndefined();
		e.undo();
		expect(e.value(0, 0, 'ann')).toBe('a1');
		e.redo();
		expect(e.value(0, 0, 'ann')).toBe('a2');

		e.undo();
		e.undo();
		expect(e.count).toBe(0);
		expect(e.canUndo).toBe(false);
	});

	it('drops the redo stack on a new edit', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'a1', 'ann');
		e.undo();
		e.setCell(0, 0, 'b', 'ann');
		expect(e.canRedo).toBe(false);
	});

	it('does not record edits that change nothing', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'ann', 'ann');
		expect(e.canUndo).toBe(false);
	});

	it('undoes a batch in one step', () => {
		const e = new GridEdits();
		e.batch(() => {
			e.setCell(0, 0, 'x', 'ann');
			e.setCell(1, 0, 'y', 'bob');
		});
		expect(e.count).toBe(2);
		e.undo();
		expect(e.count).toBe(0);
	});

	it('makes discard undoable but reset final', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'x', 'ann');
		e.discard();
		expect(e.count).toBe(0);
		e.undo();
		expect(e.count).toBe(1);
		e.reset();
		expect(e.canUndo).toBe(false);
	});
});

describe('buildChanges', () => {
	it('turns edits into deletes, updates and inserts', () => {
		const e = new GridEdits();
		e.setCell(0, 0, 'anna', 'ann');
		e.setCell(0, 2, 'ignored', 'ANN');
		e.setCell(1, 0, 'gone anyway', 'bob');
		e.toggleDelete(1);
		e.addRow(3);
		e.setNewCell(0, 0, 'dan');
		e.setNewCell(0, 2, 'ignored');

		expect(buildChanges(e, rows, source)).toEqual([
			{ op: 'delete', key: [{ column: 'id', value: '2' }] },
			{
				op: 'update',
				key: [{ column: 'id', value: '1' }],
				values: [{ column: 'name', value: 'anna' }]
			},
			{ op: 'insert', values: [{ column: 'name', value: 'dan' }] }
		]);
	});

	it('keeps explicit NULLs in new rows but leaves untouched cells to the default', () => {
		const e = new GridEdits();
		e.addRow(3);
		e.setNewCell(0, 1, null);
		expect(buildChanges(e, rows, source)).toEqual([
			{ op: 'insert', values: [{ column: 'id', value: null }] }
		]);
	});

	it('skips updates that only touched expression columns', () => {
		const e = new GridEdits();
		e.setCell(2, 2, 'x', 'CID');
		expect(buildChanges(e, rows, source)).toEqual([]);
	});
});
