import type { Change, ColumnValue, SourceInfo } from '#lib/api.js';
import type { Cell } from '#lib/grid.js';

/** A cell of a new row that was never set: the column default applies. */
export const DEFAULT = undefined;
export type NewCell = Cell | typeof DEFAULT;

interface Snapshot {
	updates: Record<number, Record<number, Cell>>;
	deleted: Record<number, true>;
	inserted: NewCell[][];
}

const HISTORY_LIMIT = 200;

/**
 * Pending edits of one result grid, saved in one go like in Navicat. Rows
 * are addressed by their index in the loaded result, not the sorted view.
 */
export class GridEdits {
	updates = $state<Record<number, Record<number, Cell>>>({});
	deleted = $state<Record<number, true>>({});
	inserted = $state<NewCell[][]>([]);
	#undo = $state<Snapshot[]>([]);
	#redo = $state<Snapshot[]>([]);

	get canUndo(): boolean {
		return this.#undo.length > 0;
	}

	get canRedo(): boolean {
		return this.#redo.length > 0;
	}

	#snapshot(): Snapshot {
		return $state.snapshot({
			updates: this.updates,
			deleted: this.deleted,
			inserted: this.inserted
		}) as Snapshot;
	}

	#restore(s: Snapshot) {
		this.updates = s.updates;
		this.deleted = s.deleted;
		this.inserted = s.inserted;
	}

	/** Call before every user-visible mutation so it can be undone. */
	#checkpoint() {
		this.#undo = [...this.#undo.slice(-(HISTORY_LIMIT - 1)), this.#snapshot()];
		this.#redo = [];
	}

	undo() {
		const previous = this.#undo.at(-1);
		if (!previous) return;
		this.#redo = [...this.#redo, this.#snapshot()];
		this.#undo = this.#undo.slice(0, -1);
		this.#restore(previous);
	}

	redo() {
		const next = this.#redo.at(-1);
		if (!next) return;
		this.#undo = [...this.#undo, this.#snapshot()];
		this.#redo = this.#redo.slice(0, -1);
		this.#restore(next);
	}

	/** Group several mutations (a multi-cell paste) into one undo step. */
	batch(fn: () => void) {
		this.#checkpoint();
		this.#batching = true;
		try {
			fn();
		} finally {
			this.#batching = false;
		}
	}

	#batching = false;

	#mutate(fn: () => void) {
		if (!this.#batching) this.#checkpoint();
		fn();
	}

	get count(): number {
		return (
			Object.keys(this.updates).length + Object.keys(this.deleted).length + this.inserted.length
		);
	}

	/** The value to show for a loaded cell, edited or not. */
	value(row: number, col: number, original: Cell): Cell {
		const edited = this.updates[row];
		return edited && col in edited ? edited[col] : original;
	}

	isEdited(row: number, col: number): boolean {
		return col in (this.updates[row] ?? {});
	}

	setCell(row: number, col: number, value: Cell, original: Cell) {
		const current = this.value(row, col, original);
		if (value === current) return;
		this.#mutate(() => {
			const edited = { ...(this.updates[row] ?? {}) };
			if (value === original) delete edited[col];
			else edited[col] = value;
			if (Object.keys(edited).length) this.updates[row] = edited;
			else delete this.updates[row];
		});
	}

	toggleDelete(row: number) {
		this.#mutate(() => {
			if (this.deleted[row]) delete this.deleted[row];
			else this.deleted[row] = true;
		});
	}

	addRow(columns: number) {
		this.#mutate(() => this.inserted.push(Array<NewCell>(columns).fill(DEFAULT)));
	}

	setNewCell(index: number, col: number, value: NewCell) {
		if (this.inserted[index][col] === value) return;
		this.#mutate(() => (this.inserted[index][col] = value));
	}

	removeNewRow(index: number) {
		this.#mutate(() => this.inserted.splice(index, 1));
	}

	/** Forget all edits and their history (after saving or reloading). */
	reset() {
		this.updates = {};
		this.deleted = {};
		this.inserted = [];
		this.#undo = [];
		this.#redo = [];
	}

	/** Throw away all edits, undoably. */
	discard() {
		if (this.count === 0) return;
		this.#mutate(() => {
			this.updates = {};
			this.deleted = {};
			this.inserted = [];
		});
	}
}

function keyOf(row: Cell[], source: SourceInfo): ColumnValue[] {
	return source.key.map((i) => ({ column: source.columns[i]!, value: row[i] }));
}

/**
 * The edits as backend changes: deletes first (a deleted row's update is
 * pointless), then updates, then inserts. Expression columns are skipped.
 */
export function buildChanges(edits: GridEdits, rows: Cell[][], source: SourceInfo): Change[] {
	const changes: Change[] = [];
	for (const index of Object.keys(edits.deleted).map(Number)) {
		changes.push({ op: 'delete', key: keyOf(rows[index], source) });
	}
	for (const [index, cols] of Object.entries(edits.updates)) {
		const row = Number(index);
		if (edits.deleted[row]) continue;
		const values = Object.entries(cols)
			.filter(([col]) => source.columns[Number(col)] !== null)
			.map(([col, value]) => ({ column: source.columns[Number(col)]!, value }));
		if (values.length) changes.push({ op: 'update', key: keyOf(rows[row], source), values });
	}
	for (const row of edits.inserted) {
		const values = row
			.map((value, col) => ({ column: source.columns[col], value }))
			.filter((v): v is ColumnValue & { value: Cell } => v.column !== null && v.value !== DEFAULT);
		changes.push({ op: 'insert', values });
	}
	return changes;
}
