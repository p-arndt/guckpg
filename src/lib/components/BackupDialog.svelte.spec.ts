import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import { invoke } from '#lib/api.js';
import { confirmation } from '#lib/state/confirm.svelte.js';
import BackupDialog from './BackupDialog.svelte';

vi.mock('#lib/api.js', () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	errorMessage: String
}));

it('exports the selected SQL contents and table patterns', async () => {
	render(BackupDialog, { open: true, id: 'c', database: 'db' });
	await fireEvent.change(screen.getByLabelText('Format'), { target: { value: 'true' } });
	await fireEvent.input(screen.getByLabelText('File path'), { target: { value: '/tmp/db.sql' } });
	await fireEvent.change(screen.getByLabelText('Export contents'), { target: { value: 'schema' } });
	await fireEvent.input(screen.getByLabelText(/Tables to export/), {
		target: { value: 'public.a\npublic.b' }
	});
	await fireEvent.click(screen.getByLabelText(/Include DROP/));
	await fireEvent.click(screen.getByRole('button', { name: 'Export' }));
	await waitFor(() =>
		expect(invoke).toHaveBeenCalledWith('backup_database', {
			id: 'c',
			database: 'db',
			path: '/tmp/db.sql',
			options: { sql: true, tables: ['public.a', 'public.b'], contents: 'schema', clean: true }
		})
	);
});

it('requires confirmation before restoring SQL', async () => {
	vi.clearAllMocks();
	vi.spyOn(confirmation, 'ask').mockResolvedValue(false);
	render(BackupDialog, { open: true, id: 'c', database: 'db' });
	await fireEvent.change(screen.getByLabelText('Format'), { target: { value: 'true' } });
	await fireEvent.input(screen.getByLabelText('File path'), { target: { value: '/tmp/db.sql' } });
	await fireEvent.click(screen.getByRole('radio', { name: 'Restore' }));
	expect(screen.queryByLabelText('Export contents')).toBeNull();
	await fireEvent.click(screen.getByRole('button', { name: 'Restore' }));
	expect(confirmation.ask).toHaveBeenCalledOnce();
	expect(invoke).not.toHaveBeenCalled();
});
