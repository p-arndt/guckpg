<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage, type Relation, type SchemaObject } from '#lib/api.js';
	import { groupSchemaObjects, groupRelations } from '#lib/objects.js';
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
	let loading = $state(false);
	let relations = $state<Relation[] | null>(null);
	let objects = $state<SchemaObject[]>([]);
	let refreshing = $state<string | null>(null);
	let creatingTable = $state(false);
	const readOnly = $derived(connections.get(connectionId)?.readOnly ?? true);
	let error = $state<string | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			[relations, objects] = await Promise.all([
				api.listRelations(connectionId, database, schema),
				api.listObjects(connectionId, database, schema)
			]);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void connections.revision[connectionId];
		if (expanded) void load();
	});

	function quoted(name: string) {
		return /^[a-z_][a-z0-9_]*$/.test(name) ? name : `"${name.replaceAll('"', '""')}"`;
	}

	function qualified(name: string) {
		return schema === 'public' ? quoted(name) : `${quoted(schema)}.${quoted(name)}`;
	}

	const relationGroups = $derived(groupRelations(relations ?? []));
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
				{loading}
				label={schema}
				onclick={() => (expanded = !expanded)}
			>
				{#snippet icon()}<FolderIcon class="size-3.5" />{/snippet}
			</TreeRow>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-48">
		<ContextMenu.Item disabled={readOnly} onSelect={() => (creatingTable = true)}
			>New table…</ContextMenu.Item
		>
		<ContextMenu.Item
			onSelect={() =>
				tabs.openQuery(
					connectionId,
					database,
					schema === 'public' ? '' : `SET search_path TO ${quoted(schema)};\n`
				)}
		>
			New query
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => tabs.openDiagram(connectionId, database, schema)}>
			ER diagram
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={load}>Refresh</ContextMenu.Item>
	</ContextMenu.Content>
</ContextMenu.Root>

{#if expanded}
	{#if error}
		<p
			class="py-1 pr-2 text-xs break-words text-destructive"
			style:padding-left="{(depth + 1) * 14 + 26}px"
		>
			{error}
		</p>
	{:else if relations && relations.length === 0 && objects.length === 0}
		<p
			class="py-1 text-[13px] text-muted-foreground"
			style:padding-left="{(depth + 1) * 14 + 26}px"
		>
			No objects
		</p>
	{/if}
	{#each relationGroups as group (group.label)}
		<p
			class="pt-2 pb-1 text-xs text-muted-foreground"
			style:padding-left="{(depth + 1) * 14 + 26}px"
		>
			{group.label}
		</p>
		{#each group.items as rel (rel.name)}
			<ContextMenu.Root>
				<ContextMenu.Trigger>
					{#snippet child({ props })}
						<TreeRow
							{...props}
							depth={depth + 1}
							label={rel.name}
							ondblclick={() => tabs.openTable(connectionId, database, schema, rel.name)}
							onkeydown={(e) => {
								if (e.key === 'Enter') tabs.openTable(connectionId, database, schema, rel.name);
							}}
						>
							{#snippet icon()}
								{@const style = kindStyle(rel.kind)}
								<style.icon class="size-3.5 {style.color}" />
							{/snippet}
							{#snippet trailing()}
								<span
									class="measured text-xs text-muted-foreground opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100"
								>
									{formatEstimate(rel.estimatedRows)}
								</span>
							{/snippet}
						</TreeRow>
					{/snippet}
				</ContextMenu.Trigger>
				<ContextMenu.Content class="w-52">
					{#if rel.kind === 'table' || rel.kind === 'partitioned'}
						<ContextMenu.Item
							onSelect={() => {
								const tab = tabs.openTable(connectionId, database, schema, rel.name);
								tab.requestedView = 'structure';
							}}>Design table</ContextMenu.Item
						>
					{/if}
					<ContextMenu.Item
						onSelect={() => tabs.openTable(connectionId, database, schema, rel.name)}
					>
						Open data
					</ContextMenu.Item>
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
								`SELECT *\nFROM ${qualified(rel.name)}\nLIMIT 100;`
							)}
					>
						New query (SELECT)
					</ContextMenu.Item>
					<ContextMenu.Separator />
					<ContextMenu.Item
						onSelect={async () => {
							await navigator.clipboard.writeText(qualified(rel.name));
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
			class="pt-2 pb-1 text-xs text-muted-foreground"
			style:padding-left="{(depth + 1) * 14 + 26}px"
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
