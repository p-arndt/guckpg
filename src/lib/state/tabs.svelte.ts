import { api, toAppError, type AppError, type Column, type QueryOutput } from '#lib/api.js';
import { cycleSort, type Sort } from '#lib/grid.js';
import { confirmDiscard } from './confirm.svelte.js';
import { connections } from './connections.svelte.js';
import type { BuilderModel } from '#lib/query-builder.js';

export type TableFilter = { column: string; value: string };

/**
 * Tabs are classes rather than plain objects so `output` can be raw state:
 * a deep proxy would create a signal per row and cell of results that can
 * hold 100k rows. Results are only ever replaced, never mutated in place.
 */
abstract class TabBase {
	readonly id: string;
	title = $state('');
	connectionId = $state('');
	database = $state('');
	running = $state(false);
	output = $state.raw<QueryOutput | null>(null);
	error = $state<AppError | null>(null);
	/** Unsaved grid edits in this tab. */
	dirty = $state(0);

	constructor(id: string, title: string, connectionId: string, database: string) {
		this.id = id;
		this.title = title;
		this.connectionId = connectionId;
		this.database = database;
	}
}

export class QueryTab extends TabBase {
	readonly kind = 'query';
	sql = $state('');
	/** Offset in `sql` where the last executed text started, to map error positions. */
	ranFrom = $state(0);
	builderOpen = $state(false);
	/** Kept while the tab lives so closing the builder does not lose the canvas. */
	builder = $state<{ database: string; model: BuilderModel } | null>(null);
	maxRows = $state(1000);
	/** The EXPLAIN output shown instead of results; null shows results. */
	plan = $state<string | null>(null);
	/** Last executed text, re-run after saving grid edits. */
	#lastRun: { text: string; from: number; inEditor: boolean } | null = null;

	constructor(id: string, title: string, connectionId: string, database: string, sql: string) {
		super(id, title, connectionId, database);
		this.sql = sql;
	}

	/**
	 * Where in `sql` the last error points, or null. Only for text run from the
	 * editor: positions in builder SQL do not map onto the editor text.
	 */
	get errorOffset(): number | null {
		const error = this.error;
		if (!this.#lastRun?.inEditor || error?.kind !== 'postgres' || error.position === null)
			return null;
		return this.ranFrom + error.position - 1;
	}

	/** Run unless the user keeps unsaved grid edits; `from` is the offset of `text` in `sql`. */
	async execute(text: string, from: number, inEditor = true): Promise<void> {
		if (this.running || !text.trim()) return;
		if (!(await confirmDiscard(this.dirty))) return;
		await this.run(text, from, inEditor);
	}

	async run(text: string, from: number, inEditor = true): Promise<void> {
		if (this.running) return;
		this.plan = null;
		this.#lastRun = { text, from, inEditor };
		this.running = true;
		this.error = null;
		this.ranFrom = from;
		try {
			this.output = await api.runQuery(
				this.id,
				this.connectionId,
				this.database,
				text,
				this.maxRows
			);
		} catch (e) {
			this.output = null;
			this.error = toAppError(e);
		} finally {
			this.running = false;
		}
	}

	rerun(): Promise<void> {
		const last = this.#lastRun;
		return last ? this.run(last.text, last.from, last.inEditor) : Promise.resolve();
	}

	async explain(text: string): Promise<void> {
		if (this.running || !text.trim()) return;
		if (!(await confirmDiscard(this.dirty))) return;
		this.running = true;
		this.error = null;
		this.plan = null;
		try {
			const output = await api.explainQuery(this.id, this.connectionId, this.database, text);
			this.plan = output.results[0]?.rows[0]?.[0] ?? '';
		} catch (e) {
			this.output = null;
			this.error = toAppError(e);
		} finally {
			this.running = false;
		}
	}

	async cancel(): Promise<void> {
		try {
			await api.cancelQuery(this.id);
		} catch (e) {
			this.error = toAppError(e);
		}
	}
}

export class TableTab extends TabBase {
	readonly kind = 'table';
	readonly schema: string;
	table = $state('');
	view = $state<'data' | 'structure'>('data');
	structureBusy = $state(false);
	sort = $state<Sort | null>(null);
	filter = $state<TableFilter | null>(null);
	limit = $state(200);
	offset = $state(0);
	columns = $state.raw<Column[]>([]);
	lastSql = $state('');
	/** Unsaved edits in the data grid and in the table designer; `dirty` is their sum. */
	gridDirty = $state(0);
	structureDirty = $state(0);
	/** Bumped to remount the grid, which throws its unsaved edits away. */
	gridVersion = $state(0);
	#request = 0;
	#loading: Promise<void> = Promise.resolve();

