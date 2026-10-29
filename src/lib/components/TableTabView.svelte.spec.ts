import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
import type { TableTab } from '#lib/state/tabs.svelte.js';

const mocks = vi.hoisted(() => ({ editView: vi.fn(), relations: vi.fn() }));
vi.mock('#lib/view-actions.js', () => ({ editView: mocks.editView }));
vi.mock('#lib/state/catalog.js', () => ({ catalog: { relations: mocks.relations } }));
import TableTabView from './TableTabView.svelte';
import { connections, newConnection } from '#lib/state/connections.svelte.js';

beforeEach(() => {
	vi.clearAllMocks();
	connections.list = [{ ...newConnection(), id: 'c' }];
});

it('offers editing directly in the open view', async () => {
	mocks.relations.mockResolvedValue([{ name: 'active_customers', kind: 'view' }]);
	const tab = {
		id: 't',
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'active_customers',
		view: 'data',
		columns: [],
		output: null,
		running: false,
		dirty: 0,
		structureBusy: false,
		structureDirty: 0,
		offset: 0,
		limit: 200,
		filters: [],
		sort: null,
		open: vi.fn(),
		lastSql: '',
		gridVersion: 0
	} as unknown as TableTab;
	render(TableTabView, { tab });
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit view' }));
	expect(mocks.editView).toHaveBeenCalledWith('c', 'db', 'public', 'active_customers');
});

it('validates conditions and applies explicit connectors', async () => {
	mocks.relations.mockResolvedValue([]);
	const applyFilter = vi.fn();
	const tab = {
		id: 'filters',
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'items',
		view: 'data',
		columns: [{ name: 'name', dataType: 'text' }],
		output: null,
		running: false,
		dirty: 0,
		structureBusy: false,
		structureDirty: 0,
		offset: 0,
		limit: 200,
		filters: [],
		sort: null,
		open: vi.fn(),
		applyFilter,
		lastSql: '',
		gridVersion: 0
	} as unknown as TableTab;
	render(TableTabView, { tab });
	await fireEvent.click(screen.getByRole('button', { name: 'Filter' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Apply filters' }));
	expect(screen.getByRole('alert').textContent).toContain('Choose a column');
	expect(applyFilter).not.toHaveBeenCalled();
	await fireEvent.change(screen.getByRole('combobox', { name: 'Filter column' }), {
		target: { value: 'name' }
	});
	await fireEvent.input(screen.getByRole('combobox', { name: 'Filter value' }), {
		target: { value: 'Alice' }
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Add condition' }));
	await fireEvent.change(screen.getByRole('combobox', { name: 'Condition connector' }), {
		target: { value: 'OR' }
	});
	await fireEvent.change(screen.getAllByRole('combobox', { name: 'Filter column' })[1], {
		target: { value: 'name' }
	});
	await fireEvent.change(screen.getAllByRole('combobox', { name: 'Filter operator' })[1], {
		target: { value: 'isNull' }
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Apply filters' }));
	await vi.waitFor(() =>
		expect(applyFilter).toHaveBeenCalledWith([
			{ column: 'name', op: 'contains', value: 'Alice', connector: 'AND' },
			{ column: 'name', op: 'isNull', value: '', connector: 'OR' }
		])
	);
	expect(screen.queryByRole('button', { name: 'Apply filters' })).toBeNull();
});

it('discards unapplied drafts when reopening the filter', async () => {
	mocks.relations.mockResolvedValue([]);
	const tab = {
		id: 'drafts',
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'items',
		view: 'data',
		columns: [{ name: 'name', dataType: 'text' }],
		output: null,
		running: false,
		dirty: 0,
		structureBusy: false,
		structureDirty: 0,
		offset: 0,
		limit: 200,
		filters: [],
		sort: null,
		open: vi.fn(),
		lastSql: '',
		gridVersion: 0
	} as unknown as TableTab;
	render(TableTabView, { tab });
	await fireEvent.click(screen.getByRole('button', { name: 'Filter' }));
	await fireEvent.input(screen.getByRole('combobox', { name: 'Filter value' }), {
		target: { value: 'draft' }
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Filter' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Filter' }));
	expect((screen.getByRole('combobox', { name: 'Filter value' }) as HTMLInputElement).value).toBe(
		''
	);
});

it('shows active filters and suggests values from the current page', async () => {
	vi.stubGlobal(
		'ResizeObserver',
		class {
			observe() {}
			unobserve() {}
			disconnect() {}
		}
	);
	mocks.relations.mockResolvedValue([]);
	const tab = {
		setGridDirty: vi.fn(),
		id: 'active',
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'items',
		view: 'data',
		columns: [{ name: 'name', dataType: 'text' }],
		output: {
			results: [{ columns: ['name'], rows: [['Alice'], ['Bob'], ['Alice']], source: null }],
			elapsedMs: 1
		},
		running: false,
		dirty: 0,
		structureBusy: false,
		structureDirty: 0,
		offset: 0,
		limit: 200,
		filters: [{ column: 'name', op: 'contains', value: 'Ali', connector: 'AND' }],
		sort: null,
		open: vi.fn(),
		applyFilter: vi.fn(),
		lastSql: '',
		gridVersion: 0
	} as unknown as TableTab;
	const { container } = render(TableTabView, { tab });
	expect(screen.getByLabelText('Active filters').textContent).toContain('name contains Ali');
	await fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
	const input = screen.getByLabelText('Filter value') as HTMLInputElement;
	const list = container.querySelector(`#${input.getAttribute('list')}`)!;
	expect([...list.querySelectorAll('option')].map((option) => option.value)).toEqual([
		'Alice',
		'Bob'
	]);
	await fireEvent.input(input, { target: { value: 'custom' } });
	expect(input.value).toBe('custom');
});
