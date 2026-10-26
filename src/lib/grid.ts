export type Cell = string | null;

export interface Sort {
	column: string;
	dir: 'asc' | 'desc';
}

const CHAR_PX = 7.4;
const PADDING_PX = 24;
export const MIN_COL_PX = 60;
export const MAX_COL_PX = 360;

const HEADER_GAP_PX = 6;
const KEY_ICON_PX = 12 + HEADER_GAP_PX;

export interface HeaderInfo {
	type?: string;
	primaryKey?: boolean;
}

/**
 * Width per column from the header (name, type label, key icon) and the
 * first rows, clamped.
 */
export function columnWidths(
	columns: string[],
	rows: Cell[][],
	headers: HeaderInfo[] = [],
	sample = 200
): number[] {
	return columns.map((name, c) => {
		const header = headers[c] ?? {};
		let headerPx = name.length * CHAR_PX;
		if (header.type) headerPx += HEADER_GAP_PX + header.type.length * CHAR_PX;
		if (header.primaryKey) headerPx += KEY_ICON_PX;
		let chars = 0;
		for (let r = 0; r < Math.min(rows.length, sample); r++) {
			const v = rows[r][c];
			// Only the first line counts: cells render on one line.
			const len = v === null ? 4 : v.indexOf('\n') === -1 ? v.length : v.indexOf('\n');
			if (len > chars) chars = len;
		}
		const contentPx = Math.max(headerPx, chars * CHAR_PX);
		return Math.round(Math.min(MAX_COL_PX, Math.max(MIN_COL_PX, contentPx + PADDING_PX)));
	});
}

export function isNumeric(v: Cell): boolean {
	return v !== null && /^-?\d+(\.\d+)?(e[+-]?\d+)?$/i.test(v);
}

/**
 * Tab-separated text that pastes into spreadsheets. NULL and DEFAULT (undefined)
 * become an empty field and an empty string becomes `""`, so `fromTsv` can tell
 * them apart again.
 */
