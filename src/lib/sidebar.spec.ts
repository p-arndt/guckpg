import { describe, expect, it } from 'vitest';
import type { ConnectionConfig } from './api.js';
import { connectionUrl, moveConnection, renameFolder, sidebarLayout } from './sidebar.js';

function conn(id: string, folder: string | null = null, extra: Partial<ConnectionConfig> = {}) {
	return {
		id,
		name: id,
		host: 'localhost',
		port: 5432,
		user: 'postgres',
		database: 'postgres',
		sslMode: 'prefer',
		readOnly: false,
		color: null,
		ssh: null,
		folder,
		...extra
	} satisfies ConnectionConfig;
}

const names = (list: { id: string }[]) => list.map((c) => c.id);

describe('sidebarLayout', () => {
	it('groups by folder in the saved order, then by appearance', () => {
		const list = [conn('a', 'dev'), conn('b'), conn('c', 'prod'), conn('d', 'dev')];
		const { folders, loose } = sidebarLayout(list, ['prod', 'empty']);
		expect(folders.map((f) => f.name)).toEqual(['prod', 'empty', 'dev']);
		expect(names(folders[2].connections)).toEqual(['a', 'd']);
		expect(folders[1].connections).toEqual([]);
		expect(names(loose)).toEqual(['b']);
	});

	it('filters by name, host and database and hides empty folders', () => {
		const list = [
			conn('shop', 'prod'),
			conn('blog', 'prod', { host: 'db.example.com' }),
			conn('local')
		];
		const { folders, loose } = sidebarLayout(list, ['empty'], 'EXAMPLE');
		expect(folders.map((f) => f.name)).toEqual(['prod']);
		expect(names(folders[0].connections)).toEqual(['blog']);
		expect(loose).toEqual([]);
	});

	it('shows a whole folder when the filter matches its name', () => {
		const list = [conn('a', 'production'), conn('b', 'production')];
		const { folders } = sidebarLayout(list, [], 'prod');
		expect(names(folders[0].connections)).toEqual(['a', 'b']);
	});
});

describe('moveConnection', () => {
	const list = [conn('a'), conn('b', 'prod'), conn('c', 'prod')];

	it('moves into a folder at the end', () => {
		expect(moveConnection(list, 'a', 'prod')).toEqual([
			{ id: 'b', folder: 'prod' },
			{ id: 'c', folder: 'prod' },
			{ id: 'a', folder: 'prod' }
		]);
	});

	it('moves before another connection', () => {
		expect(moveConnection(list, 'c', 'prod', 'b')).toEqual([
			{ id: 'a', folder: null },
			{ id: 'c', folder: 'prod' },
			{ id: 'b', folder: 'prod' }
		]);
	});

	it('returns null when nothing changes', () => {
		expect(moveConnection(list, 'c', 'prod')).toBeNull();
		expect(moveConnection(list, 'b', 'prod', 'b')).toBeNull();
		expect(moveConnection(list, 'missing', null)).toBeNull();
	});
});

describe('renameFolder', () => {
	it('renames or dissolves a folder', () => {
		const list = [conn('a', 'old'), conn('b', 'other')];
		expect(renameFolder(list, 'old', 'new')).toEqual([
			{ id: 'a', folder: 'new' },
			{ id: 'b', folder: 'other' }
		]);
		expect(renameFolder(list, 'old', null)[0]).toEqual({ id: 'a', folder: null });
	});
});

describe('connectionUrl', () => {
	it('builds a URL without the password', () => {
		expect(connectionUrl(conn('a'))).toBe('postgresql://postgres@localhost:5432/postgres');
		expect(
			connectionUrl(
				conn('a', null, { user: 'a@b', host: '::1', sslMode: 'require', database: 'x' })
			)
		).toBe('postgresql://a%40b@[::1]:5432/x?sslmode=require');
	});
});
