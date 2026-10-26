import { describe, expect, it } from 'vitest';
import {
	allDeleted,
	editChanged,
	gridItems,
	inSpan,
	openEdit,
	pastePlan,
	pickedSpan,
	planPickedDelete,
	spanItems,
	type GridItem
} from './grid-selection.js';

const row = (orig: number): GridItem => ({ kind: 'row', orig });
const fresh = (index: number): GridItem => ({ kind: 'new', index });

describe('gridItems', () => {
	it('lists loaded rows in order, then new rows', () => {
		expect(gridItems([2, 0], 2)).toEqual([row(2), row(0), fresh(0), fresh(1)]);
	});
});

describe('pickedSpan', () => {
	it('orders the ends and clamps to the rows that exist', () => {
		expect(pickedSpan({ anchor: 5, focus: 2 }, 10)).toEqual({ start: 2, end: 5 });
		expect(pickedSpan({ anchor: 1, focus: 8 }, 4)).toEqual({ start: 1, end: 3 });
	});

	it('is null without a pick or rows', () => {
		expect(pickedSpan(null, 3)).toBeNull();
		expect(pickedSpan({ anchor: 0, focus: 0 }, 0)).toBeNull();
	});

	it('answers membership and slices the items', () => {
		const span = { start: 1, end: 2 };
		expect([0, 1, 2, 3].map((i) => inSpan(span, i))).toEqual([false, true, true, false]);
		expect(inSpan(null, 0)).toBe(false);
		expect(spanItems([row(0), row(1), row(2), row(3)], span)).toEqual([row(1), row(2)]);
		expect(spanItems([row(0)], null)).toEqual([]);
	});
});

describe('planPickedDelete', () => {
	it('deletes the rows not yet deleted and removes new rows from the end', () => {
		expect(planPickedDelete([row(0), row(1), fresh(0), fresh(2)], { 1: true })).toEqual({
			restore: false,
			toggle: [0],
			remove: [2, 0]
		});
	});

	it('restores when every loaded row is already deleted, keeping new rows', () => {
		const items = [row(0), row(1), fresh(0)];
		expect(allDeleted(items, { 0: true, 1: true })).toBe(true);
		expect(planPickedDelete(items, { 0: true, 1: true })).toEqual({
			restore: true,
			toggle: [0, 1],
			remove: []
		});
	});

	it('never restores a pick of only new rows', () => {
		expect(allDeleted([fresh(0)], {})).toBe(false);
		expect(planPickedDelete([fresh(0)], {}).restore).toBe(false);
	});
});

describe('pastePlan', () => {
	it('drops cells past the last column', () => {
		expect(pastePlan({ row: 0, col: 1 }, [['a', 'b', 'c']], 3, 5)).toEqual({
			addRows: 0,
			cells: [
				{ row: 0, col: 1, value: 'a' },
				{ row: 0, col: 2, value: 'b' }
			]
		});
	});

	it('adds rows for a block that runs past the end', () => {
		const plan = pastePlan({ row: 1, col: 0 }, [['a'], [null], ['c']], 1, 2);
		expect(plan.addRows).toBe(2);
		expect(plan.cells.map((c) => c.row)).toEqual([1, 2, 3]);
		expect(plan.cells[1].value).toBeNull();
	});

	it('adds nothing for an empty block', () => {
		expect(pastePlan({ row: 4, col: 0 }, [], 1, 5)).toEqual({ addRows: 0, cells: [] });
	});
});

describe('cell edits', () => {
	it('keeps NULL and DEFAULT when the editor is committed untouched', () => {
		expect(editChanged(openEdit(0, 0, null))).toBe(false);
		expect(editChanged(openEdit(0, 0, undefined))).toBe(false);
		expect(editChanged(openEdit(0, 0, 'x'))).toBe(false);
	});

	it('writes a changed draft', () => {
		const edit = openEdit(0, 0, null);
		expect(edit.draft).toBe('');
		edit.draft = 'a';
		expect(editChanged(edit)).toBe(true);
		edit.draft = '';
		expect(editChanged(edit)).toBe(false);
	});

	it('always writes an editor started by typing', () => {
		const edit = openEdit(1, 2, 'a', 'a');
		expect(edit).toMatchObject({ row: 1, col: 2, draft: 'a' });
		expect(editChanged(edit)).toBe(true);
	});
});
