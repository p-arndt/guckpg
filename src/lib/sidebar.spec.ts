import { describe, expect, it } from 'vitest';
import type { ConnectionConfig } from './api.js';
import {
	connectionUrl,
	moveConnection,
	renameFolder,
	sidebarLayout,
	sortConnections,
	treeKeyMove
} from './sidebar.js';

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

describe('sortConnections', () => {
	const list = [
		conn('b2', null, { name: 'db10', updatedAt: 100 }),
		conn('a', null, { name: 'Alpha' }),
		conn('b1', null, { name: 'db9', updatedAt: 300 })
	];

	it('keeps the custom order', () => {
		expect(names(sortConnections(list, { mode: 'manual' }))).toEqual(['b2', 'a', 'b1']);
	});

	it('sorts by name, case-insensitive and numeric', () => {
		expect(names(sortConnections(list, { mode: 'name' }))).toEqual(['a', 'b1', 'b2']);
	});

	it('puts the newest change first, connections without a timestamp last', () => {
		expect(names(sortConnections(list, { mode: 'modified' }))).toEqual(['b1', 'b2', 'a']);
	});

	it('puts the last used first and keeps the custom order for unused ones', () => {
		const sorted = sortConnections(list, { mode: 'used', lastUsed: { a: 5 } });
		expect(names(sorted)).toEqual(['a', 'b2', 'b1']);
	});

	it('sorts folders by name only when sorting by name', () => {
		const grouped = [conn('x', 'zeta'), conn('y', 'alpha', { updatedAt: 1 })];
		const byName = sidebarLayout(grouped, ['zeta'], '', { mode: 'name' });
		expect(byName.folders.map((f) => f.name)).toEqual(['alpha', 'zeta']);
		const byModified = sidebarLayout(grouped, ['zeta'], '', { mode: 'modified' });
		expect(byModified.folders.map((f) => f.name)).toEqual(['zeta', 'alpha']);
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

	it('adds the root certificate only when the server is verified', () => {
		const cert = '/certs/my ca.pem';
		expect(connectionUrl(conn('a', null, { sslMode: 'verify-full', sslRootCert: cert }))).toBe(
			'postgresql://postgres@localhost:5432/postgres?sslmode=verify-full&sslrootcert=%2Fcerts%2Fmy%20ca.pem'
		);
		expect(connectionUrl(conn('a', null, { sslMode: 'require', sslRootCert: cert }))).toBe(
			'postgresql://postgres@localhost:5432/postgres?sslmode=require'
		);
	});
});

describe('treeKeyMove', () => {
	// folder > connection (expanded) > database (collapsed), then a loose connection.
	const items = [
		{ level: 1, expanded: true },
		{ level: 2, expanded: true },
		{ level: 3, expanded: false },
		{ level: 1 }
	];

	it('walks visible rows up and down and jumps to the ends', () => {
		expect(treeKeyMove(items, 0, 'ArrowDown')).toEqual({ focus: 1 });
		expect(treeKeyMove(items, 3, 'ArrowDown')).toBeNull();
		expect(treeKeyMove(items, 0, 'ArrowUp')).toBeNull();
		expect(treeKeyMove(items, 2, 'ArrowUp')).toEqual({ focus: 1 });
		expect(treeKeyMove(items, 2, 'Home')).toEqual({ focus: 0 });
		expect(treeKeyMove(items, 0, 'End')).toEqual({ focus: 3 });
	});

	it('expands collapsed rows and enters expanded ones', () => {
		expect(treeKeyMove(items, 2, 'ArrowRight')).toEqual({ toggle: 2 });
		expect(treeKeyMove(items, 1, 'ArrowRight')).toEqual({ focus: 2 });
		expect(treeKeyMove(items, 3, 'ArrowRight')).toBeNull();
	});

	it('collapses expanded rows and otherwise goes to the parent', () => {
		expect(treeKeyMove(items, 1, 'ArrowLeft')).toEqual({ toggle: 1 });
		expect(treeKeyMove(items, 2, 'ArrowLeft')).toEqual({ focus: 1 });
		expect(treeKeyMove(items, 3, 'ArrowLeft')).toBeNull();
	});

	it('ignores other keys', () => {
		expect(treeKeyMove(items, 0, 'Enter')).toBeNull();
	});
});