export function toTsv(columns: string[] | null, rows: (Cell | undefined)[][]): string {
	const escape = (v: Cell | undefined) => {
		if (v === null || v === undefined) return '';
		if (v === '') return '""';
		return /[\t\n\r"]/.test(v) ? `"${v.replaceAll('"', '""')}"` : v;
	};
	const lines = rows.map((r) => r.map(escape).join('\t'));
	if (columns) lines.unshift(columns.map(escape).join('\t'));
	return lines.join('\n');
}

/** Rows to render for a scroll position, with some overscan. */
export function visibleRange(
	scrollTop: number,
	viewportHeight: number,
	rowHeight: number,
	total: number,
	overscan = 8
): { start: number; end: number } {
	const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
	const end = Math.min(total, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan);
	return { start: Math.min(start, end), end };
}

interface Field {
	text: string;
	quoted: boolean;
}

function parseFields(text: string): Field[][] {
	const rows: Field[][] = [];
	let row: Field[] = [];
	let cell = '';
	let wasQuoted = false;
	let quoted = false;
	let i = 0;
	const input = text.replace(/\r\n?/g, '\n').replace(/\n$/, '');
	const endCell = () => {
		row.push({ text: cell, quoted: wasQuoted });
		cell = '';
		wasQuoted = false;
	};
	while (i < input.length) {
		const c = input[i];
		if (quoted) {
			if (c === '"' && input[i + 1] === '"') {
				cell += '"';
				i += 2;
				continue;
			}
			if (c === '"') quoted = false;
			else cell += c;
		} else if (c === '"' && cell === '' && !wasQuoted) {
			quoted = true;
			wasQuoted = true;
		} else if (c === '\t') {
			endCell();
		} else if (c === '\n') {
			endCell();
			rows.push(row);
			row = [];
		} else cell += c;
		i++;
	}
	endCell();
	rows.push(row);
	return rows;
}

/**
 * Parse clipboard text from a spreadsheet or from `toTsv`: tabs between
 * cells, newlines between rows, double quotes around cells with specials.
 */
export function parseTsv(text: string): string[][] {
	return parseFields(text).map((row) => row.map((f) => f.text));
}

/**
 * Like `parseTsv`, but an empty unquoted field is NULL: that is what `toTsv`
 * writes for NULL and what a spreadsheet gives for an empty cell. `""` stays
 * an empty string.
 */
export function fromTsv(text: string): Cell[][] {
	return parseFields(text).map((row) =>
		row.map((f) => (f.text === '' && !f.quoted ? null : f.text))
	);
}

/**
 * The value a pasted cell sets. A NULL pasted into a cell of a new row means
 * "not given" (undefined, the edits store's DEFAULT), so the column default applies.
 */
export function pastedValue(value: Cell, newRow: boolean): Cell | undefined {
	return value === null && newRow ? undefined : value;
}

/** The next sort after clicking a column header: ascending, descending, off. */
export function cycleSort(current: Sort | null, column: string): Sort | null {
	if (current?.column !== column) return { column, dir: 'asc' };
	return current.dir === 'asc' ? { column, dir: 'desc' } : null;
}

const NUMBER = /^(-?)(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$/i;

/** A number as sign, significant digits and the position of the decimal point. */
interface Decimal {
	sign: -1 | 0 | 1;
	digits: string;
	point: number;
}

/** Exact, so bigint and numeric values beyond 2^53 keep their order. */
function parseDecimal(v: string): Decimal | null {
	const m = NUMBER.exec(v);
	if (!m) return null;
	const [, minus, int, frac = '', exp = '0'] = m;
	let digits = int + frac;
	let point = int.length + Number(exp);
	const lead = digits.length - digits.replace(/^0+/, '').length;
	digits = digits.slice(lead).replace(/0+$/, '');
	point -= lead;
	if (!digits) return { sign: 0, digits: '', point: 0 };
	return { sign: minus ? -1 : 1, digits, point };
}

function compareDecimal(x: Decimal, y: Decimal): number {
	if (x.sign !== y.sign) return x.sign - y.sign;
	if (x.sign === 0) return 0;
	let magnitude = x.point - y.point;
	if (magnitude === 0) magnitude = x.digits < y.digits ? -1 : x.digits > y.digits ? 1 : 0;
	return Math.sign(magnitude) * x.sign;
}

/**
 * Row indexes sorted by one column, NULLs last in both directions. Numbers
 * compare exactly and come before text, which keeps the order consistent when
 * a column mixes both. Keys are computed once per row: testing and parsing
 * inside the comparator would run them n·log(n) times, which is slow for 100k rows.
 */
export function sortOrder(rows: Cell[][], col: number, dir: 'asc' | 'desc'): number[] {
	const sign = dir === 'asc' ? 1 : -1;
	const keys = rows.map((r) => {
		const v = r[col];
		return { v, n: v === null ? null : parseDecimal(v) };
	});
	const indexes = rows.map((_, i) => i);
	return indexes.sort((a, b) => {
		const x = keys[a];
		const y = keys[b];
		if (x.v === y.v) return 0;
		if (x.v === null) return 1;
		if (y.v === null) return -1;
		if (x.n && y.n) return compareDecimal(x.n, y.n) * sign;
		if (x.n) return -sign;
		if (y.n) return sign;
		return x.v.localeCompare(y.v) * sign;
	});
}

/** Left edge of each column, plus the total width as the last entry. */
export function columnOffsets(widths: number[]): number[] {
	const offsets = [0];
	for (const w of widths) offsets.push(offsets[offsets.length - 1] + w);
	return offsets;
}

/** Columns overlapping the viewport `[left, left + width)`, with overscan. */
export function visibleColumns(
	offsets: number[],
	left: number,
	width: number,
	overscan = 2
): { start: number; end: number } {
	const count = offsets.length - 1;
	let start = 0;
	while (start < count && offsets[start + 1] <= left) start++;
	let end = start;
	while (end < count && offsets[end] < left + width) end++;
	return { start: Math.max(0, start - overscan), end: Math.min(count, end + overscan) };
}

/** Longest text a cell renders; the rest is only visible in the value dialog. */
export const CELL_TEXT_MAX = 500;

/** A cell's text for the grid: first line only, capped. */
export function cellText(v: string): string {
	const nl = v.indexOf('\n');
	const line = nl === -1 ? v : v.slice(0, nl);
	const capped = line.length > CELL_TEXT_MAX ? line.slice(0, CELL_TEXT_MAX) : line;
	return capped.length < v.length ? capped + ' …' : capped;
}

/** The rows from one picked row to another, inclusive, in order. */
export function rowSpan(a: number, b: number): { start: number; end: number } {
	return { start: Math.min(a, b), end: Math.max(a, b) };
}

/** The row under a y offset in the scrolled content (header included), clamped to the rows. */
export function rowAt(contentY: number, headerPx: number, rowPx: number, total: number): number {
	const row = Math.floor((contentY - headerPx) / rowPx);
	return Math.min(Math.max(0, total - 1), Math.max(0, row));
}
