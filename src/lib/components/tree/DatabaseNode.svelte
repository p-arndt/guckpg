<script lang="ts">
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { api, errorMessage } from '#lib/api.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TreeRow from './TreeRow.svelte';
	import SchemaNode from './SchemaNode.svelte';

	interface Props {
		connectionId: string;
		database: string;
		depth: number;
		initiallyExpanded?: boolean;
	}

	let { connectionId, database, depth, initiallyExpanded = false }: Props = $props();

	// svelte-ignore state_referenced_locally
	let expanded = $state(initiallyExpanded);
	let loading = $state(false);
	let schemas = $state<string[] | null>(null);
	let showSystem = $state(false);
	let error = $state<string | null>(null);

	async function load() {
		loading = true;
		error = null;
		try {
			schemas = await api.listSchemas(connectionId, database, showSystem);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void connections.revision[connectionId];
		void showSystem;
		if (expanded) void load();
	});
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<TreeRow
				{...props}
				{depth}
				{expanded}
				{loading}
				label={database}
				onclick={() => (expanded = !expanded)}
			>
				{#snippet icon()}<DatabaseIcon class="size-3.5" />{/snippet}
			</TreeRow>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-52">
		<ContextMenu.Item onSelect={() => tabs.openQuery(connectionId, database)}>
			New query
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={load}>Refresh</ContextMenu.Item>
		<ContextMenu.CheckboxItem bind:checked={showSystem}
			>Show system schemas</ContextMenu.CheckboxItem
		>
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
	{/if}
	{#each schemas ?? [] as schema (schema)}
		<SchemaNode
			{connectionId}
			{database}
			{schema}
			depth={depth + 1}
			initiallyExpanded={schema === 'public'}
		/>
	{/each}
{/if}
