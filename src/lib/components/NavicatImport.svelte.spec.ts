import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke, load } = vi.hoisted(() => ({ invoke: vi.fn(), load: vi.fn() }));
vi.mock('#lib/api.js', () => ({ invoke, errorMessage: (e: Error) => e.message }));
vi.mock('#lib/state/connections.svelte.js', () => ({ connections: { load } }));

import NavicatImport from './NavicatImport.svelte';

function file(content: string, size = content.length) {
	return {
		size,
		arrayBuffer: async () => new TextEncoder().encode(content).buffer
	};
}

async function openImport() {
	render(NavicatImport);
	await fireEvent.click(screen.getByRole('button', { name: 'Import Navicat connections' }));
	return document.querySelector('input[type=file]')!;
}

describe('NavicatImport', () => {
	beforeEach(() => {
		invoke.mockReset();
		load.mockReset().mockResolvedValue(undefined);
	});

	it('imports the selected file, refreshes connections and reports warnings', async () => {
		invoke.mockResolvedValue({ imported: 2, skipped: 1, warnings: ['Password unavailable'] });
		const input = await openImport();
		await fireEvent.change(input, { target: { files: [file('<connections/>')] } });
		await waitFor(() => expect(load).toHaveBeenCalledOnce());
		expect(invoke).toHaveBeenCalledWith('import_navicat', { xml: '<connections/>' });
		expect(await screen.findByText('Imported 2 connections; skipped 1.')).toBeTruthy();
		expect(screen.getByText('Password unavailable')).toBeTruthy();
	});

	it('shows errors and refreshes after a failed import', async () => {
		invoke.mockRejectedValue(new Error('Invalid export'));
		const input = await openImport();
		await fireEvent.change(input, { target: { files: [file('bad')] } });
		expect((await screen.findByRole('alert')).textContent).toContain('Invalid export');
		await waitFor(() => expect(load).toHaveBeenCalledOnce());
	});

	it('rejects oversized files before reading them', async () => {
		const input = await openImport();
		await fireEvent.change(input, { target: { files: [file('', 16 * 1024 * 1024 + 1)] } });
		expect((await screen.findByRole('alert')).textContent).toContain('16 MiB');
		expect(invoke).not.toHaveBeenCalled();
	});
});
