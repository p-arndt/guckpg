import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { connections, newConnection } from '#lib/state/connections.svelte.js';
import { sidebar } from '#lib/state/sidebar.svelte.js';

vi.mock('#lib/state/catalog.js', () => ({ catalog: { databases: vi.fn().mockResolvedValue([]) } }));
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
		await fireEvent.click(nodes[0]);
		expect(sidebar.isOpen('one')).toBe(true);
		await fireEvent.input(screen.getByRole('textbox', { name: 'Filter connections' }), {
			target: { value: 'Development' }
		});
		expect(screen.queryByRole('treeitem', { name: 'Production' })).toBeNull();
		expect(screen.getByRole('treeitem', { name: 'Development' })).toBeTruthy();
	});
});
