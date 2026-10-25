import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

const { Tabs } = await import('./tabs.svelte');
const { memoryStorage } = await import('./test-storage');

describe('Tabs', () => {
	it('opens query tabs with increasing titles and activates them', () => {
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db');
		const second = tabs.openQuery('c1', 'db', 'select 1');
		expect(tabs.items.map((t) => t.title)).toEqual(['Query 1', 'Query 2']);
		expect(tabs.activeId).toBe(second.id);
	});

	it('reuses the tab of an already open table', () => {
		const tabs = new Tabs();
		tabs.openTable('c1', 'db', 'public', 'users');
		tabs.openQuery('c1', 'db');
		tabs.openTable('c1', 'db', 'public', 'users');
		expect(tabs.items).toHaveLength(2);
		expect(tabs.active?.title).toBe('users');
	});

	it('opens one diagram tab per schema', () => {
		const tabs = new Tabs();
		tabs.openDiagram('c1', 'db', 'public');
		tabs.openQuery('c1', 'db');
		tabs.openDiagram('c1', 'db', 'public');
		tabs.openDiagram('c1', 'db', 'audit');
		expect(tabs.items.map((t) => t.title)).toEqual(['ER: public', 'Query 1', 'ER: audit']);
		expect(tabs.active?.title).toBe('ER: audit');
	});

	it('qualifies tables outside public', () => {
		const tabs = new Tabs();
		tabs.openTable('c1', 'db', 'audit', 'log');
		expect(tabs.active?.title).toBe('audit.log');
	});

	it('activates a neighbour when closing the active tab', () => {
		const tabs = new Tabs();
		const a = tabs.openQuery('c1', 'db');
		const b = tabs.openQuery('c1', 'db');
		const c = tabs.openQuery('c1', 'db');
		tabs.activeId = b.id;
		tabs.close(b.id);
		expect(tabs.activeId).toBe(c.id);
		tabs.close(c.id);
		expect(tabs.activeId).toBe(a.id);
		tabs.close(a.id);
		expect(tabs.activeId).toBeNull();
	});

	it('asks before closing a tab with unsaved edits', async () => {
		const { confirmation } = await import('./confirm.svelte.js');
		const tabs = new Tabs();
		const tab = tabs.openQuery('c1', 'db');
		tabs.items[0].dirty = 2;

		const kept = tabs.requestClose(tab.id);
		confirmation.answer(false);
		await kept;
		expect(tabs.items).toHaveLength(1);

		const closed = tabs.requestClose(tab.id);
		confirmation.answer(true);
		await closed;
		expect(tabs.items).toHaveLength(0);
	});

	it('closes clean tabs without asking', async () => {
		const tabs = new Tabs();
		const tab = tabs.openQuery('c1', 'db');
		await tabs.requestClose(tab.id);
		expect(tabs.items).toHaveLength(0);
	});

	it('closes all tabs of a connection', () => {
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db');
		tabs.openTable('c2', 'db', 'public', 't');
		tabs.openQuery('c1', 'db');
		tabs.closeForConnection('c1');
		expect(tabs.items.map((t) => t.connectionId)).toEqual(['c2']);
	});

	it('saves query tabs and restores those of known connections', () => {
		const storage = memoryStorage();
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db', 'select 1');
		tabs.openQuery('gone', 'db', 'select 2');
		tabs.openTable('c1', 'db', 'public', 't');
		tabs.save(storage);

		const restored = new Tabs();
		restored.restore(new Set(['c1']), storage);
		expect(restored.items).toHaveLength(1);
		expect(restored.items[0]).toMatchObject({ kind: 'query', sql: 'select 1', running: false });
		expect(restored.activeId).toBe(restored.items[0].id);
	});

	it('persists only query tab fields', () => {
		const tabs = new Tabs();
		const tab = tabs.openQuery('c1', 'db', 'select 1');
		tab.output = { results: [], elapsedMs: 1 };
		tabs.openTable('c1', 'db', 'public', 't');
		expect(tabs.persisted()).toEqual([
			{ id: tab.id, title: 'Query 1', connectionId: 'c1', database: 'db', sql: 'select 1' }
		]);
	});

	it('keeps results unproxied', () => {
		const tabs = new Tabs();
		const tab = tabs.openQuery('c1', 'db');
		const output = { results: [], elapsedMs: 1 };
		tab.output = output;
		expect(tabs.items[0].output).toBe(output);
	});

	it('ignores corrupt saved state', () => {
		const storage = memoryStorage();
		storage.setItem('guckpg.tabs', '{nope');
		const tabs = new Tabs();
		tabs.restore(new Set(['c1']), storage);
		expect(tabs.items).toEqual([]);
	});

	it('duplicates query tabs next to the original', () => {
		const tabs = new Tabs();
		const a = tabs.openQuery('c1', 'db', 'select 1');
		tabs.openQuery('c1', 'db');
		const copy = tabs.duplicate(a.id)!;
		expect(tabs.items.map((t) => t.title)).toEqual(['Query 1', 'Query 1 copy', 'Query 2']);
		expect(copy.sql).toBe('select 1');
		expect(tabs.activeId).toBe(copy.id);
		tabs.openTable('c1', 'db', 'public', 't');
		expect(tabs.duplicate(tabs.activeId!)).toBeUndefined();
	});

	it('renames, moves and cycles tabs', () => {
		const tabs = new Tabs();
		const a = tabs.openQuery('c1', 'db');
		const b = tabs.openQuery('c1', 'db');
		const c = tabs.openQuery('c1', 'db');
		tabs.rename(a.id, '  users ');
		tabs.rename(b.id, ' ');
		expect(a.title).toBe('users');
		expect(b.title).toBe('Query 2');

		tabs.move(c.id, 0);
		expect(tabs.items.map((t) => t.id)).toEqual([c.id, a.id, b.id]);

		tabs.cycle(1);
		expect(tabs.activeId).toBe(a.id);
		tabs.cycle(-2);
		expect(tabs.activeId).toBe(b.id);
		tabs.activateIndex(0);
		expect(tabs.activeId).toBe(c.id);
		tabs.activateIndex(-1);
		expect(tabs.activeId).toBe(b.id);
	});

	it('closes others and tabs to the right', async () => {
		const tabs = new Tabs();
		const a = tabs.openQuery('c1', 'db');
		const b = tabs.openQuery('c1', 'db');
		tabs.openQuery('c1', 'db');
		await tabs.closeToRight(b.id);
		expect(tabs.items.map((t) => t.id)).toEqual([a.id, b.id]);
		await tabs.closeOthers(a.id);
		expect(tabs.items.map((t) => t.id)).toEqual([a.id]);
		await tabs.closeAll();
		expect(tabs.items).toEqual([]);
	});

	it('asks once before closing several dirty tabs', async () => {
		const { confirmation } = await import('./confirm.svelte.js');
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db').dirty = 1;
		tabs.openQuery('c1', 'db').dirty = 2;
		const closing = tabs.closeAll();
		expect(confirmation.pending?.title).toBe('Discard 3 unsaved changes?');
		confirmation.answer(false);
		await closing;
		expect(tabs.items).toHaveLength(2);
	});

	it('reopens closed tabs, newest first, skipping deleted connections', () => {
		const tabs = new Tabs();
		const q = tabs.openQuery('c1', 'db', 'select 1');
		tabs.rename(q.id, 'mine');
		tabs.openTable('c1', 'db', 'audit', 'log');
		tabs.openQuery('gone', 'db');
		for (const t of [...tabs.items]) tabs.close(t.id);

		expect(tabs.reopenClosed((id) => id !== 'gone')).toBe(true);
		expect(tabs.active?.title).toBe('audit.log');
		expect(tabs.reopenClosed()).toBe(true);
		expect(tabs.active).toMatchObject({ kind: 'query', title: 'mine', sql: 'select 1' });
		expect(tabs.reopenClosed()).toBe(false);
	});

	it('does not remember tabs closed with their connection', () => {
		const tabs = new Tabs();
		tabs.openQuery('c1', 'db');
		tabs.closeForConnection('c1');
		expect(tabs.closed).toEqual([]);
	});
});
