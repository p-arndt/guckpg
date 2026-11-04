import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import { invoke } from '#lib/api.js';
import CsvTransfer from './CsvTransfer.svelte';

vi.mock('#lib/api.js', () => ({ invoke: vi.fn(), errorMessage: String }));

it('imports the file chosen in the native picker', async () => {
	vi.mocked(invoke).mockResolvedValueOnce('/tmp/data.csv').mockResolvedValueOnce(1);
	const onImported = vi.fn();
	render(CsvTransfer, {
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'items',
		readOnly: false,
		onImported
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Import CSV' }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith('import_csv', {
			id: 'c',
			database: 'db',
			schema: 'public',
			table: 'items',
			path: '/tmp/data.csv'
		})
	);
	expect(onImported).toHaveBeenCalledOnce();
});

it('reports a picker error', async () => {
	vi.mocked(invoke).mockRejectedValueOnce('Picker unavailable');
	render(CsvTransfer, {
		connectionId: 'c',
		database: 'db',
		schema: 'public',
		table: 'items',
		readOnly: false,
		onImported: vi.fn()
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Export CSV' }));
	expect((await screen.findByRole('alert')).textContent).toContain('Picker unavailable');
});
