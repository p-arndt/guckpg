/**
 * Sidebar view state. Folder membership lives on each connection in the
 * backend; this keeps what only the sidebar needs: folder order (so empty
 * folders survive), which folders are collapsed, and the filter.
 */
interface Saved {
	folders: string[];
	collapsed: string[];
}

const STORAGE_KEY = 'guckpg.sidebar';

export class Sidebar {
	folders = $state<string[]>([]);
	collapsed = $state<string[]>([]);
	filter = $state('');
	/** Id of the connection being dragged. */
	dragging = $state<string | null>(null);
	/** Connections expanded in the tree; kept here so moving one between folders keeps it open. */
	open = $state<string[]>([]);

	constructor(private storage: Storage | null) {
		try {
			const saved: Partial<Saved> = JSON.parse(storage?.getItem(STORAGE_KEY) ?? '{}');
			this.folders = Array.isArray(saved.folders) ? saved.folders : [];
			this.collapsed = Array.isArray(saved.collapsed) ? saved.collapsed : [];
		} catch {
			// Corrupt state only costs the folder order.
		}
	}

	private save() {
		const saved: Saved = { folders: this.folders, collapsed: this.collapsed };
		this.storage?.setItem(STORAGE_KEY, JSON.stringify(saved));
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
