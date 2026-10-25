import { describe, expect, it } from 'vitest';
import { Sidebar } from './sidebar.svelte';
import { memoryStorage } from './test-storage';

describe('Sidebar', () => {
	it('persists folders and collapsed state', () => {
		const storage = memoryStorage();
		const a = new Sidebar(storage);
		expect(a.addFolder(' prod ')).toBe(true);
		expect(a.addFolder('prod')).toBe(false);
		expect(a.addFolder('dev', ['dev'])).toBe(false);
		expect(a.addFolder('  ')).toBe(false);
		a.toggle('prod');

		const b = new Sidebar(storage);
		expect(b.folders).toEqual(['prod']);
		expect(b.isCollapsed('prod')).toBe(true);
		b.toggle('prod');
		expect(b.isCollapsed('prod')).toBe(false);
	});

	it('renames and removes folders', () => {
		const s = new Sidebar(memoryStorage());
		s.addFolder('a');
		s.addFolder('b');
		s.toggle('a');
		s.renameFolder('a', 'c');
		expect(s.folders).toEqual(['c', 'b']);
		expect(s.isCollapsed('c')).toBe(true);
		s.renameFolder('c', 'b');
		expect(s.folders).toEqual(['b']);
		s.renameFolder('unsaved', 'x');
		expect(s.folders).toEqual(['b', 'x']);
		s.removeFolder('b');
		expect(s.folders).toEqual(['x']);
	});

	it('ignores corrupt state', () => {
		const storage = memoryStorage();
		storage.setItem('guckpg.sidebar', '{nope');
		expect(new Sidebar(storage).folders).toEqual([]);
	});
});
