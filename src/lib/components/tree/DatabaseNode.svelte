<script lang="ts">
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { errorMessage } from '#lib/api.js';
	import { Resource } from '#lib/resource.svelte.js';
	import { catalog } from '#lib/state/catalog.js';
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
	let showSystem = $state(false);

	let fresh = false;
	const schemas = new Resource(() => catalog.schemas(connectionId, database, showSystem, fresh));
	const error = $derived(schemas.failed ? errorMessage(schemas.error) : null);

	function load(reload = false) {
		fresh = reload;
		void schemas.load();
	}

	$effect(() => {
		void connections.revision[connectionId];
		void showSystem;
		if (expanded) load();
	});
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<TreeRow
				{...props}
				{depth}
				{expanded}
				loading={schemas.loading}
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
		<ContextMenu.Item onSelect={() => load(true)}>Refresh</ContextMenu.Item>
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
	{#each schemas.data ?? [] as schema (schema)}
		<SchemaNode
			{connectionId}
			{database}
			{schema}
			depth={depth + 1}
			initiallyExpanded={schema === 'public'}
		/>
	{/each}
{/if}
