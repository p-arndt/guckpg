import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
vi.mock('../SqlEditor.svelte', async () => ({
	default: (await import('../test-fixtures/SqlEditorStub.svelte')).default
}));
vi.mock('./BuilderCanvas.svelte', async () => ({
	default: (await import('../test-fixtures/BuilderCanvasStub.svelte')).default,
	TABLE_DRAG_TYPE: 'table'
}));
import { api } from '#lib/api.js';
import Harness from '../test-fixtures/InlineBuilderHarness.svelte';

beforeEach(() => {
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
	);
	vi.spyOn(api, 'schemaDiagram').mockResolvedValue({ tables: [], foreignKeys: [] });
});

it('edits, previews and applies SQL without leaving the builder', async () => {
	const onrun = vi.fn();
	render(Harness, { onrun, onopen: vi.fn() });
	const sql = 'SELECT id FROM public.customers WHERE id > 5;';
	await fireEvent.input(screen.getByRole('textbox', { name: 'SQL' }), { target: { value: sql } });
	expect(screen.getByLabelText('Built query').textContent).toContain('> 1');
	await fireEvent.click(screen.getByRole('button', { name: /Preview/ }));
	expect(onrun).toHaveBeenCalledExactlyOnceWith(sql);
	await fireEvent.click(screen.getByRole('button', { name: 'Apply to builder' }));
	expect(screen.getByLabelText('Built query').textContent).toContain('> 5');
	expect(screen.queryByRole('button', { name: 'Apply to builder' })).toBeNull();
	await fireEvent.input(screen.getByRole('textbox', { name: 'Value' }), { target: { value: '8' } });
	expect((screen.getByRole('textbox', { name: 'SQL' }) as HTMLTextAreaElement).value).toContain(
		'> 8'
	);
});

it('keeps unsupported drafts until they are explicitly discarded or expanded', async () => {
	const onopen = vi.fn();
	render(Harness, { onrun: vi.fn(), onopen });
	const sql = 'SELECT now();';
	await fireEvent.input(screen.getByRole('textbox', { name: 'SQL' }), { target: { value: sql } });
	expect(
		(screen.getByRole('button', { name: 'Apply to builder' }) as HTMLButtonElement).disabled
	).toBe(true);
	expect((screen.getByRole('button', { name: 'Clear' }) as HTMLButtonElement).disabled).toBe(true);
	await fireEvent.click(screen.getByRole('button', { name: 'Expand' }));
	expect(onopen).toHaveBeenCalledWith(sql);
	await fireEvent.click(screen.getByRole('button', { name: 'Discard edits' }));
	expect((screen.getByRole('textbox', { name: 'SQL' }) as HTMLTextAreaElement).value).toContain(
		'> 1'
	);
});
