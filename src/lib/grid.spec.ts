import { describe, expect, it } from 'vitest';
import {
	MAX_COL_PX,
	MIN_COL_PX,
	CELL_TEXT_MAX,
	cellText,
	columnOffsets,
	columnWidths,
	isNumeric,
	parseTsv,
	sortOrder,
	toTsv,
	visibleColumns,
	visibleRange
} from './grid';

describe('columnWidths', () => {
	it('clamps between min and max', () => {
		const [narrow, wide] = columnWidths(['a', 'b'], [['1', 'x'.repeat(500)]]);
		expect(narrow).toBe(MIN_COL_PX);
		expect(wide).toBe(MAX_COL_PX);
	});

	it('grows with content and uses only the first line', () => {
		const [short, multi] = columnWidths(['c', 'd'], [['abcdefghijkl', 'ab\n' + 'x'.repeat(100)]]);
		expect(short).toBeGreaterThan(MIN_COL_PX);
		expect(multi).toBe(MIN_COL_PX);
	});

	it('leaves room for the type label and key icon in the header', () => {
		const rows = [['1', '2000']];
		const [bare, typed] = columnWidths(['product_count', 'product_count'], rows, [
			{},
			{ type: 'bigint' }
		]);
		expect(typed).toBeGreaterThan(bare);

		const [plain, keyed] = columnWidths(
			['id', 'id'],
			[['1', '1']],
			[{ type: 'integer' }, { type: 'integer', primaryKey: true }]
		);
		expect(keyed).toBeGreaterThan(plain);
		// "id integer" must not be squeezed to the minimum width.
		expect(plain).toBeGreaterThan(MIN_COL_PX);
	});
});

describe('isNumeric', () => {
	it('detects numbers', () => {
		expect(isNumeric('42')).toBe(true);
		expect(isNumeric('-3.14')).toBe(true);
		expect(isNumeric('1e10')).toBe(true);
		expect(isNumeric('12abc')).toBe(false);
		expect(isNumeric(null)).toBe(false);
		expect(isNumeric('')).toBe(false);
	});
});

describe('toTsv', () => {
	it('joins with tabs and quotes special values', () => {
		expect(
			toTsv(
				['id', 'note'],
				[
					['1', 'plain'],
					['2', null],
					['3', 'tab\there'],
					['4', 'say "hi"']
				]
			)
		).toBe('id\tnote\n1\tplain\n2\t\n3\t"tab\there"\n4\t"say ""hi"""');
	});

	it('can omit the header', () => {
		expect(toTsv(null, [['a', 'b']])).toBe('a\tb');
	});
});

describe('visibleRange', () => {
	it('covers the viewport plus overscan', () => {
		expect(visibleRange(0, 100, 25, 1000, 2)).toEqual({ start: 0, end: 6 });
		expect(visibleRange(250, 100, 25, 1000, 2)).toEqual({ start: 8, end: 16 });
	});

	it('stays within the row count', () => {
		expect(visibleRange(10_000, 500, 25, 10, 8)).toEqual({ start: 10, end: 10 });
		expect(visibleRange(0, 500, 25, 0)).toEqual({ start: 0, end: 0 });
	});
});

describe('parseTsv', () => {
	it('reads a single value', () => {
		expect(parseTsv('hello')).toEqual([['hello']]);
	});

	it('reads rows and columns, ignoring a trailing newline', () => {
		expect(parseTsv('a\tb\r\nc\td\n')).toEqual([
			['a', 'b'],
			['c', 'd']
		]);
	});

	it('round-trips toTsv', () => {
		const rows = [
			['1', 'tab\there', 'say "hi"'],
			['2', 'multi\nline', '']
		];
		expect(parseTsv(toTsv(null, rows))).toEqual(rows);
	});
});

describe('sortOrder', () => {
	const rows = [['10'], [null], ['9'], ['b'], ['a']];

	it('sorts numbers numerically and text alphabetically, NULLs last', () => {
		expect(sortOrder(rows, 0, 'asc')).toEqual([2, 0, 4, 3, 1]);
	});

	it('keeps NULLs last when descending', () => {
		expect(sortOrder(rows, 0, 'desc').at(-1)).toBe(1);
		expect(sortOrder([['1'], ['3'], ['2']], 0, 'desc')).toEqual([1, 2, 0]);
	});
});

describe('visibleColumns', () => {
	const offsets = columnOffsets([100, 100, 100, 100, 100, 100]);

	it('builds offsets with the total last', () => {
		expect(offsets).toEqual([0, 100, 200, 300, 400, 500, 600]);
	});

	it('covers the viewport plus overscan', () => {
		expect(visibleColumns(offsets, 250, 100, 0)).toEqual({ start: 2, end: 4 });
		expect(visibleColumns(offsets, 250, 100, 1)).toEqual({ start: 1, end: 5 });
	});

	it('clamps at both ends', () => {
		expect(visibleColumns(offsets, 0, 10_000)).toEqual({ start: 0, end: 6 });
		expect(visibleColumns(columnOffsets([]), 0, 500)).toEqual({ start: 0, end: 0 });
	});
});

describe('cellText', () => {
	it('shows the first line only', () => {
		expect(cellText('one\ntwo')).toBe('one …');
		expect(cellText('plain')).toBe('plain');
	});

	it('caps very long values', () => {
		const text = cellText('x'.repeat(CELL_TEXT_MAX * 10));
		expect(text).toBe('x'.repeat(CELL_TEXT_MAX) + ' …');
	});
});
