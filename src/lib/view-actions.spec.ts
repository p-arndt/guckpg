import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	viewDetails: vi.fn(),
	relationDefinition: vi.fn(),
	completionTables: vi.fn(),
	openQuery: vi.fn(),
	error: vi.fn()
}));
vi.mock('#lib/api.js', () => ({
	api: { viewDetails: mocks.viewDetails, relationDefinition: mocks.relationDefinition },
	errorMessage: (e: Error) => e.message
}));
vi.mock('#lib/state/catalog.js', () => ({ catalog: { completionTables: mocks.completionTables } }));
vi.mock('#lib/state/tabs.svelte.js', () => ({ tabs: { openQuery: mocks.openQuery } }));
vi.mock('svelte-sonner', () => ({ toast: { error: mocks.error } }));
import { editView, newView } from './view-actions.js';

beforeEach(() => {
	vi.clearAllMocks();
	mocks.openQuery.mockImplementation(() => ({}));
});

describe('view actions', () => {
	it('opens a new view with the selected schema and visual editor', () => {
		newView('c', 'db', 'sales');
		const tab = mocks.openQuery.mock.results[0].value;
		expect(tab).toMatchObject({
			title: 'New view',
			viewSchema: 'sales',
			creatingView: true,
			builderOpen: true
		});
	});
	it('opens supported definitions in the visual editor', async () => {
		mocks.viewDetails.mockResolvedValue({
			query: 'SELECT customers.id FROM customers WHERE customers.active;',
			options: '',
			sources: [{ schema: 'public', name: 'customers' }],
			searchPath: ['pg_catalog', 'public']
		});
		mocks.completionTables.mockResolvedValue([
			{ schema: 'public', name: 'customers', columns: ['id', 'active'] }
		]);
		await editView('c', 'db', 'public', 'active_customers');
		const tab = mocks.openQuery.mock.results[0].value;
		expect(tab.builderOpen).toBe(true);
		expect(tab.builder.model.filters[0].operator).toBe('IS TRUE');
		expect(tab.editingView.name).toBe('active_customers');
	});
	it('explains SQL fallback in the tab when the visual editor cannot import the view', async () => {
		mocks.viewDetails.mockResolvedValue({
			query: 'SELECT 1;',
			options: '',
			sources: [],
			searchPath: []
		});
		mocks.completionTables.mockResolvedValue([]);
		mocks.relationDefinition.mockResolvedValue('CREATE OR REPLACE VIEW public.v AS SELECT 1;');
		await editView('c', 'db', 'public', 'v');
		expect(mocks.openQuery).toHaveBeenCalledWith(
			'c',
			'db',
			'CREATE OR REPLACE VIEW public.v AS SELECT 1;'
		);
		expect(mocks.openQuery.mock.results[0].value.viewNotice).toContain(
			'visual builder cannot edit'
		);
	});
});
