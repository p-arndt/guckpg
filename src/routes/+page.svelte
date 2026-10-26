<script lang="ts">
	import { onMount } from 'svelte';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import SearchIcon from '@lucide/svelte/icons/search';
	import HistoryIcon from '@lucide/svelte/icons/history';
	import { toast } from 'svelte-sonner';
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
	import AiPanel from '#lib/components/AiPanel.svelte';
	import TopBar from '#lib/components/TopBar.svelte';
	import QueryTabView from '#lib/components/QueryTabView.svelte';
	import TableTabView from '#lib/components/TableTabView.svelte';
	import ErDiagramView from '#lib/components/er/ErDiagramView.svelte';
	import RoleTabView from '#lib/components/RoleTabView.svelte';
	import BackdropArt from '#lib/components/BackdropArt.svelte';
	import { appearance } from '#lib/state/appearance.svelte.js';

	$effect(() => appearance.apply(document.documentElement));

	let aiOpen = $state(false);
	let aiPane = $state<ReturnType<typeof Resizable.Pane> | null>(null);
	let aiSize = 26;

	$effect(() => {
		if (!aiPane) return;
		if (aiOpen && aiPane.isCollapsed()) aiPane.resize(aiSize);
		else if (!aiOpen && !aiPane.isCollapsed()) aiPane.collapse();
	});
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
		if (!(await tabs.requestCloseForConnection(c.id))) return;
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

<div class="relative isolate flex h-screen w-screen flex-col overflow-hidden bg-chrome">
	{#if appearance.background !== 'none'}
		<!-- The art twice: crisp behind the title bar and sidebar, and huge and blurred behind the
		     whole window, so its light runs on under the panels instead of stopping at an edge. -->
		<div class="backdrop pointer-events-none absolute inset-0 -z-10" aria-hidden="true">
			<BackdropArt
				variant={appearance.background}
				class="ambient-art absolute top-0 left-0 h-[640px] w-[1100px]"
			/>
			<div class="ambient-glow absolute -right-16 -bottom-52 h-[600px] w-[900px]"></div>
			<BackdropArt
				variant={appearance.background}
				class="header-art absolute top-0 left-0 h-[440px] max-h-[65%] w-[560px]"
			/>
		</div>
	{/if}
	<TopBar
		onNewQuery={newQuery}
		onOpenPalette={() => (paletteOpen = true)}
		onToggleAi={() => (aiOpen = !aiOpen)}
		{aiOpen}
	/>
	<div class="flex min-h-0 flex-1 pr-2 pb-2">
		<Resizable.PaneGroup direction="horizontal" autoSaveId="guckpg.layout" class="min-h-0 flex-1">
			<Resizable.Pane
				id="sidebar"
				order={1}
				defaultSize={20}
				minSize={12}
				maxSize={45}
				class="flex flex-col"
			>
				<Sidebar bind:this={sidebarView} onEdit={edit} onDelete={(c) => (deleting = c)} />
			</Resizable.Pane>
			<Resizable.Handle variant="gap" />
			<Resizable.Pane
				id="main"
				order={2}
				defaultSize={80}
				minSize={30}
				class="glass-card flex min-w-0 flex-col rounded-xl border bg-background shadow-panel"
			>
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
								{:else if tab.kind === 'role'}
									<RoleTabView {tab} />
								{:else}
									<ErDiagramView {tab} />
								{/if}
							</div>
						{/each}
					</div>
				{:else}
					<div class="flex h-full flex-col items-center justify-center px-10 pb-[10vh]">
						<div class="flex w-full max-w-sm flex-col gap-5">
							<div class="flex flex-col gap-1.5">
								<h1 class="text-xl font-medium tracking-[-0.015em]">Nothing open yet</h1>
								<p class="text-ink-2">
									Pick a connection on the left, then double-click a table or start a query.
								</p>
							</div>
							<div class="flex flex-col">
								{#snippet action(
									label: string,
									icon: typeof SearchIcon,
									shortcut: string,
									onclick: () => void
								)}
									{@const Icon = icon}
									<button
										type="button"
										class="-mx-2 flex h-10 items-center gap-3 rounded-md px-2 text-left text-ink-2 transition-[background-color,color] duration-(--dur-micro) hover:bg-hover hover:text-foreground"
										{onclick}
									>
										<Icon class="size-4 text-muted-foreground" />
										<span>{label}</span>
										<kbd class="key ml-auto">{shortcut}</kbd>
									</button>
								{/snippet}
								{@render action('New query', SquareTerminalIcon, '⌘T', newQuery)}
								{@render action('Go to…', SearchIcon, '⌘K', () => (paletteOpen = true))}
								{#if tabs.closed.length}
									{@render action('Reopen closed tab', HistoryIcon, '⌘⇧T', () =>
										tabs.reopenClosed((id) => !!connections.get(id))
									)}
								{/if}
							</div>
						</div>
					</div>
				{/if}
			</Resizable.Pane>
			<Resizable.Handle variant="gap" disabled={!aiOpen} class={aiOpen ? '' : 'hidden'} />
			<!-- Collapsed rather than unmounted, so the chat keeps tracking the active database. -->
			<Resizable.Pane
				bind:this={aiPane}
				id="ai"
				order={3}
				defaultSize={0}
				collapsible
				collapsedSize={0}
				minSize={18}
				maxSize={55}
				class="min-w-0"
				onResize={(size) => {
					if (size > 0) aiSize = size;
				}}
				onCollapse={() => (aiOpen = false)}
			>
				<AiPanel open={aiOpen} onClose={() => (aiOpen = false)} />
			</Resizable.Pane>
		</Resizable.PaneGroup>
	</div>
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
