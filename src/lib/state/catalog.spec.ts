import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { Catalog } = await import('./catalog');

describe('Catalog', () => {
	let revision: Record<string, number>;
	let catalog: InstanceType<typeof Catalog>;

	beforeEach(() => {
		invoke.mockReset();
		invoke.mockImplementation((cmd: string, args: Record<string, unknown>) =>
			Promise.resolve(`${cmd}:${JSON.stringify(args)}`)
		);
		revision = {};
		catalog = new Catalog((id) => revision[id] ?? 0);
	});

	it('shares one request per connection, database and revision', async () => {
		const [a, b] = await Promise.all([catalog.databases('c1'), catalog.databases('c1')]);
		expect(a).toBe(b);
		await catalog.completionTables('c1', 'db');
		await catalog.completionTables('c1', 'db');
		await catalog.completionTables('c1', 'other');
		await catalog.databases('c2');
		expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual([
			'list_databases',
			'completion_tables',
			'completion_tables',
			'list_databases'
		]);
	});

	it('fetches again after the revision changes', async () => {
		await catalog.relations('c1', 'db', 'public');
		revision.c1 = 1;
		await catalog.relations('c1', 'db', 'public');
		expect(invoke).toHaveBeenCalledTimes(2);
		await catalog.relations('c1', 'db', 'public');
		expect(invoke).toHaveBeenCalledTimes(2);
	});

	it('fetches again when asked for fresh data', async () => {
		await catalog.schemas('c1', 'db');
		await catalog.schemas('c1', 'db', false, true);
		expect(invoke).toHaveBeenCalledTimes(2);
		await catalog.schemas('c1', 'db', true);
		expect(invoke).toHaveBeenCalledTimes(3);
	});

	it('does not cache failures', async () => {
		invoke.mockRejectedValueOnce({ kind: 'message', message: 'down' });
		await expect(catalog.databases('c1')).rejects.toBeTruthy();
		await expect(catalog.databases('c1')).resolves.toContain('list_databases');
		expect(invoke).toHaveBeenCalledTimes(2);
	});
});
