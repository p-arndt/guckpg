import { api, type AppError, type Column, type QueryOutput } from '#lib/api.js';
import { confirmDiscard } from './confirm.svelte.js';

type Sort = { column: string; dir: 'asc' | 'desc' } | null;

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

	constructor(id: string, title: string, connectionId: string, database: string, sql: string) {
		super(id, title, connectionId, database);
		this.sql = sql;
	}
}

export class TableTab extends TabBase {
	readonly kind = 'table';
	readonly schema: string;
	readonly table: string;
	sort = $state<Sort>(null);
	limit = $state(200);
	offset = $state(0);
	columns = $state.raw<Column[]>([]);
	lastSql = $state('');

	constructor(connectionId: string, database: string, schema: string, table: string) {
		super(newId(), schema === 'public' ? table : `${schema}.${table}`, connectionId, database);
		this.schema = schema;
		this.table = table;
	}
}

export class DiagramTab extends TabBase {
	readonly kind = 'diagram';
	readonly schema: string;

	constructor(connectionId: string, database: string, schema: string) {
		super(newId(), `ER: ${schema}`, connectionId, database);
		this.schema = schema;
	}
}

export type Tab = QueryTab | TableTab | DiagramTab;

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
	| { kind: 'diagram'; connectionId: string; database: string; schema: string };

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
			return;
		}
		const tab = new TableTab(connectionId, database, schema, table);
		this.items.push(tab);
		this.activeId = tab.id;
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
		if (tab && !(await confirmDiscard(tab.dirty))) return;
		this.close(id);
	}

	/** Close several tabs, asking once if any of them has unsaved edits. */
	async requestCloseMany(ids: string[]) {
		const closing = this.items.filter((t) => ids.includes(t.id));
		const dirty = closing.reduce((sum, t) => sum + t.dirty, 0);
		if (!(await confirmDiscard(dirty))) return;
		for (const t of closing) this.close(t.id);
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

	/** Tabs of a disconnected or deleted connection are not offered for reopening. */
	closeForConnection(connectionId: string) {
		for (const t of this.items.filter((t) => t.connectionId === connectionId)) {
			this.close(t.id, false);
		}
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
