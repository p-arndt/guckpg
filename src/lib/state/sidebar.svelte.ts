/**
 * Sidebar view state. Folder membership lives on each connection in the
 * backend; this keeps what only the sidebar needs: folder order (so empty
 * folders survive), which folders are collapsed, and the filter.
 */
import type { SortMode } from '#lib/sidebar.js';

interface Saved {
	folders: string[];
	collapsed: string[];
	sort: SortMode;
	lastUsed: Record<string, number>;
}

const SORT_MODES: SortMode[] = ['manual', 'name', 'modified', 'used'];

const STORAGE_KEY = 'guckpg.sidebar';

export class Sidebar {
	folders = $state<string[]>([]);
	collapsed = $state<string[]>([]);
	filter = $state('');
	sort = $state<SortMode>('manual');
	/** Last connect per connection id, for sorting by last use. */
	lastUsed = $state<Record<string, number>>({});
	/** Id of the connection being dragged. */
	dragging = $state<string | null>(null);
	/** Connections expanded in the tree; kept here so moving one between folders keeps it open. */
	open = $state<string[]>([]);

	constructor(private storage: Storage | null) {
		try {
			const saved: Partial<Saved> = JSON.parse(storage?.getItem(STORAGE_KEY) ?? '{}');
			this.folders = Array.isArray(saved.folders) ? saved.folders : [];
			this.collapsed = Array.isArray(saved.collapsed) ? saved.collapsed : [];
			if (saved.sort && SORT_MODES.includes(saved.sort)) this.sort = saved.sort;
			if (saved.lastUsed && typeof saved.lastUsed === 'object') this.lastUsed = saved.lastUsed;
		} catch {
			// Corrupt state only costs the folder order.
		}
	}

	private save() {
		const saved: Saved = {
			folders: this.folders,
			collapsed: this.collapsed,
			sort: this.sort,
			lastUsed: this.lastUsed
		};
		this.storage?.setItem(STORAGE_KEY, JSON.stringify(saved));
	}

	setSort(mode: SortMode) {
		this.sort = mode;
		this.save();
	}

	markUsed(id: string, at = Date.now()) {
		this.lastUsed = { ...this.lastUsed, [id]: at };
		this.save();
	}

	isOpen(id: string): boolean {
		return this.open.includes(id);
	}

	setOpen(id: string, open: boolean) {
		if (open === this.isOpen(id)) return;
		this.open = open ? [...this.open, id] : this.open.filter((o) => o !== id);
	}

	isCollapsed(name: string): boolean {
		return this.collapsed.includes(name);
	}

	toggle(name: string) {
		this.collapsed = this.isCollapsed(name)
			? this.collapsed.filter((n) => n !== name)
			: [...this.collapsed, name];
		this.save();
	}

	/** Remember a folder; returns false if the name is empty or taken. */
	addFolder(name: string, existing: string[] = []): boolean {
		const trimmed = name.trim();
		if (!trimmed || this.folders.includes(trimmed) || existing.includes(trimmed)) return false;
		this.folders = [...this.folders, trimmed];
		this.save();
		return true;
	}

	renameFolder(from: string, to: string) {
		const order = this.folders.includes(from) ? this.folders : [...this.folders, from];
		this.folders = order.map((n) => (n === from ? to : n)).filter((n, i, a) => a.indexOf(n) === i);
		this.collapsed = this.collapsed.map((n) => (n === from ? to : n));
		this.save();
	}

	removeFolder(name: string) {
		this.folders = this.folders.filter((n) => n !== name);
		this.collapsed = this.collapsed.filter((n) => n !== name);
		this.save();
	}
}

export const sidebar = new Sidebar(typeof window === 'undefined' ? null : window.localStorage);
