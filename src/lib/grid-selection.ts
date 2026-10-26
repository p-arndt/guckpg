import { rowSpan, type Cell } from '#lib/grid.js';

/** A displayed row: a loaded row (by index into the result rows) or a new one. */
export type GridItem = { kind: 'row'; orig: number } | { kind: 'new'; index: number };

/** Whole rows picked from the row numbers; `focus` is the moving end. */
export interface RowPick {
	anchor: number;
	focus: number;
}

export interface Span {
	start: number;
	end: number;
}

/** Loaded rows in display order, then the rows added in this session. */
export function gridItems(order: readonly number[], insertedCount: number): GridItem[] {
	return [
		...order.map((orig) => ({ kind: 'row' as const, orig })),
		...Array.from({ length: insertedCount }, (_, index) => ({ kind: 'new' as const, index }))
	];
}

/** The picked rows clamped to the rows that still exist, or null when nothing is picked. */
export function pickedSpan(pick: RowPick | null, total: number): Span | null {
	if (!pick || !total) return null;
	const { start, end } = rowSpan(pick.anchor, pick.focus);
	return { start, end: Math.min(end, total - 1) };
}

export function inSpan(span: Span | null, index: number): boolean {
	return span !== null && index >= span.start && index <= span.end;
}

export function spanItems(items: readonly GridItem[], span: Span | null): GridItem[] {
	return span ? items.slice(span.start, span.end + 1) : [];
}

function loadedRows(items: readonly GridItem[]): number[] {
	return items.flatMap((item) => (item.kind === 'row' ? [item.orig] : []));
}

/** True when the items contain loaded rows and every one of them is marked deleted. */
export function allDeleted(items: readonly GridItem[], deleted: Record<number, true>): boolean {
	const loaded = loadedRows(items);
	return loaded.length > 0 && loaded.every((orig) => deleted[orig]);
}

export interface DeletePlan {
	/** Restoring when every picked loaded row is already deleted, deleting otherwise. */
	restore: boolean;
	/** Loaded rows whose delete mark has to flip. */
	toggle: number[];
	/** New rows to remove, highest index first so removals do not shift the rest. */
	remove: number[];
}

/** Delete the picked rows, or restore them when all of them are already deleted. */
export function planPickedDelete(
	items: readonly GridItem[],
	deleted: Record<number, true>
): DeletePlan {
	const restore = allDeleted(items, deleted);
	const toggle = loadedRows(items).filter((orig) => Boolean(deleted[orig]) === restore);
	const remove = restore
		? []
		: items.flatMap((item) => (item.kind === 'new' ? [item.index] : [])).sort((a, b) => b - a);
	return { restore, toggle, remove };
}

export interface PasteCell {
	row: number;
	col: number;
	value: Cell;
}

/**
 * Where a pasted block lands from `start`: cells past the last column are dropped, and rows
 * past the end are added like a spreadsheet does (`addRows`).
 */
export function pastePlan(
	start: { row: number; col: number },
	block: readonly (readonly Cell[])[],
	columnCount: number,
	rowCount: number
): { addRows: number; cells: PasteCell[] } {
	const cells: PasteCell[] = [];
	block.forEach((values, r) =>
		values.forEach((value, c) => {
			const col = start.col + c;
			if (col < columnCount) cells.push({ row: start.row + r, col, value });
		})
	);
	const lastRow = start.row + block.length - 1;
	return { addRows: block.length ? Math.max(0, lastRow - rowCount + 1) : 0, cells };
}

export interface CellEdit {
	row: number;
	col: number;
	draft: string;
	/** The draft as the editor opened, when it opened from the cell value rather than a typed key. */
	opened: string | null;
}

/** An inline editor on a cell; NULL and DEFAULT open as an empty draft. */
export function openEdit(
	row: number,
	col: number,
	current: Cell | undefined,
	typed?: string
): CellEdit {
	const draft = typed ?? current ?? '';
	return { row, col, draft, opened: typed === undefined ? draft : null };
}

/**
 * Whether closing the editor should write its draft. An editor left untouched writes nothing,
 * so opening a NULL or DEFAULT cell and pressing Enter does not turn it into ''.
 */
export function editChanged(edit: CellEdit): boolean {
	return edit.opened === null || edit.draft !== edit.opened;
}
