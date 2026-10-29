<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage, type Relation, type SchemaObject } from '#lib/api.js';
	import { groupSchemaObjects, groupRelations } from '#lib/objects.js';
	import { editView, newView } from '#lib/view-actions.js';
	import { Resource } from '#lib/resource.svelte.js';
	import { ident, qualified } from '#lib/sql-quote.js';
	import { catalog } from '#lib/state/catalog.js';
	import { confirmation } from '#lib/state/confirm.svelte.js';
	import { formatEstimate } from '#lib/format.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TreeRow from './TreeRow.svelte';
	import { kindStyle } from './kinds.js';
	import NewTableDialog from '../NewTableDialog.svelte';

	interface Props {
		connectionId: string;
		database: string;
		schema: string;
		depth: number;
		initiallyExpanded?: boolean;
	}

	let { connectionId, database, schema, depth, initiallyExpanded = false }: Props = $props();

	// svelte-ignore state_referenced_locally
	let expanded = $state(initiallyExpanded);
	let refreshing = $state<string | null>(null);
	let creatingTable = $state(false);
	const readOnly = $derived(connections.get(connectionId)?.readOnly ?? true);

	let fresh = false;
	const contents = new Resource(async () => {
		const [relations, objects] = await Promise.all([
			catalog.relations(connectionId, database, schema, fresh),
			api.listObjects(connectionId, database, schema)
		]);
		return { relations, objects };
	});
	const relations = $derived(contents.data?.relations ?? null);
	const objects = $derived(contents.data?.objects ?? []);
	const error = $derived(contents.failed ? errorMessage(contents.error) : null);

	function load(reload = false) {
		fresh = reload;
		void contents.load();
	}

	$effect(() => {
		void connections.revision[connectionId];
		if (expanded) load();
	});

	const tableName = (name: string) => qualified(schema, name);

	const relationGroups = $derived.by(() => {
		if (!relations) return [];
		const groups = groupRelations(relations);
		if (!groups.some((group) => group.label === 'Tables'))
			groups.unshift({ label: 'Tables', items: [] });
		if (!groups.some((group) => group.label === 'Views')) {
			const index = groups.findIndex((group) => group.label === 'Materialized views');
			groups.splice(index < 0 ? groups.length : index, 0, { label: 'Views', items: [] });
		}
		return groups;
	});
	const objectGroups = $derived(groupSchemaObjects(objects));

	async function openDefinition(object: SchemaObject | Relation) {
		try {
			const sql =
				'oid' in object
					? await api.objectDefinition(connectionId, database, schema, object.kind, object.oid)
					: await api.relationDefinition(connectionId, database, schema, object.name);
			tabs.openQuery(connectionId, database, sql);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function refreshMaterializedView(name: string) {
		if (readOnly || refreshing !== null) return;
		refreshing = name;
		try {
			const confirmed = await confirmation.ask({
				title: `Refresh ${name}?`,
				description: 'Refreshing replaces the stored rows and can block readers until it finishes.',
				confirmLabel: 'Refresh materialized view'
			});
			if (!confirmed || readOnly) return;
			await api.refreshMaterializedView(connectionId, database, schema, name);
			toast.success(`Refreshed ${name}`);
			await load();
		} catch (e) {
			toast.error(errorMessage(e));
		} finally {
			refreshing = null;
		}
	}
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<TreeRow
				{...props}
				{depth}
				{expanded}
				loading={contents.loading}
				label={schema}
				onclick={() => (expanded = !expanded)}
			>
				{#snippet icon()}<FolderIcon class="size-3.5" />{/snippet}
			</TreeRow>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-48">
		<ContextMenu.Item disabled={readOnly} onSelect={() => newView(connectionId, database, schema)}
			>New view…</ContextMenu.Item
		>
		<ContextMenu.Item disabled={readOnly} onSelect={() => (creatingTable = true)}
			>New table…</ContextMenu.Item
		>
		<ContextMenu.Item
			onSelect={() =>
				tabs.openQuery(
					connectionId,
					database,
					schema === 'public' ? '' : `SET search_path TO ${ident(schema)};\n`
				)}
		>
			New query
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => tabs.openDiagram(connectionId, database, schema)}>
			ER diagram
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => load(true)}>Refresh</ContextMenu.Item>
	</ContextMenu.Content>
</ContextMenu.Root>

{#if expanded}
	{#if error}
		<p
			class="py-1 pr-2 text-xs break-words text-destructive"
			style:padding-left="{(depth + 1) * 10 + 22}px"
		>
			{error}
		</p>
	{:else if relations && relations.length === 0 && objects.length === 0}
		<p
			class="py-1 text-[13px] text-muted-foreground"
			style:padding-left="{(depth + 1) * 10 + 22}px"
		>
			No objects
		</p>
	{/if}
	{#each relationGroups as group (group.label)}
		<div
			class="flex shrink-0 items-center gap-1 pt-1 pr-2 pb-0.5 text-xs text-muted-foreground"
			style:padding-left="{(depth + 1) * 10 + 22}px"
		>
			<span class="min-w-0 flex-1">{group.label}</span>
			{#if (group.label === 'Views' || group.label === 'Tables') && !readOnly}
				<button
					type="button"
					class="flex size-5 items-center justify-center rounded-md hover:bg-hover hover:text-foreground"
					aria-label="New {group.label === 'Views' ? 'view' : 'table'} in {schema}"
					title="New {group.label === 'Views' ? 'view' : 'table'} in {schema}"
					onclick={() =>
						group.label === 'Views'
							? newView(connectionId, database, schema)
							: (creatingTable = true)}><PlusIcon class="size-4" /></button
				>
			{/if}
		</div>
		{#if group.label === 'Views' && group.items.length === 0 && !readOnly}
			<button
				type="button"
				class="shrink-0 py-1 text-left text-xs text-muted-foreground hover:text-foreground"
				style:padding-left="{(depth + 1) * 10 + 22}px"
				onclick={() => newView(connectionId, database, schema)}>Create your first view…</button
			>
		{/if}
		{#each group.items as rel (rel.name)}
			<ContextMenu.Root>
				<ContextMenu.Trigger>
					{#snippet child({ props })}
						<TreeRow
							{...props}
							depth={depth + 1}
							label={rel.name}
							title={formatEstimate(rel.estimatedRows)
								? `${rel.name} · ${formatEstimate(rel.estimatedRows)} rows`
								: rel.name}
							ondblclick={() => tabs.openTable(connectionId, database, schema, rel.name)}
							onkeydown={(e) => {
								if (e.key === 'Enter') tabs.openTable(connectionId, database, schema, rel.name);
							}}
						>
							{#snippet icon()}
								{@const style = kindStyle(rel.kind)}
								<style.icon class="size-3.5 {style.color}" />
							{/snippet}
						</TreeRow>
					{/snippet}
				</ContextMenu.Trigger>
				<ContextMenu.Content class="w-52">
					{#if rel.kind === 'table' || rel.kind === 'partitioned'}
						<ContextMenu.Item
							onSelect={() =>
								void tabs
									.openTable(connectionId, database, schema, rel.name)
									.switchView('structure')}>Design table</ContextMenu.Item
						>
					{/if}
					<ContextMenu.Item
						onSelect={() => tabs.openTable(connectionId, database, schema, rel.name)}
					>
						Open data
					</ContextMenu.Item>
					{#if rel.kind === 'view'}
						<ContextMenu.Item
							disabled={readOnly}
							onSelect={() => editView(connectionId, database, schema, rel.name)}
							>Edit visually</ContextMenu.Item
						>
					{/if}
					{#if rel.kind === 'view' || rel.kind === 'materialized'}
						<ContextMenu.Item onSelect={() => openDefinition(rel)}>Open definition</ContextMenu.Item
						>
					{/if}
					{#if rel.kind === 'materialized'}
						<ContextMenu.Item
							disabled={readOnly || refreshing !== null}
							onSelect={() => refreshMaterializedView(rel.name)}
						>
							Refresh materialized view
						</ContextMenu.Item>
					{/if}
					<ContextMenu.Item
						onSelect={() =>
							tabs.openQuery(
								connectionId,
								database,
								`SELECT *\nFROM ${tableName(rel.name)}\nLIMIT 100;`
							)}
					>
						New query (SELECT)
					</ContextMenu.Item>
					<ContextMenu.Separator />
					<ContextMenu.Item
						onSelect={async () => {
							await navigator.clipboard.writeText(tableName(rel.name));
							toast.success('Copied name');
						}}
					>
						Copy name
					</ContextMenu.Item>
				</ContextMenu.Content>
			</ContextMenu.Root>
		{/each}
	{/each}
	{#each objectGroups as group (group.kind)}
		<p
			class="shrink-0 pt-1 pb-0.5 text-xs text-muted-foreground"
			style:padding-left="{(depth + 1) * 10 + 22}px"
		>
			{group.label}
		</p>
		{#each group.items as object (object.oid)}
			<ContextMenu.Root>
				<ContextMenu.Trigger>
					{#snippet child({ props })}
						<TreeRow
							{...props}
							depth={depth + 1}
							label={object.identity}
							ondblclick={() => openDefinition(object)}
							onkeydown={(e) => {
								if (e.key === 'Enter') void openDefinition(object);
							}}
						>
							{#snippet icon()}
								{@const style = kindStyle(object.kind)}
								<style.icon class="size-3.5 {style.color}" />
							{/snippet}
						</TreeRow>
					{/snippet}
				</ContextMenu.Trigger>
				<ContextMenu.Content class="w-52">
					<ContextMenu.Item onSelect={() => openDefinition(object)}
						>Open definition</ContextMenu.Item
					>
				</ContextMenu.Content>
			</ContextMenu.Root>
		{/each}
	{/each}
{/if}

<NewTableDialog bind:open={creatingTable} {connectionId} {database} {schema} />
