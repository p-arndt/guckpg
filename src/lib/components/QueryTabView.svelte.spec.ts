import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
vi.mock('./SqlEditor.svelte', async () => ({
	default: (await import('./test-fixtures/SqlEditorStub.svelte')).default
}));
vi.mock('./builder/QueryBuilder.svelte', async () => ({
	default: (await import('./test-fixtures/QueryBuilderStub.svelte')).default
}));

import { QueryTab } from '#lib/state/tabs.svelte.js';
import { confirmation } from '#lib/state/confirm.svelte.js';
import { catalog } from '#lib/state/catalog.js';
import QueryTabView from './test-fixtures/QueryTabViewHarness.svelte';

beforeEach(() => {
	vi.restoreAllMocks();
	vi.spyOn(catalog, 'databases').mockResolvedValue(['db']);
	vi.spyOn(catalog, 'completionTables').mockResolvedValue([]);
});

it('asks before discarding SQL changes in a new view', async () => {
	const tab = new QueryTab('t', 'New view', 'c', 'db', 'SELECT 1;');
	tab.creatingView = true;
	tab.builderOpen = true;
	const ask = vi
		.spyOn(confirmation, 'ask')
		.mockResolvedValueOnce(false)
		.mockResolvedValueOnce(true);
	render(QueryTabView, { tab });
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	tab.sql = 'SELECT 2;';
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(ask).toHaveBeenCalledOnce());
	expect(tab.builderOpen).toBe(false);
	expect(tab.sql).toBe('SELECT 2;');
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(tab.builderOpen).toBe(true));
	expect(tab.sql).toBe('SELECT 1;');
});

it('protects SQL edits made after opening a new view query in the editor', async () => {
	const tab = new QueryTab('t', 'New view', 'c', 'db', '');
	tab.creatingView = true;
	tab.builderOpen = true;
	const ask = vi.spyOn(confirmation, 'ask').mockResolvedValue(false);
	render(QueryTabView, { tab });
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit SQL' }));
	expect(tab.sql).toBe('SELECT 1;\n');
	tab.sql = 'SELECT 2;';
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(ask).toHaveBeenCalledOnce());
	expect(tab.builderOpen).toBe(false);
	expect(tab.sql).toBe('SELECT 2;');
});
