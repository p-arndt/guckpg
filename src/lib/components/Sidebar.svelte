<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import FolderPlusIcon from '@lucide/svelte/icons/folder-plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Button } from '#lib/components/ui/button/index.js';
	import { errorMessage, type ConnectionConfig, type Placement } from '#lib/api.js';
	import { moveConnection, renameFolder, sidebarLayout } from '#lib/sidebar.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { prompt } from '#lib/state/prompt.svelte.js';
	import { sidebar } from '#lib/state/sidebar.svelte.js';
	import ConnectionNode from './tree/ConnectionNode.svelte';
	import FolderNode from './tree/FolderNode.svelte';

	interface Props {
		onEdit: (c: ConnectionConfig | null) => void;
		onDelete: (c: ConnectionConfig) => void;
	}

	let { onEdit, onDelete }: Props = $props();

	let filterInput = $state<HTMLInputElement | null>(null);
	let rootDrop = $state(false);

	const layout = $derived(sidebarLayout(connections.list, sidebar.folders, sidebar.filter));
	const folderNames = $derived(
		sidebarLayout(connections.list, sidebar.folders).folders.map((f) => f.name)
	);
	const filtering = $derived(sidebar.filter.trim() !== '');

	export function focusFilter() {
		filterInput?.focus();
		filterInput?.select();
	}

	async function arrange(placements: Placement[] | null) {
		if (!placements) return;
		try {
			await connections.arrange(placements);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	function move(id: string, folder: string | null, beforeId: string | null) {
		void arrange(moveConnection(connections.list, id, folder, beforeId));
	}

	async function duplicate(c: ConnectionConfig) {
		try {
			const copy = await connections.duplicate(c.id);
			toast.success(`Created “${copy.name}”`, {
				action: { label: 'Edit', onClick: () => onEdit(copy) }
			});
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	async function askFolderName(title: string, initial = ''): Promise<string | null> {
		const name = await prompt.ask({ title, label: 'Folder name', initial, confirmLabel: 'Save' });
		if (!name || name === initial) return null;
		if (folderNames.includes(name)) {
			toast.error(`A folder named “${name}” already exists`);
			return null;
		}
		return name;
	}

	async function newFolder(c?: ConnectionConfig) {
		const name = await askFolderName('New folder');
		if (!name) return;
		sidebar.addFolder(name);
		if (c) move(c.id, name, null);
	}

	async function rename(from: string) {
		const to = await askFolderName('Rename folder', from);
		if (!to) return;
		sidebar.renameFolder(from, to);
		await arrange(renameFolder(connections.list, from, to));
	}

	async function remove(name: string) {
		sidebar.removeFolder(name);
		await arrange(renameFolder(connections.list, name, null));
	}

	function onRootDragOver(e: DragEvent) {
		if (!sidebar.dragging) return;
		e.preventDefault();
		rootDrop = true;
	}

	function onRootDrop(e: DragEvent) {
		e.preventDefault();
		rootDrop = false;
		const id = sidebar.dragging;
		sidebar.dragging = null;
		if (id) move(id, null, null);
	}
</script>

{#snippet connectionNode(connection: ConnectionConfig, depth: number)}
	<ConnectionNode
		{connection}
		{depth}
		folders={folderNames}
		{onEdit}
		{onDelete}
		onDuplicate={duplicate}
		onMove={move}
		onNewFolder={newFolder}
	/>
{/snippet}

<div class="flex h-9 shrink-0 items-center justify-between border-b px-3">
	<span class="text-sm font-semibold tracking-tight">guckpg</span>
	<div class="flex items-center">
		<Button size="icon-xs" variant="ghost" title="New folder" onclick={() => newFolder()}>
			<FolderPlusIcon />
		</Button>
		<Button size="icon-xs" variant="ghost" title="New connection" onclick={() => onEdit(null)}>
			<PlusIcon />
		</Button>
	</div>
</div>

{#if connections.list.length}
	<div class="relative shrink-0 px-1.5 pt-1.5">
		<SearchIcon
			class="pointer-events-none absolute top-1/2 left-3.5 mt-[3px] size-3.5 -translate-y-1/2 text-muted-foreground"
		/>
		<input
			bind:this={filterInput}
			bind:value={sidebar.filter}
			placeholder="Filter connections"
			spellcheck="false"
			autocomplete="off"
			class="h-7 w-full rounded-md border bg-background pr-7 pl-7 text-xs outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/40"
			onkeydown={(e) => {
				if (e.key === 'Escape') {
					sidebar.filter = '';
					e.currentTarget.blur();
				}
			}}
		/>
		{#if filtering}
			<button
				class="absolute top-1/2 right-3 mt-[3px] -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:bg-accent"
				title="Clear filter"
				onclick={() => (sidebar.filter = '')}
			>
				<XIcon class="size-3" />
			</button>
		{/if}
	</div>
{/if}

<div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-1.5" role="tree">
	{#each layout.folders as folder (folder.name)}
		<FolderNode
			name={folder.name}
			count={folder.connections.length}
			forceOpen={filtering}
			onRename={rename}
			onRemove={remove}
			onDropConnection={(id, name) => move(id, name, null)}
		>
			{#each folder.connections as connection (connection.id)}
				{@render connectionNode(connection, 1)}
			{/each}
		</FolderNode>
	{/each}
	{#each layout.loose as connection (connection.id)}
		{@render connectionNode(connection, 0)}
	{/each}

	{#if !connections.list.length}
		<div
			class="flex flex-col items-center gap-3 px-4 py-10 text-center text-sm text-muted-foreground"
		>
			<p>No connections yet.</p>
			<Button size="sm" variant="outline" onclick={() => onEdit(null)}>
				<PlusIcon /> Add connection
			</Button>
		</div>
	{:else if filtering && !layout.folders.length && !layout.loose.length}
		<p class="px-3 py-6 text-center text-xs text-muted-foreground">
			Nothing matches “{sidebar.filter}”.
		</p>
	{/if}

	<!-- Room to drop a connection out of its folder. -->
	{#if sidebar.dragging}
		<div
			role="presentation"
			ondragover={onRootDragOver}
			ondragleave={() => (rootDrop = false)}
			ondrop={onRootDrop}
			class="mt-1 flex min-h-10 flex-1 items-center justify-center rounded-md border border-dashed text-xs text-muted-foreground {rootDrop
				? 'border-primary bg-primary/5 text-foreground'
				: ''}"
		>
			Drop here for no folder
		</div>
	{/if}
</div>
