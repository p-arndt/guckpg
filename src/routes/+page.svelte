<script lang="ts">
	import { onMount } from 'svelte';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import SearchIcon from '@lucide/svelte/icons/search';
	import HistoryIcon from '@lucide/svelte/icons/history';
	import { toast } from 'svelte-sonner';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Resizable from '#lib/components/ui/resizable/index.js';
	import * as AlertDialog from '#lib/components/ui/alert-dialog/index.js';
	import { errorMessage, type ConnectionConfig } from '#lib/api.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import { allowNativeContextMenu } from '#lib/context-menu.js';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import PromptDialog from '#lib/components/PromptDialog.svelte';
	import ConnectionDialog from '#lib/components/ConnectionDialog.svelte';
	import HostKeyDialog from '#lib/components/HostKeyDialog.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import TabBar from '#lib/components/TabBar.svelte';
	import TopBar from '#lib/components/TopBar.svelte';
	import QueryTabView from '#lib/components/QueryTabView.svelte';
	import TableTabView from '#lib/components/TableTabView.svelte';
	import ErDiagramView from '#lib/components/er/ErDiagramView.svelte';

	let dialogOpen = $state(false);
	let editing = $state<ConnectionConfig | null>(null);
	let deleting = $state<ConnectionConfig | null>(null);
	let restored = $state(false);
	let paletteOpen = $state(false);
	let sidebarView = $state<Sidebar | null>(null);

	onMount(async () => {
		try {
			await connections.load();
		} catch (e) {
			toast.error(errorMessage(e));
		}
		tabs.restore(new Set(connections.list.map((c) => c.id)));
		restored = true;
	});

	// Persist query tabs (debounced: this runs on every keystroke).
	$effect(() => {
		if (!restored) return;
		// Track only what is saved: snapshotting all tabs would deep-copy every
		// result grid on each keystroke.
		void tabs.persisted();
		const timer = setTimeout(() => tabs.save(), 500);
		return () => clearTimeout(timer);
	});

	function edit(c: ConnectionConfig | null) {
		editing = c;
		dialogOpen = true;
	}

	async function confirmDelete() {
		const c = deleting;
		deleting = null;
		if (!c) return;
		tabs.closeForConnection(c.id);
		try {
			await connections.remove(c.id);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}

	/** A new query for the connection of the current tab, or the first connected one. */
	function newQuery() {
		const current = tabs.active;
		if (current) {
			tabs.openQuery(current.connectionId, current.database);
			return;
		}
		const connected = connections.list.find((c) => connections.status[c.id] === 'connected');
		if (connected)
			tabs.openQuery(connected.id, connections.info[connected.id]?.database ?? connected.database);
		else toast.info('Connect to a database first');
	}

	function onContextMenu(e: MouseEvent) {
		const target = e.target instanceof Element ? e.target : null;
		const hasSelection = !!window.getSelection()?.toString();
		if (
			!allowNativeContextMenu(target, {
				hasSelection,
				shiftKey: e.shiftKey,
				dev: import.meta.env.DEV
			})
		)
			e.preventDefault();
	}

	function onKeydown(e: KeyboardEvent) {
		// Ctrl+Tab works on every platform, like in browsers.
		if (e.ctrlKey && e.key === 'Tab') {
			e.preventDefault();
			tabs.cycle(e.shiftKey ? -1 : 1);
			return;
		}
		const mod = e.metaKey || e.ctrlKey;
		if (!mod) return;
		const key = e.key.toLowerCase();
		if (key === 'k' || (key === 'p' && !e.shiftKey)) {
			e.preventDefault();
			paletteOpen = !paletteOpen;
		} else if (key === 't' && e.shiftKey) {
			e.preventDefault();
			if (!tabs.reopenClosed((id) => !!connections.get(id))) toast.info('No closed tabs');
		} else if (key === 't') {
			e.preventDefault();
			newQuery();
		} else if (key === 'w' && tabs.activeId) {
			e.preventDefault();
			void tabs.requestClose(tabs.activeId);
		} else if (/^[1-9]$/.test(e.key) && !e.shiftKey && !e.altKey) {
			e.preventDefault();
			// ⌘9 is always the last tab, as in browsers.
			tabs.activateIndex(e.key === '9' ? -1 : Number(e.key) - 1);
		} else if (e.altKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
			e.preventDefault();
			tabs.cycle(e.key === 'ArrowRight' ? 1 : -1);
		} else if (key === 'f' && e.shiftKey) {
			e.preventDefault();
			sidebarView?.focusFilter();
		}
	}
</script>

<svelte:window onkeydown={onKeydown} oncontextmenu={onContextMenu} />

<div class="flex h-screen w-screen flex-col overflow-hidden">
	<TopBar
		onNewConnection={() => edit(null)}
		onNewQuery={newQuery}
		onOpenPalette={() => (paletteOpen = true)}
	/>
	<Resizable.PaneGroup direction="horizontal" class="min-h-0 flex-1">
		<Resizable.Pane defaultSize={20} minSize={12} maxSize={45} class="flex flex-col bg-sidebar">
			<Sidebar bind:this={sidebarView} onEdit={edit} onDelete={(c) => (deleting = c)} />
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={80} class="flex min-w-0 flex-col">
			{#if tabs.items.length}
				<TabBar onNewQuery={newQuery} />
				<div class="relative min-h-0 flex-1">
					<!-- Inactive tabs stay mounted so editors keep undo history and scroll position. -->
					{#each tabs.items as tab (tab.id)}
						<div class="absolute inset-0" class:hidden={tab.id !== tabs.activeId}>
							{#if tab.kind === 'query'}
								<QueryTabView {tab} />
							{:else if tab.kind === 'table'}
								<TableTabView {tab} />
							{:else}
								<ErDiagramView {tab} />
							{/if}
						</div>
					{/each}
				</div>
			{:else}
				<div
					class="flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground"
				>
					<p>Open a connection on the left, then double-click a table or start a query.</p>
					<div class="flex gap-2">
						<Button size="sm" variant="outline" onclick={newQuery}>
							<SquareTerminalIcon /> New query <kbd class="ml-1 font-mono text-xs">⌘T</kbd>
						</Button>
						<Button size="sm" variant="outline" onclick={() => (paletteOpen = true)}>
							<SearchIcon /> Go to… <kbd class="ml-1 font-mono text-xs">⌘K</kbd>
						</Button>
					</div>
					{#if tabs.closed.length}
						<Button
							size="sm"
							variant="ghost"
							onclick={() => tabs.reopenClosed((id) => !!connections.get(id))}
						>
							<HistoryIcon /> Reopen closed tab <kbd class="ml-1 font-mono text-xs">⌘⇧T</kbd>
						</Button>
					{/if}
				</div>
			{/if}
		</Resizable.Pane>
	</Resizable.PaneGroup>
</div>

<ConnectionDialog bind:open={dialogOpen} {editing} />
<HostKeyDialog />
<ConfirmDialog />
<PromptDialog />
<CommandPalette
	bind:open={paletteOpen}
	onNewQuery={newQuery}
	onNewConnection={() => edit(null)}
	onFilterConnections={() => sidebarView?.focusFilter()}
/>

<AlertDialog.Root
	open={deleting !== null}
	onOpenChange={(open) => {
		if (!open) deleting = null;
	}}
>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>Delete “{deleting?.name}”?</AlertDialog.Title>
			<AlertDialog.Description>
				Removes the connection and its saved passwords from the keychain. The database itself is not
				touched.
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action variant="destructive" onclick={confirmDelete}>Delete</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
