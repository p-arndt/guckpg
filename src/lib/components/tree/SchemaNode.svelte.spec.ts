import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '#lib/api.js';
import { catalog } from '#lib/state/catalog.js';
import { connections, newConnection } from '#lib/state/connections.svelte.js';

const actions = vi.hoisted(() => ({ newView: vi.fn(), editView: vi.fn() }));
vi.mock('#lib/view-actions.js', () => actions);
import SchemaNode from './SchemaNode.svelte';

beforeEach(() => {
	vi.clearAllMocks();
	connections.list = [{ ...newConnection(), id: 'c' }];
	vi.spyOn(api, 'listObjects').mockResolvedValue([]);
});

describe('view actions in the schema tree', () => {
	it('exposes view creation without a context menu', async () => {
		vi.spyOn(catalog, 'relations').mockResolvedValue([
			{ name: 'active_customers', kind: 'view', estimatedRows: null },
			{ name: 'long_table_name', kind: 'table', estimatedRows: 12345 }
		]);
		render(SchemaNode, {
			connectionId: 'c',
			database: 'db',
			schema: 'public',
			depth: 1,
			initiallyExpanded: true
		});
		await fireEvent.click(await screen.findByRole('button', { name: 'New view in public' }));
		expect(actions.newView).toHaveBeenCalledWith('c', 'db', 'public');
		expect(screen.getByRole('treeitem', { name: 'active_customers' })).toBeTruthy();
		const table = screen.getByRole('treeitem', { name: 'long_table_name' });
		expect(table.getAttribute('title')).toBe('long_table_name · ~12k rows');
		expect(table.textContent).not.toContain('~12k');
	});
	it('opens the table designer from the Tables heading', async () => {
		vi.spyOn(catalog, 'relations').mockResolvedValue([]);
		render(SchemaNode, {
			connectionId: 'c',
			database: 'db',
			schema: 'sales',
			depth: 1,
			initiallyExpanded: true
		});
		await fireEvent.click(await screen.findByRole('button', { name: 'New table in sales' }));
		expect(await screen.findByRole('dialog', { name: 'Create table' })).toBeTruthy();
	});
	it('offers view creation in an empty schema', async () => {
		vi.spyOn(catalog, 'relations').mockResolvedValue([]);
		render(SchemaNode, {
			connectionId: 'c',
			database: 'db',
			schema: 'sales',
			depth: 1,
			initiallyExpanded: true
		});
		await fireEvent.click(await screen.findByRole('button', { name: 'Create your first view…' }));
		expect(actions.newView).toHaveBeenCalledWith('c', 'db', 'sales');
	});
});
