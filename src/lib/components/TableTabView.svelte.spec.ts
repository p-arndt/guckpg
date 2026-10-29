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
