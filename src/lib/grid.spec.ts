import { describe, expect, it } from 'vitest';
import {
	MAX_COL_PX,
	MIN_COL_PX,
	CELL_TEXT_MAX,
	cellText,
	columnOffsets,
	columnWidths,
	isNumeric,
	cycleSort,
	fromTsv,
	parseTsv,
	pastedValue,
	rowAt,
	rowSpan,
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

	it('writes NULL and DEFAULT empty but an empty string quoted', () => {
		expect(toTsv(null, [[null, '', undefined, 'x']])).toBe('\t""\t\tx');
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

describe('fromTsv', () => {
	it('reads empty fields as NULL and quoted empties as empty strings', () => {
		expect(fromTsv('a\t\t""\n\tb')).toEqual([
			['a', null, ''],
			[null, 'b']
		]);
	});

	it('round-trips NULL and empty strings through toTsv', () => {
		const rows = [
			['1', null, ''],
			[null, 'say "hi"', 'multi\nline']
		];
		expect(fromTsv(toTsv(null, rows))).toEqual(rows);
	});

	it('reads a lone empty clipboard as one NULL cell', () => {
		expect(fromTsv('')).toEqual([[null]]);
	});
});

describe('pastedValue', () => {
	it('turns NULL into DEFAULT only in new rows', () => {
		expect(pastedValue(null, true)).toBeUndefined();
		expect(pastedValue(null, false)).toBeNull();
		expect(pastedValue('', true)).toBe('');
		expect(pastedValue('x', false)).toBe('x');
	});
});

describe('cycleSort', () => {
	it('goes ascending, descending, off', () => {
		const asc = cycleSort(null, 'a');
		expect(asc).toEqual({ column: 'a', dir: 'asc' });
		const desc = cycleSort(asc, 'a');
		expect(desc).toEqual({ column: 'a', dir: 'desc' });
		expect(cycleSort(desc, 'a')).toBeNull();
	});

	it('starts ascending on another column', () => {
		expect(cycleSort({ column: 'a', dir: 'desc' }, 'b')).toEqual({ column: 'b', dir: 'asc' });
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

describe('rowSpan', () => {
	it('orders the ends whichever way the drag went', () => {
		expect(rowSpan(2, 5)).toEqual({ start: 2, end: 5 });
		expect(rowSpan(5, 2)).toEqual({ start: 2, end: 5 });
		expect(rowSpan(3, 3)).toEqual({ start: 3, end: 3 });
	});
});

describe('rowAt', () => {
	it('maps a y offset below the header to a row', () => {
		expect(rowAt(32, 32, 28, 10)).toBe(0);
		expect(rowAt(32 + 28 * 3 + 5, 32, 28, 10)).toBe(3);
	});

	it('clamps above the first and below the last row', () => {
		expect(rowAt(0, 32, 28, 10)).toBe(0);
		expect(rowAt(-50, 32, 28, 10)).toBe(0);
		expect(rowAt(10_000, 32, 28, 10)).toBe(9);
		expect(rowAt(100, 32, 28, 0)).toBe(0);
	});
});

describe('sortOrder with exact numbers', () => {
	const sorted = (values: string[]) =>
		sortOrder(
			values.map((v) => [v]),
			0,
			'asc'
		).map((i) => values[i]);

	it('orders bigints beyond 2^53 exactly', () => {
		expect(sorted(['9007199254740993', '9007199254740992', '9007199254740994'])).toEqual([
			'9007199254740992',
			'9007199254740993',
			'9007199254740994'
		]);
	});

	it('handles signs, lengths, decimals and exponents', () => {
		expect(
			sorted(['10', '-2', '0.5', '-10', '0', '2', '1e1', '1.25e-1', '-0.0', '100.001'])
		).toEqual(['-10', '-2', '0', '-0.0', '1.25e-1', '0.5', '2', '10', '1e1', '100.001']);
	});

	it('puts numbers before text so mixed columns sort consistently', () => {
		expect(sorted(['b', '10', 'a', '9', '1x'])).toEqual(['9', '10', '1x', 'a', 'b']);
		// Any input order gives the same result when the comparator is transitive.
		expect(sorted(['a', '9', '1x', 'b', '10'])).toEqual(['9', '10', '1x', 'a', 'b']);
	});
});
