<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import EyeIcon from '@lucide/svelte/icons/eye';
	import LayersIcon from '@lucide/svelte/icons/layers';
	import GlobeIcon from '@lucide/svelte/icons/globe';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage, type Relation } from '#lib/api.js';
	import { formatEstimate } from '#lib/format.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TreeRow from './TreeRow.svelte';

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
	let error = $state<string | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			relations = await api.listRelations(connectionId, database, schema);
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

	const tables = $derived(relations?.filter((r) => r.kind !== 'view') ?? []);
	const views = $derived(relations?.filter((r) => r.kind === 'view') ?? []);
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
			style:padding-left="{(depth + 1) * 12 + 24}px"
		>
			{error}
		</p>
	{:else if relations && relations.length === 0}
		<p class="py-1 text-xs text-muted-foreground" style:padding-left="{(depth + 1) * 12 + 24}px">
			No tables
		</p>
	{/if}
	{#each [...tables, ...views] as rel (rel.name)}
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
							{#if rel.kind === 'view'}
								<EyeIcon class="size-3.5 text-sky-500" />
							{:else if rel.kind === 'materialized'}
								<LayersIcon class="size-3.5 text-violet-500" />
							{:else if rel.kind === 'foreign'}
								<GlobeIcon class="size-3.5 text-teal-500" />
							{:else}
								<TableIcon class="size-3.5 text-amber-600 dark:text-amber-400" />
							{/if}
						{/snippet}
						{#snippet trailing()}
							<span class="text-[10px] text-muted-foreground opacity-0 group-hover:opacity-100">
								{formatEstimate(rel.estimatedRows)}
							</span>
						{/snippet}
					</TreeRow>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Content class="w-52">
				<ContextMenu.Item onSelect={() => tabs.openTable(connectionId, database, schema, rel.name)}>
					Open data
				</ContextMenu.Item>
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
{/if}
