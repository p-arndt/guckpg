<script lang="ts">
	import { api, errorMessage, type ViewDetails } from '#lib/api.js';
	import { toast } from 'svelte-sonner';
	import { catalog } from '#lib/state/catalog.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { createViewSql, replaceViewSql } from '#lib/view-builder.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import SqlBlock from './SqlBlock.svelte';

	let {
		open = $bindable(false),
		connectionId,
		database,
		query,
		defaultSchema = '',
		problems = [],
		editingView = null
	}: {
		open: boolean;
		connectionId: string;
		database: string;
		query: string;
		defaultSchema?: string;
		problems?: string[];
		editingView?: { database: string; schema: string; name: string; details: ViewDetails } | null;
	} = $props();
	let schemas = $state<string[]>([]);
	let schema = $state('');
	let name = $state('');
	let loading = $state(false);
	let busy = $state(false);
	let error = $state('');

	$effect(() => {
		if (!open) return;
		const id = connectionId;
		const db = database;
		const preferred = defaultSchema;
		const existing = editingView;
		let current = true;
		loading = true;
		error = '';
		name = existing?.name ?? '';
		schema = existing?.schema ?? '';
		schemas = existing ? [existing.schema] : [];
		if (existing) {
			loading = false;
			return;
		}
		catalog
			.schemas(id, db)
			.then(
				(list) => {
					if (!current) return;
					schemas = list;
					schema = list.includes(preferred)
						? preferred
						: list.includes('public')
							? 'public'
							: (list[0] ?? '');
				},
				(e) => {
					if (current) error = errorMessage(e);
				}
			)
			.finally(() => {
				if (current) loading = false;
			});
		return () => {
			current = false;
		};
	});

	const preview = $derived.by(() => {
		try {
			return editingView
				? replaceViewSql(schema, name, editingView.details, query)
				: createViewSql(schema, name, query);
		} catch {
			return '';
		}
	});
	async function save() {
		if (busy || !preview || connections.get(connectionId)?.readOnly !== false) return;
		busy = true;
		error = '';
		const tabId = `create-view-${crypto.randomUUID()}`;
		try {
			if (editingView) {
				const current = await api.viewDetails(connectionId, database, schema, name);
				if (
					current.query !== editingView.details.query ||
					current.options !== editingView.details.options ||
					JSON.stringify(current.sources) !== JSON.stringify(editingView.details.sources) ||
					JSON.stringify(current.searchPath) !== JSON.stringify(editingView.details.searchPath)
				)
					throw new Error('This view changed since it was opened. Reopen it before saving.');
			}
			await api.runQuery(tabId, connectionId, database, preview, 1);
			if (editingView) {
				try {
					editingView.details = await api.viewDetails(connectionId, database, schema, name);
				} catch {
					/* A later save will detect stale details and require reopening. */
				}
			}
			connections.refresh(connectionId);
			toast.success(`${editingView ? 'Updated' : 'Created'} view ${name.trim()}`);
			open = false;
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
			void api.closeTab(tabId).catch(() => {});
		}
	}
</script>

<Dialog.Root
	{open}
	onOpenChange={(next) => {
		if (!busy) open = next;
	}}
>
	<Dialog.Content class="sm:max-w-2xl" showCloseButton={!busy}>
		<Dialog.Header>
			<Dialog.Title>{editingView ? 'Update view' : 'Save as view'}</Dialog.Title>
			<Dialog.Description
				>{editingView
					? 'Review the SQL before updating the view'
					: 'Choose a schema and name, then review the SQL before creating the view'} in {database}.</Dialog.Description
			>
		</Dialog.Header>
		<div class="grid gap-3 sm:grid-cols-2">
			<label class="flex flex-col gap-1.5 text-sm"
				>Schema
				<Select.Root type="single" bind:value={schema} disabled={loading || busy || !!editingView}>
					<Select.Trigger class="w-full" aria-label="Schema"
						>{schema || (loading ? 'Loading…' : 'Choose schema')}</Select.Trigger
					>
					<Select.Content
						>{#each schemas as item (item)}<Select.Item
								value={item}
								label={item}
							/>{/each}</Select.Content
					>
				</Select.Root>
			</label>
			<label class="flex flex-col gap-1.5 text-sm"
				>View name
				<Input
					bind:value={name}
					disabled={busy || !!editingView}
					placeholder="e.g. active_customers"
				/>
			</label>
		</div>
		{#if problems.length}<ul class="text-xs text-signal-text">
				{#each problems as problem (problem)}<li>{problem}</li>{/each}
			</ul>{/if}
		{#if name && !preview}<p class="text-xs text-bad">
				Enter a valid view name of at most 63 bytes and choose a schema.
			</p>{/if}
		{#if error}<p role="alert" class="text-sm text-bad">{error}</p>{/if}
		<SqlBlock
			sql={preview || '-- Enter a view name to preview the SQL.'}
			class="measured max-h-[45vh] overflow-auto rounded-lg border bg-background p-3 text-xs whitespace-pre-wrap select-text"
		/>
		<Dialog.Footer>
			<Button variant="outline" disabled={busy} onclick={() => (open = false)}>Cancel</Button>
			<Button
				disabled={busy || loading || !preview || connections.get(connectionId)?.readOnly !== false}
				onclick={save}>{busy ? 'Saving…' : editingView ? 'Update view' : 'Create view'}</Button
			>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
