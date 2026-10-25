import type { ConnectionConfig, Placement } from './api.js';

export interface FolderGroup {
	name: string;
	connections: ConnectionConfig[];
}

export interface SidebarLayout {
	folders: FolderGroup[];
	/** Connections without a folder, shown below the folders. */
	loose: ConnectionConfig[];
}

function matches(c: ConnectionConfig, needle: string): boolean {
	return [c.name, c.host, c.database, c.user, c.ssh?.host ?? ''].some((v) =>
		v.toLowerCase().includes(needle)
	);
}

/**
 * Group connections by folder. Folders come in `folderOrder` first, then any
 * others in order of appearance. Empty folders only show without a filter;
 * a filter that matches a folder's name shows the whole folder.
 */
export function sidebarLayout(
	list: ConnectionConfig[],
	folderOrder: string[],
	filter = ''
): SidebarLayout {
	const needle = filter.trim().toLowerCase();
	const names = [...folderOrder];
	for (const c of list) {
		if (c.folder && !names.includes(c.folder)) names.push(c.folder);
	}
	const folders: FolderGroup[] = [];
	for (const name of names) {
		const inFolder = list.filter((c) => c.folder === name);
		const shown =
			!needle || name.toLowerCase().includes(needle)
				? inFolder
				: inFolder.filter((c) => matches(c, needle));
		if (needle && shown.length === 0) continue;
		folders.push({ name, connections: shown });
	}
	const loose = list.filter((c) => !c.folder && (!needle || matches(c, needle)));
	return { folders, loose };
}

function placements(list: ConnectionConfig[]): Placement[] {
	return list.map((c) => ({ id: c.id, folder: c.folder }));
}

/**
 * Move a connection into `folder` (null: top level), right before `beforeId`
 * or at the end. Returns null when nothing would change.
 */
export function moveConnection(
	list: ConnectionConfig[],
	id: string,
	folder: string | null,
	beforeId: string | null = null
): Placement[] | null {
	if (id === beforeId) return null;
	const moving = list.find((c) => c.id === id);
	if (!moving) return null;
	const rest = list.filter((c) => c.id !== id);
	const index = beforeId ? rest.findIndex((c) => c.id === beforeId) : -1;
	const moved = { ...moving, folder };
	if (index === -1) rest.push(moved);
	else rest.splice(index, 0, moved);
	const next = placements(rest);
	const same = next.every((p, i) => p.id === list[i].id && p.folder === list[i].folder);
	return same ? null : next;
}

/** Rename a folder; `to: null` moves its connections to the top level. */
export function renameFolder(
	list: ConnectionConfig[],
	from: string,
	to: string | null
): Placement[] {
	return list.map((c) => ({ id: c.id, folder: c.folder === from ? to : c.folder }));
}

/** A libpq URL without the password, for pasting into other tools. */
export function connectionUrl(c: ConnectionConfig): string {
	const host = c.host.includes(':') ? `[${c.host}]` : c.host;
	const url = `postgresql://${encodeURIComponent(c.user)}@${host}:${c.port}/${encodeURIComponent(c.database)}`;
	return c.sslMode === 'prefer' ? url : `${url}?sslmode=${c.sslMode}`;
}
