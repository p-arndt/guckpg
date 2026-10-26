import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { BrowseOutput, QueryOutput } from '#lib/api.js';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { Tabs } = await import('./tabs.svelte');
const { confirmation } = await import('./confirm.svelte');
const { connections } = await import('./connections.svelte');

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

function page(sql: string, rows: string[][] = [['1']]): BrowseOutput {
	return {
		sql,
		elapsedMs: 1,
		results: [{ columns: ['id'], rows, rowCount: rows.length, truncated: false, source: null }]
	};
}

const postgresError = {
	kind: 'postgres',
	message: 'syntax error',
	code: '42601',
	detail: null,
	hint: null,
	position: 3
};

beforeEach(() => {
	invoke.mockReset();
	invoke.mockResolvedValue(undefined);
	confirmation.answer(false);
});

describe('TableTab', () => {
	it('loads a page with the current sort, filter and paging', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		invoke.mockResolvedValue(page('SELECT 1'));
		await tab.applyFilter({ column: 'name', value: 'ann' });
		await tab.sortBy('id');
		await tab.page(1);
		const [, args] = invoke.mock.calls.at(-1)!;
		expect(args.request).toEqual({
			schema: 'public',
			table: 'users',
			sort: { column: 'id', dir: 'asc' },
			filter: { column: 'name', value: 'ann' },
			limit: 200,
			offset: 200
		});
		expect(tab.lastSql).toBe('SELECT 1');
		expect(tab.running).toBe(false);
	});

	it('cycles the sort and goes back to the first page', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		invoke.mockResolvedValue(page('x'));
		tab.offset = 400;
		await tab.sortBy('id');
		expect(tab.offset).toBe(0);
		await tab.sortBy('id');
		expect(tab.sort).toEqual({ column: 'id', dir: 'desc' });
		await tab.sortBy('id');
		expect(tab.sort).toBeNull();
	});

	it('drops an incomplete filter and resets the page size offset', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		invoke.mockResolvedValue(page('x'));
		await tab.applyFilter({ column: 'name', value: '' });
		expect(tab.filter).toBeNull();
		tab.offset = 200;
		await tab.setLimit(500);
		expect([tab.limit, tab.offset]).toEqual([500, 0]);
	});

	it('keeps the newest page when an older load answers last', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		const first = deferred<BrowseOutput>();
		const second = deferred<BrowseOutput>();
		invoke.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
		const older = tab.page(1);
		const newer = tab.page(1);
		second.resolve(page('page 3'));
		await newer;
		first.resolve(page('page 2'));
		await older;
		expect(tab.lastSql).toBe('page 3');
		expect(tab.running).toBe(false);
	});

	it('stores load errors as app errors', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		invoke.mockRejectedValue(new Error('gone'));
		await tab.load();
		expect(tab.error).toEqual({ kind: 'message', message: 'gone' });
	});

	it('loads columns and rows when opened, once', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		invoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === 'list_columns'
					? [{ name: 'id', dataType: 'int4', notNull: true, default: null, primaryKey: true }]
					: page('x')
			)
		);
		await tab.open();
		await tab.open();
		expect(tab.columns.map((c) => c.name)).toEqual(['id']);
		expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(['list_columns', 'browse_table']);
	});

	it('sums grid and designer edits into dirty', () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		tab.setGridDirty(2);
		tab.setStructureDirty(3);
		expect(tab.dirty).toBe(5);
		tab.setGridDirty(0);
		expect(tab.dirty).toBe(3);
	});

	it('switches to the structure view, asking before dropping grid edits', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		tab.setGridDirty(2);
		const declined = tab.switchView('structure');
		await vi.waitFor(() => expect(confirmation.pending).not.toBeNull());
		confirmation.answer(false);
		await declined;
		expect(tab.view).toBe('data');

		const accepted = tab.switchView('structure');
		await vi.waitFor(() => expect(confirmation.pending).not.toBeNull());
		confirmation.answer(true);
		await accepted;
		expect(tab.view).toBe('structure');
		expect(tab.dirty).toBe(0);
		expect(tab.gridVersion).toBe(1);
	});

	it('waits for a loading page before switching views', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		const loading = deferred<BrowseOutput>();
		invoke.mockReturnValueOnce(loading.promise);
		void tab.load();
		const switching = tab.switchView('structure');
		await Promise.resolve();
		expect(tab.view).toBe('data');
		loading.resolve(page('x'));
		await switching;
		expect(tab.view).toBe('structure');
	});

	it('does not switch while the designer applies changes', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'public', 'users');
		tab.structureBusy = true;
		await tab.switchView('structure');
		expect(tab.view).toBe('data');
	});

	it('starts over on the renamed table after the designer applied', async () => {
		const tab = new Tabs().openTable('c1', 'db', 'sales', 'orders');
		tab.sort = { column: 'id', dir: 'asc' };
		tab.filter = { column: 'id', value: '1' };
		tab.offset = 200;
		tab.setStructureDirty(4);
		invoke.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === 'list_columns' ? [] : page('renamed'))
		);
		const revision = connections.revision.c1 ?? 0;
		await tab.structureApplied('order_items');
		expect(tab.title).toBe('sales.order_items');
		expect([tab.sort, tab.filter, tab.offset, tab.dirty]).toEqual([null, null, 0, 0]);
		expect(connections.revision.c1).toBe(revision + 1);
		expect(tab.lastSql).toBe('renamed');
	});
});

