import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runQuery, closeTab, schemas, refresh, get, viewDetails } = vi.hoisted(() => ({
	viewDetails: vi.fn(),
	runQuery: vi.fn(),
	closeTab: vi.fn(),
	schemas: vi.fn(),
	refresh: vi.fn(),
	get: vi.fn()
}));
vi.mock('#lib/api.js', () => ({
	api: { runQuery, closeTab, viewDetails },
	errorMessage: (error: Error) => error.message
}));
vi.mock('#lib/state/catalog.js', () => ({ catalog: { schemas } }));
vi.mock('#lib/state/connections.svelte.js', () => ({ connections: { refresh, get } }));
vi.mock('svelte-sonner', () => ({ toast: { success: vi.fn() } }));
import SaveViewDialog from './SaveViewDialog.svelte';

beforeEach(() => {
	vi.clearAllMocks();
	schemas.mockResolvedValue(['public', 'sales']);
	get.mockReturnValue({ readOnly: false });
	runQuery.mockResolvedValue({ results: [] });
	closeTab.mockResolvedValue(undefined);
});

describe('save as view', () => {
	it('previews and creates the view in the selected schema on an isolated connection', async () => {
		render(SaveViewDialog, {
			open: true,
			connectionId: 'db',
			database: 'app',
			query: 'SELECT 1;',
			defaultSchema: 'sales'
		});
		await screen.findByRole('combobox', { name: 'Schema' });
		await waitFor(() => expect(schemas).toHaveBeenCalledWith('db', 'app'));
		await fireEvent.input(screen.getByLabelText('View name'), { target: { value: 'report' } });
		await waitFor(() =>
			expect(document.querySelector('pre')?.textContent).toContain('"sales"."report"')
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Create view' }));
		await waitFor(() => expect(runQuery).toHaveBeenCalledOnce());
		const [id, connection, database, sql] = runQuery.mock.calls[0];
		expect(id).toMatch(/^create-view-/);
		expect([connection, database, sql]).toEqual([
			'db',
			'app',
			'CREATE VIEW "sales"."report" AS\nSELECT 1;'
		]);
		await waitFor(() => expect(closeTab).toHaveBeenCalledWith(id));
		expect(refresh).toHaveBeenCalledWith('db');
	});
	it('prevents creation on a read-only connection', async () => {
		get.mockReturnValue({ readOnly: true });
		render(SaveViewDialog, { open: true, connectionId: 'db', database: 'app', query: 'SELECT 1;' });
		await waitFor(() =>
			expect(screen.getByRole('combobox', { name: 'Schema' }).textContent).toContain('public')
		);
		await fireEvent.input(screen.getByLabelText('View name'), { target: { value: 'report' } });
		expect(
			(screen.getByRole('button', { name: 'Create view' }) as HTMLButtonElement).disabled
		).toBe(true);
		expect(runQuery).not.toHaveBeenCalled();
	});
	it('shows database errors and does not refresh the catalog', async () => {
		runQuery.mockRejectedValue(new Error('relation already exists'));
		render(SaveViewDialog, { open: true, connectionId: 'db', database: 'app', query: 'SELECT 1;' });
		await waitFor(() =>
			expect(screen.getByRole('combobox', { name: 'Schema' }).textContent).toContain('public')
		);
		await fireEvent.input(screen.getByLabelText('View name'), { target: { value: 'report' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create view' }));
		expect((await screen.findByRole('alert')).textContent).toContain('relation already exists');
		expect(refresh).not.toHaveBeenCalled();
	});
	it('preserves options when updating and detects an external change', async () => {
		const details = {
			query: 'SELECT customers.id FROM customers;',
			options: "security_barrier = 'true'",
			searchPath: ['pg_catalog', 'public'],
			sources: [{ schema: 'public', name: 'customers' }]
		};
		viewDetails.mockResolvedValue({ ...details, query: 'SELECT customers.name FROM customers;' });
		render(SaveViewDialog, {
			open: true,
			connectionId: 'db',
			database: 'app',
			query: 'SELECT customers.id FROM public.customers;',
			editingView: { database: 'app', schema: 'public', name: 'report', details }
		});
		expect(document.querySelector('pre')?.textContent).toContain(
			"WITH (security_barrier = 'true')"
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Update view' }));
		expect((await screen.findByRole('alert')).textContent).toContain('changed since it was opened');
		expect(runQuery).not.toHaveBeenCalled();
		expect(refresh).not.toHaveBeenCalled();
	});
	it('updates the view and refreshes its snapshot', async () => {
		const details = {
			query: 'SELECT customers.id FROM customers;',
			options: "security_barrier = 'true'",
			searchPath: ['pg_catalog', 'public'],
			sources: [{ schema: 'public', name: 'customers' }]
		};
		const updated = { ...details, query: 'SELECT customers.name FROM customers;' };
		viewDetails.mockResolvedValueOnce(details).mockResolvedValueOnce(updated);
		render(SaveViewDialog, {
			open: true,
			connectionId: 'db',
			database: 'app',
			query: 'SELECT customers.name FROM public.customers;',
			editingView: { database: 'app', schema: 'public', name: 'report', details }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Update view' }));
		await waitFor(() => expect(runQuery).toHaveBeenCalledOnce());
		expect(runQuery.mock.calls[0][3]).toBe(
			'CREATE OR REPLACE VIEW "public"."report" WITH (security_barrier = \'true\') AS\nSELECT customers.name FROM public.customers;'
		);
		await waitFor(() => expect(refresh).toHaveBeenCalledWith('db'));
	});
});
