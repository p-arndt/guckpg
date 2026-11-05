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
import { buildSql } from '#lib/query-builder.js';
import { importViewQuery } from '#lib/view-builder.js';
import QueryTabView from './test-fixtures/QueryTabViewHarness.svelte';

beforeEach(() => {
	vi.restoreAllMocks();
	vi.spyOn(catalog, 'databases').mockResolvedValue(['db']);
	vi.spyOn(catalog, 'completionTables').mockResolvedValue([]);
});

it('uses inline SQL drafts for preview and preserves them when expanding the editor', async () => {
	const tables = [{ schema: 'public', name: 'customers', columns: ['id'] }];
	vi.spyOn(catalog, 'completionTables').mockResolvedValue(tables);
	const query = 'SELECT id FROM public.customers WHERE id > 1;';
	const draft = 'SELECT id FROM public.customers WHERE id > 5;';
	const tab = new QueryTab('t', 'View', 'c', 'db', '');
	tab.creatingView = true;
	tab.builderOpen = true;
	tab.builder = { database: 'db', model: importViewQuery(query, tables, tables)!, sqlDraft: draft };
	const execute = vi.spyOn(tab, 'execute').mockResolvedValue();
	render(QueryTabView, { tab });
	await fireEvent.click(screen.getByRole('button', { name: /Preview/ }));
	expect(execute).toHaveBeenCalledWith(draft, 0, false);
	expect(screen.getByRole('button', { name: 'Create view' }).getAttribute('title')).toBe(
		'Apply or discard the SQL edits first'
	);
	await fireEvent.click(screen.getByRole('button', { name: 'SQL editor' }));
	expect(tab.sql).toBe(draft + '\n');
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	expect(buildSql(tab.builder!.model).sql).toContain('> 5');
});

it('updates the visual builder when returning from supported SQL edits', async () => {
	vi.spyOn(catalog, 'completionTables').mockResolvedValue([
		{ schema: 'public', name: 'customers', columns: ['id'] }
	]);
	const tab = new QueryTab('t', 'New view', 'c', 'db', '');
	tab.creatingView = true;
	tab.builderOpen = true;
	const ask = vi.spyOn(confirmation, 'ask');
	render(QueryTabView, { tab });
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit SQL' }));
	tab.sql = 'SELECT customers.id FROM public.customers WHERE customers.id > 2;';
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(tab.builderOpen).toBe(true));
	expect(buildSql(tab.builder!.model).sql).toContain('WHERE customers.id > 2');
	expect(ask).not.toHaveBeenCalled();
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
	await fireEvent.click(screen.getByRole('button', { name: 'SQL editor' }));
	tab.sql = 'SELECT 2;';
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(ask).toHaveBeenCalledOnce());
	expect(tab.builderOpen).toBe(false);
	expect(tab.sql).toBe('SELECT 2;');
	await fireEvent.click(screen.getByRole('button', { name: 'Visual editor' }));
	await waitFor(() => expect(tab.builderOpen).toBe(true));
	expect(tab.sql).toBe('SELECT 1;');
});

it.each([false, true])(
	'keeps both editor modes synchronized (existing view: %s)',
	async (existing) => {
		const tables = [{ schema: 'public', name: 'customers', columns: ['id'] }];
		vi.spyOn(catalog, 'completionTables').mockResolvedValue(tables);
		const query = 'SELECT customers.id FROM public.customers WHERE customers.id > 1;';
		const tab = new QueryTab('t', 'View', 'c', 'db', 'SELECT 42;');
		tab.creatingView = !existing;
		if (existing)
			tab.editingView = {
				database: 'db',
				schema: 'public',
				name: 'report',
				details: { query, sources: tables, searchPath: ['pg_catalog', 'public'], options: '' }
			};
		tab.builder = { database: 'db', model: importViewQuery(query, tables, tables)! };
		tab.builder.model.tables[0].x = 450;
		tab.builder.model.tables[0].y = 200;
		tab.builderOpen = true;
		const ask = vi.spyOn(confirmation, 'ask');
		render(QueryTabView, { tab });
		const visual = screen.getByRole('button', { name: 'Visual editor' });
		const sql = screen.getByRole('button', { name: 'SQL editor' });
		await fireEvent.click(visual);
		expect(tab.builderOpen).toBe(true);
		await fireEvent.click(sql);
		expect(tab.sql).toContain('WHERE customers.id > 1');
		expect(tab.sql.startsWith('SELECT 42;\n\n')).toBe(true);
		if (existing) expect(tab.sql).toContain('CREATE OR REPLACE VIEW "public"."report" AS');
		expect(sql.getAttribute('aria-pressed')).toBe('true');
		tab.sql = tab.sql.replace('> 1', '> 2');
		await fireEvent.click(sql);
		expect(tab.builderOpen).toBe(false);
		expect(tab.sql).toContain('> 2');
		await fireEvent.click(visual);
		expect(buildSql(tab.builder!.model).sql).toContain('> 2');
		expect(tab.builder!.model.tables[0]).toMatchObject({ x: 450, y: 200 });
		tab.builder!.model.filters[0].value = '3';
		await fireEvent.click(sql);
		expect(tab.sql).toContain('> 3');
		expect(tab.sql.match(/FROM public.customers/g)).toHaveLength(1);
		expect(tab.sql.startsWith('SELECT 42;\n\n')).toBe(true);
		expect(ask).not.toHaveBeenCalled();
	}
);

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