describe('QueryTab', () => {
	function output(value: string): QueryOutput {
		return {
			elapsedMs: 1,
			results: [{ columns: ['x'], rows: [[value]], rowCount: 1, truncated: false, source: null }]
		};
	}

	it('runs text with the row limit and re-runs it', async () => {
		const tab = new Tabs().openQuery('c1', 'db', 'select 1;\nselect 2;');
		tab.maxRows = 100;
		invoke.mockResolvedValue(output('1'));
		await tab.execute('select 2;', 10);
		expect(invoke).toHaveBeenLastCalledWith('run_query', {
			tabId: tab.id,
			id: 'c1',
			database: 'db',
			sql: 'select 2;',
			maxRows: 100
		});
		expect(tab.ranFrom).toBe(10);
		await tab.rerun();
		expect(invoke).toHaveBeenCalledTimes(2);
		expect(tab.output).toEqual(output('1'));
	});

	it('does not run while unsaved edits are kept', async () => {
		const tab = new Tabs().openQuery('c1', 'db');
		tab.dirty = 1;
		const run = tab.execute('select 1', 0);
		await vi.waitFor(() => expect(confirmation.pending).not.toBeNull());
		confirmation.answer(false);
		await run;
		expect(invoke).not.toHaveBeenCalled();
	});

	it('maps error positions onto the editor text only for editor runs', async () => {
		const tab = new Tabs().openQuery('c1', 'db');
		invoke.mockRejectedValue(postgresError);
		await tab.run('selec 1', 20);
		expect(tab.output).toBeNull();
		expect(tab.errorOffset).toBe(22);
		await tab.run('selec 1', 0, false);
		expect(tab.error?.kind).toBe('postgres');
		expect(tab.errorOffset).toBeNull();
	});

	it('shows an explain plan until the next run', async () => {
		const tab = new Tabs().openQuery('c1', 'db');
		invoke.mockResolvedValueOnce(output('Seq Scan')).mockResolvedValueOnce(output('1'));
		await tab.explain('select 1');
		expect(tab.plan).toBe('Seq Scan');
		await tab.run('select 1', 0);
		expect(tab.plan).toBeNull();
	});

	it('reports a failed cancel as the tab error', async () => {
		const tab = new Tabs().openQuery('c1', 'db');
		invoke.mockRejectedValue('no session');
		await tab.cancel();
		expect(tab.error).toEqual({ kind: 'message', message: 'no session' });
	});
});

describe('Tabs.requestCloseForConnection', () => {
	it('closes clean tabs of the connection without asking or remembering them', async () => {
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db');
		tabs.openQuery('c2', 'db');
		await expect(tabs.requestCloseForConnection('c1')).resolves.toBe(true);
		expect(tabs.items.map((t) => t.connectionId)).toEqual(['c2']);
		expect(tabs.closed).toEqual([]);
	});

	it('asks before discarding unsaved edits and keeps the tabs when declined', async () => {
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db').dirty = 2;
		tabs.openTable('c1', 'db', 'public', 'users');
		const asked = tabs.requestCloseForConnection('c1');
		await vi.waitFor(() => expect(confirmation.pending?.title).toBe('Discard 2 unsaved changes?'));
		confirmation.answer(false);
		await expect(asked).resolves.toBe(false);
		expect(tabs.items).toHaveLength(2);

		const again = tabs.requestCloseForConnection('c1');
		await vi.waitFor(() => expect(confirmation.pending).not.toBeNull());
		confirmation.answer(true);
		await expect(again).resolves.toBe(true);
		expect(tabs.items).toHaveLength(0);
	});

	it('refuses while a table designer applies changes', async () => {
		const tabs = new Tabs();
		tabs.openTable('c1', 'db', 'public', 'users').structureBusy = true;
		await expect(tabs.requestCloseForConnection('c1')).resolves.toBe(false);
		expect(tabs.items).toHaveLength(1);
	});
});
