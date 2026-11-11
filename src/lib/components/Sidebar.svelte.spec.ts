import { fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import { connections, newConnection } from '#lib/state/connections.svelte.js';
import { sidebar } from '#lib/state/sidebar.svelte.js';

vi.mock('#lib/state/catalog.js', () => ({
	catalog: {
		databases: vi.fn().mockResolvedValue(['postgres']),
		schemas: vi.fn().mockResolvedValue([])
	}
}));
import Sidebar from './Sidebar.svelte';

describe('connected connections', () => {
	it('shows connected nodes once above the other connections and respects the filter', async () => {
		connections.list = [
			{ ...newConnection(), id: 'one', name: 'Production', folder: 'Servers' },
			{ ...newConnection(), id: 'two', name: 'Development' }
		];
		connections.status = { one: 'connected' };
		sidebar.folders = ['Servers'];
		sidebar.collapsed = [];
		sidebar.filter = '';
		sidebar.open = [];
		render(Sidebar, { onEdit: vi.fn(), onDelete: vi.fn() });
		const nodes = screen.getAllByRole('treeitem');
		expect(nodes[0].getAttribute('data-connection-id')).toBe('one');
		expect(nodes.filter((node) => node.getAttribute('data-connection-id') === 'one')).toHaveLength(
			1
		);
		await fireEvent.click(nodes[0], { detail: 1 });
		expect(sidebar.isOpen('one')).toBe(false);
		await fireEvent.dblClick(nodes[0]);
		expect(sidebar.isOpen('one')).toBe(true);
		const database = await screen.findByRole('treeitem', { name: 'postgres' });
		expect(database.getAttribute('aria-expanded')).toBe('false');
		await fireEvent.click(database);
		expect(database.getAttribute('aria-expanded')).toBe('true');
		await fireEvent.click(nodes[0], { detail: 0 });
		expect(sidebar.isOpen('one')).toBe(false);
		await fireEvent.input(screen.getByRole('textbox', { name: 'Filter connections' }), {
			target: { value: 'Development' }
		});
		expect(screen.queryByRole('treeitem', { name: 'Production' })).toBeNull();
		expect(screen.getByRole('treeitem', { name: 'Development' })).toBeTruthy();
	});
});

describe('dragging connections', () => {
	function setup() {
		connections.list = [
			{ ...newConnection(), id: 'one', name: 'Production', folder: 'Servers' },
			{ ...newConnection(), id: 'two', name: 'Development' }
		];
		connections.status = {};
		sidebar.folders = ['Servers'];
		sidebar.collapsed = [];
		sidebar.filter = '';
		sidebar.open = [];
		sidebar.sort = 'manual';
		const arrange = vi.spyOn(connections, 'arrange').mockResolvedValue();
		render(Sidebar, { onEdit: vi.fn(), onDelete: vi.fn() });
		return arrange;
	}

	it('adds no drop box while dragging and drops on empty space to leave the folder', async () => {
		const arrange = setup();
		const row = screen.getByRole('treeitem', { name: 'Production' });
		await fireEvent.dragStart(row);
		await tick();
		expect(screen.queryByText(/drop here/i)).toBeNull();

		const empty = document.querySelector('[data-root-drop]')!;
		await fireEvent.dragOver(empty);
		await fireEvent.drop(empty);
		expect(arrange).toHaveBeenCalledWith([
			{ id: 'two', folder: null },
			{ id: 'one', folder: null }
		]);
		arrange.mockRestore();
	});

	it('only changes the folder when dropping onto a connection in a sorted list', async () => {
		const arrange = setup();
		sidebar.sort = 'name';
		await tick();
		await fireEvent.dragStart(screen.getByRole('treeitem', { name: 'Development' }));
		const target = screen.getByRole('treeitem', { name: 'Production' });
		await fireEvent.dragOver(target);
		await fireEvent.drop(target);
		expect(arrange).toHaveBeenCalledWith([
			{ id: 'one', folder: 'Servers' },
			{ id: 'two', folder: 'Servers' }
		]);
		arrange.mockRestore();
	});
});