	constructor(connectionId: string, database: string, schema: string, table: string) {
		super(newId(), schema === 'public' ? table : `${schema}.${table}`, connectionId, database);
		this.schema = schema;
		this.table = table;
	}

	setGridDirty(count: number) {
		this.gridDirty = count;
		this.dirty = this.gridDirty + this.structureDirty;
	}

	setStructureDirty(count: number) {
		this.structureDirty = count;
		this.dirty = this.gridDirty + this.structureDirty;
	}

	/** Load the current page. Only the latest request lands, so quick paging cannot show an older page. */
	load(): Promise<void> {
		const request = ++this.#request;
		this.running = true;
		this.error = null;
		const loading = api
			.browseTable(this.id, this.connectionId, this.database, {
				schema: this.schema,
				table: this.table,
				sort: this.sort,
				filter: this.filter,
				limit: this.limit,
				offset: this.offset
			})
			.then(
				(out) => {
					if (request !== this.#request) return;
					this.lastSql = out.sql;
					this.output = { results: out.results, elapsedMs: out.elapsedMs };
				},
				(e) => {
					if (request === this.#request) this.error = toAppError(e);
				}
			)
			.finally(() => {
				if (request === this.#request) this.running = false;
			});
		this.#loading = loading;
		return loading;
	}

	/** Columns and the first page, unless already loaded. */
	open(): Promise<void> {
		if (this.output) return Promise.resolve();
		api
			.listColumns(this.connectionId, this.database, this.schema, this.table)
			.then((columns) => (this.columns = columns))
			.catch(() => {});
		return this.load();
	}

	page(delta: number): Promise<void> {
		this.offset = Math.max(0, this.offset + delta * this.limit);
		return this.load();
	}

	sortBy(column: string): Promise<void> {
		this.sort = cycleSort(this.sort, column);
		this.offset = 0;
		return this.load();
	}

	setLimit(limit: number): Promise<void> {
		this.limit = limit;
		this.offset = 0;
		return this.load();
	}

	applyFilter(filter: TableFilter | null): Promise<void> {
		this.filter = filter && filter.column && filter.value ? filter : null;
		this.offset = 0;
		return this.load();
	}

	/**
	 * Switch between data and structure. Waits for a page that is loading;
	 * leaving unsaved grid edits for the designer asks first.
	 */
	async switchView(view: 'data' | 'structure'): Promise<void> {
		if (this.running) await this.#loading;
		if (this.view === view || this.structureBusy || this.running) return;
		if (view === 'structure' && this.gridDirty) {
			if (!(await confirmDiscard(this.gridDirty))) return;
			this.setGridDirty(0);
			this.gridVersion++;
		}
		this.view = view;
	}

	/** The designer applied changes, maybe a rename: start over on the table. */
	async structureApplied(name: string): Promise<void> {
		this.table = name;
		this.title = this.schema === 'public' ? name : `${this.schema}.${name}`;
		this.sort = null;
		this.filter = null;
		this.offset = 0;
		this.output = null;
		this.setStructureDirty(0);
		connections.refresh(this.connectionId);
		try {
			this.columns = await api.listColumns(this.connectionId, this.database, this.schema, name);
			await this.load();
		} catch (e) {
			this.error = toAppError(e);
		}
	}
}

export class DiagramTab extends TabBase {
	readonly kind = 'diagram';
	schema = $state('');

	constructor(connectionId: string, database: string, schema: string) {
		super(newId(), `ER: ${schema}`, connectionId, database);
		this.schema = schema;
	}
}

export class RoleTab extends TabBase {
	readonly kind = 'role';
	role = $state('');

	/** `database` is where schema and table privileges are shown. */
	constructor(connectionId: string, database: string, role: string) {
		super(newId(), role, connectionId, database);
		this.role = role;
	}
}

export type Tab = QueryTab | TableTab | DiagramTab | RoleTab;

/** What survives a restart: open query tabs with their text. */
interface SavedTab {
	id: string;
	title: string;
	connectionId: string;
	database: string;
	sql: string;
}

/** Enough to open a closed tab again. */
export type ClosedTab =
	| { kind: 'query'; title: string; connectionId: string; database: string; sql: string }
	| { kind: 'table'; connectionId: string; database: string; schema: string; table: string }
	| { kind: 'diagram'; connectionId: string; database: string; schema: string }
	| { kind: 'role'; connectionId: string; database: string; role: string };

const STORAGE_KEY = 'guckpg.tabs';
const MAX_CLOSED = 20;

function describe(tab: Tab): ClosedTab {
	const { connectionId, database } = tab;
	switch (tab.kind) {
		case 'query':
			return { kind: 'query', title: tab.title, connectionId, database, sql: tab.sql };
		case 'table':
			return { kind: 'table', connectionId, database, schema: tab.schema, table: tab.table };
		case 'diagram':
			return { kind: 'diagram', connectionId, database, schema: tab.schema };
		case 'role':
			return { kind: 'role', connectionId, database, role: tab.role };
	}
}

function newId(): string {
	return crypto.randomUUID();
}

class Tabs {
	items = $state<Tab[]>([]);
	activeId = $state<string | null>(null);
	/** Recently closed tabs, newest last. */
	closed = $state<ClosedTab[]>([]);

	get active(): Tab | undefined {
		return this.items.find((t) => t.id === this.activeId);
	}

	openQuery(connectionId: string, database: string, sql = ''): QueryTab {
		const n = this.items.filter((t) => t.kind === 'query').length + 1;
		const tab = new QueryTab(newId(), `Query ${n}`, connectionId, database, sql);
		this.items.push(tab);
		this.activeId = tab.id;
		return tab;
	}

	openTable(connectionId: string, database: string, schema: string, table: string) {
		const existing = this.items.find(
			(t) =>
				t.kind === 'table' &&
				t.connectionId === connectionId &&
				t.database === database &&
				t.schema === schema &&
				t.table === table
		);
		if (existing) {
			this.activeId = existing.id;
			return existing as TableTab;
		}
		const tab = new TableTab(connectionId, database, schema, table);
		this.items.push(tab);
		this.activeId = tab.id;
		return tab;
	}

	openDiagram(connectionId: string, database: string, schema: string) {
		const existing = this.items.find(
			(t) =>
				t.kind === 'diagram' &&
				t.connectionId === connectionId &&
				t.database === database &&
				t.schema === schema
		);
		if (existing) {
			this.activeId = existing.id;
			return;
		}
		const tab = new DiagramTab(connectionId, database, schema);
		this.items.push(tab);
		this.activeId = tab.id;
	}

	openRole(connectionId: string, database: string, role: string): RoleTab {
		const existing = this.items.find(
			(t): t is RoleTab => t.kind === 'role' && t.connectionId === connectionId && t.role === role
		);
		if (existing) {
			this.activeId = existing.id;
			return existing;
		}
		const tab = new RoleTab(connectionId, database, role);
		this.items.push(tab);
		this.activeId = tab.id;
		return tab;
	}

	/** Close the tabs of a role that no longer exists. */
	closeRole(connectionId: string, role: string) {
		for (const t of this.items.filter(
			(t) => t.kind === 'role' && t.connectionId === connectionId && t.role === role
		)) {
			this.close(t.id, false);
		}
	}

	/**
	 * Point a diagram tab at another schema. When a tab for that schema is already open it
	 * is activated instead, so there is never more than one per schema. Returns the tab
	 * that now shows the schema.
	 */
	switchDiagramSchema(id: string, schema: string): DiagramTab | undefined {
		const tab = this.items.find((t): t is DiagramTab => t.id === id && t.kind === 'diagram');
		if (!tab || tab.schema === schema) return tab;
		const existing = this.items.find(
			(t): t is DiagramTab =>
				t.kind === 'diagram' &&
				t.connectionId === tab.connectionId &&
				t.database === tab.database &&
				t.schema === schema
		);
		if (existing) {
			this.activeId = existing.id;
			return existing;
		}
		tab.schema = schema;
		tab.title = `ER: ${schema}`;
		return tab;
	}

	/** Open a copy of a query tab right after it. */
	duplicate(id: string): QueryTab | undefined {
		const index = this.items.findIndex((t) => t.id === id);
		const source = this.items[index];
		if (source?.kind !== 'query') return;
		const tab = new QueryTab(
			newId(),
			`${source.title} copy`,
			source.connectionId,
			source.database,
			source.sql
		);
		this.items.splice(index + 1, 0, tab);
		this.activeId = tab.id;
		return tab;
	}

	rename(id: string, title: string) {
		const tab = this.items.find((t) => t.id === id);
		if (tab && title.trim()) tab.title = title.trim();
	}

	/** Move a tab to `index` in the tab bar. */
	move(id: string, index: number) {
		const from = this.items.findIndex((t) => t.id === id);
		if (from === -1) return;
		const [tab] = this.items.splice(from, 1);
		this.items.splice(Math.max(0, Math.min(index, this.items.length)), 0, tab);
	}

	/** Activate the tab at `index`; negative counts from the end. */
	activateIndex(index: number) {
		const tab = this.items.at(index);
		if (tab) this.activeId = tab.id;
	}

	/** Activate the next (1) or previous (-1) tab, wrapping around. */
	cycle(delta: number) {
		if (!this.items.length) return;
		const current = this.items.findIndex((t) => t.id === this.activeId);
		const n = this.items.length;
		this.activeId = this.items[(((current + delta) % n) + n) % n].id;
	}

	/** Open the most recently closed tab whose connection still exists. */
	reopenClosed(exists: (connectionId: string) => boolean = () => true): boolean {
		while (this.closed.length) {
			const c = this.closed.pop()!;
			if (!exists(c.connectionId)) continue;
			if (c.kind === 'query') {
				const tab = this.openQuery(c.connectionId, c.database, c.sql);
				tab.title = c.title;
			} else if (c.kind === 'table') {
				this.openTable(c.connectionId, c.database, c.schema, c.table);
			} else if (c.kind === 'role') {
				this.openRole(c.connectionId, c.database, c.role);
			} else {
				this.openDiagram(c.connectionId, c.database, c.schema);
			}
			return true;
		}
		return false;
	}

	close(id: string, remember = true) {
		const index = this.items.findIndex((t) => t.id === id);
		if (index === -1) return;
		if (remember) this.closed = [...this.closed, describe(this.items[index])].slice(-MAX_CLOSED);
		this.items.splice(index, 1);
		if (this.activeId === id) {
			this.activeId = (this.items[index] ?? this.items[index - 1])?.id ?? null;
		}
		void api.closeTab(id).catch(() => {});
	}

	/** Close unless the user keeps unsaved edits. */
	async requestClose(id: string) {
		const tab = this.items.find((t) => t.id === id);
		if (tab?.kind === 'table' && tab.structureBusy) return;
		if (tab && !(await confirmDiscard(tab.dirty))) return;
		this.close(id);
	}

	/**
	 * Close several tabs, asking once if any of them has unsaved edits. Resolves
	 * false when they stay open.
	 */
	async requestCloseMany(ids: string[], remember = true): Promise<boolean> {
		const closing = this.items.filter((t) => ids.includes(t.id));
		if (closing.some((t) => t.kind === 'table' && t.structureBusy)) return false;
		const dirty = closing.reduce((sum, t) => sum + t.dirty, 0);
		if (!(await confirmDiscard(dirty))) return false;
		for (const t of closing) this.close(t.id, remember);
		return true;
	}

	closeOthers(id: string) {
		return this.requestCloseMany(this.items.filter((t) => t.id !== id).map((t) => t.id));
	}

	closeToRight(id: string) {
		const index = this.items.findIndex((t) => t.id === id);
		return this.requestCloseMany(this.items.slice(index + 1).map((t) => t.id));
	}

	closeAll() {
		return this.requestCloseMany(this.items.map((t) => t.id));
	}

	/**
	 * Close the tabs of a connection before disconnecting or deleting it, asking
	 * first when that would throw away unsaved edits. Resolves false when the
	 * user keeps them, so the caller must not go on. The closed tabs are not
	 * offered for reopening.
	 */
	requestCloseForConnection(connectionId: string): Promise<boolean> {
		const ids = this.items.filter((t) => t.connectionId === connectionId).map((t) => t.id);
		return this.requestCloseMany(ids, false);
	}

	/** The part of the tabs that `save` persists; reading it tracks nothing else. */
	persisted(): SavedTab[] {
		return this.items
			.filter((t): t is QueryTab => t.kind === 'query')
			.map(({ id, title, connectionId, database, sql }) => ({
				id,
				title,
				connectionId,
				database,
				sql
			}));
	}

	save(storage: Storage = localStorage) {
		storage.setItem(STORAGE_KEY, JSON.stringify(this.persisted()));
	}

	restore(knownConnections: Set<string>, storage: Storage = localStorage) {
		let saved: SavedTab[];
		try {
			saved = JSON.parse(storage.getItem(STORAGE_KEY) ?? '[]');
		} catch {
			return;
		}
		for (const s of saved) {
			if (!knownConnections.has(s.connectionId)) continue;
			this.items.push(new QueryTab(s.id, s.title, s.connectionId, s.database, s.sql));
		}
		this.activeId = this.items[0]?.id ?? null;
	}
}

export { Tabs };
export const tabs = new Tabs();
