import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionConfig } from '#lib/api.js';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { Connections, newConnection } = await import('./connections.svelte');

function connection(id: string, folder: string | null = null): ConnectionConfig {
	return { ...newConnection(), id, name: id, folder };
}

describe('Connections.arrange', () => {
	beforeEach(() => {
		invoke.mockReset();
	});

	it('shows the new order before the backend answers', async () => {
		const store = new Connections();
		store.list = [connection('a'), connection('b')];
		let finish!: (list: ConnectionConfig[]) => void;
		invoke.mockImplementation((cmd: string) => {
			if (cmd === 'arrange_connections') return new Promise((resolve) => (finish = resolve));
			throw new Error(`unexpected ${cmd}`);
		});
		const done = store.arrange([
			{ id: 'b', folder: 'prod' },
			{ id: 'a', folder: null }
		]);
		expect(store.list.map((c) => [c.id, c.folder])).toEqual([
			['b', 'prod'],
			['a', null]
		]);
		const saved = [connection('b', 'prod'), connection('a')];
		finish(saved);
		await done;
		expect(store.list).toEqual(saved);
	});

	it('drops placements of unknown connections', async () => {
		const store = new Connections();
		store.list = [connection('a')];
		invoke.mockResolvedValue([connection('a')]);
		const done = store.arrange([
			{ id: 'gone', folder: null },
			{ id: 'a', folder: null }
		]);
		expect(store.list.map((c) => c.id)).toEqual(['a']);
		await done;
	});

	it('reloads the stored order when arranging fails', async () => {
		const store = new Connections();
		const stored = [connection('a'), connection('b')];
		store.list = stored;
		invoke.mockImplementation((cmd: string) => {
			if (cmd === 'arrange_connections') return Promise.reject({ kind: 'message', message: 'no' });
			if (cmd === 'list_connections') return Promise.resolve(stored);
			throw new Error(`unexpected ${cmd}`);
		});
		await expect(
			store.arrange([
				{ id: 'b', folder: null },
				{ id: 'a', folder: null }
			])
		).rejects.toEqual({ kind: 'message', message: 'no' });
		expect(store.list.map((c) => c.id)).toEqual(['a', 'b']);
	});
});

describe('Connections.connect', () => {
	beforeEach(() => {
		invoke.mockReset();
	});

	it('tracks the status through a failed connect', async () => {
		const store = new Connections();
		invoke.mockRejectedValue({ kind: 'message', message: 'refused' });
		await expect(store.connect('a')).rejects.toBeTruthy();
		expect(store.status.a).toBe('error');
	});

	it('bumps the revision on disconnect so cached catalog lists are dropped', async () => {
		const store = new Connections();
		invoke.mockResolvedValue(undefined);
		await store.disconnect('a');
		expect(store.status.a).toBe('idle');
		expect(store.revision.a).toBe(1);
	});

	it('bumps the revision on refresh', () => {
		const store = new Connections();
		store.refresh('a');
		store.refresh('a');
		expect(store.revision.a).toBe(2);
	});
});
