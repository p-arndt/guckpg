<script lang="ts">
	import ServerIcon from '@lucide/svelte/icons/server';
	import LockIcon from '@lucide/svelte/icons/lock';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage, type ConnectionConfig } from '#lib/api.js';
	import { connectionUrl } from '#lib/sidebar.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { sidebar } from '#lib/state/sidebar.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TreeRow from './TreeRow.svelte';
	import DatabaseNode from './DatabaseNode.svelte';
	import RolesNode from './RolesNode.svelte';

	interface Props {
		connection: ConnectionConfig;
		depth?: number;
		/** Every folder name, for "Move to folder". */
		folders: string[];
		onEdit: (c: ConnectionConfig) => void;
		onDelete: (c: ConnectionConfig) => void;
		onDuplicate: (c: ConnectionConfig) => void;
		/** Place the dragged connection; `beforeId` null appends to `folder`. */
		onMove: (id: string, folder: string | null, beforeId: string | null) => void;
		onNewFolder: (c: ConnectionConfig) => void;
	}

	let {
		connection,
		depth = 0,
		folders,
		onEdit,
		onDelete,
		onDuplicate,
		onMove,
		onNewFolder
	}: Props = $props();

	let databases = $state<string[] | null>(null);
	let dropTarget = $state(false);

	const status = $derived(connections.status[connection.id] ?? 'idle');
	const info = $derived(connections.info[connection.id]);
	const expanded = $derived(sidebar.isOpen(connection.id));

	async function loadDatabases() {
		try {
			databases = await api.listDatabases(connection.id);
		} catch (e) {
			sidebar.setOpen(connection.id, false);
			toast.error(`${connection.name}: ${errorMessage(e)}`);
		}
	}

	async function open() {
		try {
			if (status !== 'connected') await connections.connect(connection.id);
			await loadDatabases();
			sidebar.setOpen(connection.id, true);
		} catch (e) {
			sidebar.setOpen(connection.id, false);
			toast.error(`${connection.name}: ${errorMessage(e)}`);
		}
	}

	// Opened from elsewhere (command palette) or remounted in another folder.
	$effect(() => {
		if (expanded && status === 'connected' && databases === null) void loadDatabases();
	});

	async function toggle() {
		if (expanded) sidebar.setOpen(connection.id, false);
		else await open();
	}

	async function disconnect() {
		tabs.closeForConnection(connection.id);
		await connections.disconnect(connection.id);
		sidebar.setOpen(connection.id, false);
		databases = null;
	}

	function refresh() {
		connections.refresh(connection.id);
		void open();
	}

	async function copyUrl() {
		await navigator.clipboard.writeText(connectionUrl(connection));
		toast.success('Copied connection URL (without password)');
	}

	function onDragStart(e: DragEvent) {
		sidebar.dragging = connection.id;
		e.dataTransfer?.setData('text/plain', connection.name);
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
	}

	function onDragOver(e: DragEvent) {
		if (!sidebar.dragging || sidebar.dragging === connection.id) return;
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
		if (id && id !== connection.id) onMove(id, connection.folder, connection.id);
	}

	// The default database first: it is where people usually work.
	const ordered = $derived([
		...(databases ?? []).filter((d) => d === connection.database),
		...(databases ?? []).filter((d) => d !== connection.database)
	]);
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<div class="relative">
				{#if dropTarget}
					<div
						class="pointer-events-none absolute inset-x-1 -top-px z-10 h-0.5 rounded-full bg-signal"
					></div>
				{/if}
				<TreeRow
					{...props}
					{depth}
					{expanded}
					loading={status === 'connecting'}
					label={connection.name}
					onclick={toggle}
					draggable="true"
					ondragstart={onDragStart}
					ondragend={() => (sidebar.dragging = null)}
					ondragover={onDragOver}
					ondragleave={() => (dropTarget = false)}
					ondrop={onDrop}
					class={sidebar.dragging === connection.id ? 'opacity-50' : ''}
				>
					{#snippet icon()}
						<span class="relative">
							<ServerIcon
								class="size-3.5"
								style={connection.color ? `color: ${connection.color}` : ''}
							/>
							{#if status === 'connected'}
								<span
									class="absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full bg-ok ring-2 ring-sidebar"
								></span>
							{:else if status === 'error'}
								<span
									class="absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full bg-bad ring-2 ring-sidebar"
								></span>
							{/if}
						</span>
					{/snippet}
					{#snippet trailing()}
						{#if connection.readOnly}
							<LockIcon class="size-3 text-muted-foreground" />
						{/if}
						{#if connection.ssh}
							<span class="text-[11px] text-muted-foreground" title="Connects through an SSH tunnel"
								>ssh</span
							>
						{/if}
					{/snippet}
				</TreeRow>
			</div>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-56">
		{#if status === 'connected'}
			<ContextMenu.Item onSelect={() => tabs.openQuery(connection.id, connection.database)}>
				New query
			</ContextMenu.Item>
			<ContextMenu.Item onSelect={refresh}>Refresh</ContextMenu.Item>
			<ContextMenu.Item onSelect={disconnect}>Disconnect</ContextMenu.Item>
		{:else}
			<ContextMenu.Item onSelect={open}>Connect</ContextMenu.Item>
		{/if}
		<ContextMenu.Separator />
		<ContextMenu.Item onSelect={() => onEdit(connection)}>Edit…</ContextMenu.Item>
		<ContextMenu.Item onSelect={() => onDuplicate(connection)}>Duplicate</ContextMenu.Item>
		<ContextMenu.Sub>
			<ContextMenu.SubTrigger>Move to folder</ContextMenu.SubTrigger>
			<ContextMenu.SubContent class="w-48">
				{#each folders as folder (folder)}
					<ContextMenu.Item
						disabled={folder === connection.folder}
						onSelect={() => onMove(connection.id, folder, null)}
					>
						{folder}
					</ContextMenu.Item>
				{/each}
				{#if connection.folder}
					<ContextMenu.Item onSelect={() => onMove(connection.id, null, null)}>
						No folder
					</ContextMenu.Item>
				{/if}
				{#if folders.length || connection.folder}<ContextMenu.Separator />{/if}
				<ContextMenu.Item onSelect={() => onNewFolder(connection)}>New folder…</ContextMenu.Item>
			</ContextMenu.SubContent>
		</ContextMenu.Sub>
		<ContextMenu.Item onSelect={copyUrl}>Copy URL</ContextMenu.Item>
		<ContextMenu.Separator />
		<ContextMenu.Item variant="destructive" onSelect={() => onDelete(connection)}>
			Delete…
		</ContextMenu.Item>
		{#if info}
			<ContextMenu.Separator />
			<ContextMenu.Label class="text-xs font-normal text-muted-foreground">
				PostgreSQL {info.version}
			</ContextMenu.Label>
		{/if}
	</ContextMenu.Content>
</ContextMenu.Root>

{#if expanded}
	<RolesNode connectionId={connection.id} readOnly={connection.readOnly} depth={depth + 1} />
	{#each ordered as database (database)}
		<DatabaseNode
			connectionId={connection.id}
			{database}
			depth={depth + 1}
			initiallyExpanded={database === connection.database}
		/>
	{/each}
{/if}
