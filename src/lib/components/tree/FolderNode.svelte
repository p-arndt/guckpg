<script lang="ts">
	import type { Snippet } from 'svelte';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { sidebar } from '#lib/state/sidebar.svelte.js';
	import TreeRow from './TreeRow.svelte';

	interface Props {
		name: string;
		count: number;
		/** Filtering shows matches even in collapsed folders. */
		forceOpen: boolean;
		onRename: (name: string) => void;
		onRemove: (name: string) => void;
		onDropConnection: (id: string, folder: string) => void;
		children: Snippet;
	}

	let { name, count, forceOpen, onRename, onRemove, onDropConnection, children }: Props = $props();

	let dropTarget = $state(false);
	const expanded = $derived(forceOpen || !sidebar.isCollapsed(name));

	function onDragOver(e: DragEvent) {
		if (!sidebar.dragging) return;
		e.preventDefault();
		e.stopPropagation();
		dropTarget = true;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dropTarget = false;
		const id = sidebar.dragging;
		sidebar.dragging = null;
		if (id) onDropConnection(id, name);
	}
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<TreeRow
				{...props}
				depth={0}
				{expanded}
				label={name}
				onclick={() => sidebar.toggle(name)}
				ondragover={onDragOver}
				ondragleave={() => (dropTarget = false)}
				ondrop={onDrop}
				class="font-medium {dropTarget ? 'bg-primary/10 ring-1 ring-primary/40' : ''}"
			>
				{#snippet icon()}
					{#if expanded}
						<FolderOpenIcon class="size-3.5" />
					{:else}
						<FolderIcon class="size-3.5" />
					{/if}
				{/snippet}
				{#snippet trailing()}
					<span class="text-[10px] text-muted-foreground tabular-nums">{count}</span>
				{/snippet}
			</TreeRow>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-52">
		<ContextMenu.Item onSelect={() => sidebar.toggle(name)}>
			{expanded ? 'Collapse' : 'Expand'}
		</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => onRename(name)}>Rename…</ContextMenu.Item>
		<ContextMenu.Separator />
		<ContextMenu.Item variant="destructive" onSelect={() => onRemove(name)}>
			Remove folder
		</ContextMenu.Item>
		<ContextMenu.Label class="text-xs font-normal text-muted-foreground">
			Connections move to the top level.
		</ContextMenu.Label>
	</ContextMenu.Content>
</ContextMenu.Root>

{#if expanded}
	{@render children()}
	{#if count === 0}
		<p class="py-1 pl-10 text-xs text-muted-foreground">Drag connections here</p>
	{/if}
{/if}
