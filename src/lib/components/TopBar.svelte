<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import BackupDialog from './BackupDialog.svelte';
	import AppearanceSettings from './AppearanceSettings.svelte';
	import SchemaCompare from './SchemaCompare.svelte';
	import SessionMonitor from './SessionMonitor.svelte';
	import BotIcon from '@lucide/svelte/icons/bot';
	import ServerIcon from '@lucide/svelte/icons/server';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import NetworkIcon from '@lucide/svelte/icons/network';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import UnplugIcon from '@lucide/svelte/icons/unplug';
	import SearchIcon from '@lucide/svelte/icons/search';
	import LockIcon from '@lucide/svelte/icons/lock';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage } from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';

	interface Props {
		onNewQuery: () => void;
		onOpenPalette: () => void;
		onToggleAi: () => void;
		aiOpen: boolean;
	}

	let { onNewQuery, onOpenPalette, onToggleAi, aiOpen }: Props = $props();

	/** Tauri on macOS draws the traffic lights over the page (`titleBarStyle: Overlay`). */
	const overlayTitleBar =
		typeof window !== 'undefined' &&
		'__TAURI_INTERNALS__' in window &&
		/Mac/.test(navigator.userAgent);

	/** The connection and database the toolbar acts on: the active tab's, otherwise the first connected one. */
	const context = $derived.by(() => {
		const tab = tabs.active;
		if (tab) {
			const connection = connections.get(tab.connectionId);
			return connection ? { connection, database: tab.database } : null;
		}
		const connection = connections.list.find((c) => connections.status[c.id] === 'connected');
		if (!connection) return null;
		return {
			connection,
			database: connections.info[connection.id]?.database ?? connection.database
		};
	});

	/** Schemas of the context database, loaded when the menu opens; null while loading. */
	let backupOpen = $state(false);
	let compareOpen = $state(false);
	let sessionsOpen = $state(false);
	let schemas = $state<string[] | null>(null);

	async function loadSchemas() {
		if (!context) return;
		schemas = null;
		try {
			schemas = await api.listSchemas(context.connection.id, context.database);
		} catch (e) {
			schemas = [];
			toast.error(errorMessage(e));
		}
	}

	function openDiagram(schema: string) {
		if (!context) return;
		tabs.openDiagram(context.connection.id, context.database, schema);
	}

	function refresh() {
		if (!context) return;
		connections.refresh(context.connection.id);
	}

	async function disconnect() {
		if (!context) return;
		const id = context.connection.id;
		tabs.closeForConnection(id);
		try {
			await connections.disconnect(id);
		} catch (e) {
			toast.error(errorMessage(e));
		}
	}
</script>

<!-- The window has no native title bar on macOS: this bar is the drag region and the
traffic lights sit in its left inset. Only the bar itself drags, never its controls. -->
<header
	class="flex h-12 shrink-0 items-center gap-3 pr-2 {overlayTitleBar ? 'pl-[84px]' : 'pl-3'}"
	data-tauri-drag-region
>
	<span class="text-[15px] font-semibold tracking-[-0.02em]" data-tauri-drag-region>guckpg</span>

	<div class="flex min-w-0 flex-1 basis-0 items-center gap-2 text-sm" data-tauri-drag-region>
		{#if context}
			<DropdownMenu.Root onOpenChange={(open) => open && loadSchemas()}>
				<DropdownMenu.Trigger>
					{#snippet child({ props })}
						<button
							{...props}
							type="button"
							class="flex h-8 min-w-0 items-center gap-2 rounded-lg px-2 transition-[background-color] duration-(--dur-micro) hover:bg-hover data-[state=open]:bg-hover"
							title="Connection actions"
						>
							<ServerIcon
								class="size-3.5 shrink-0 text-muted-foreground"
								style={context.connection.color ? `color: ${context.connection.color}` : ''}
							/>
							<span class="truncate font-medium">{context.connection.name}</span>
							<ChevronDownIcon class="size-3.5 shrink-0 text-muted-foreground" />
						</button>
					{/snippet}
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="start" class="w-56">
					<DropdownMenu.Item onSelect={onNewQuery}>
						<SquareTerminalIcon /> New query
						<DropdownMenu.Shortcut>⌘T</DropdownMenu.Shortcut>
					</DropdownMenu.Item>
					<DropdownMenu.Sub>
						<DropdownMenu.SubTrigger>
							<NetworkIcon /> ER diagram
						</DropdownMenu.SubTrigger>
						<DropdownMenu.SubContent class="max-h-80 w-48 overflow-y-auto">
							<DropdownMenu.Label class="text-xs font-normal text-muted-foreground">
								Schema in {context.database}
							</DropdownMenu.Label>
							{#if schemas === null}
								<DropdownMenu.Item disabled>Loading…</DropdownMenu.Item>
							{:else}
								{#each schemas as schema (schema)}
									<DropdownMenu.Item onSelect={() => openDiagram(schema)}
										>{schema}</DropdownMenu.Item
									>
								{:else}
									<DropdownMenu.Item disabled>No schemas</DropdownMenu.Item>
								{/each}
							{/if}
						</DropdownMenu.SubContent>
					</DropdownMenu.Sub>
					<DropdownMenu.Item onSelect={() => (backupOpen = true)}
						>Backup and restore</DropdownMenu.Item
					>
					<DropdownMenu.Item onSelect={() => (compareOpen = true)}
						>Compare schemas</DropdownMenu.Item
					>
					<DropdownMenu.Item onSelect={() => (sessionsOpen = true)}>Sessions</DropdownMenu.Item>
					<DropdownMenu.Separator />
					<DropdownMenu.Item onSelect={refresh}>
						<RefreshCwIcon /> Reload schema
					</DropdownMenu.Item>
					<DropdownMenu.Item onSelect={disconnect}>
						<UnplugIcon /> Disconnect
					</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Root>
			{#if context.connection.readOnly}
				<span class="chip"><LockIcon class="size-3" />read-only</span>
			{/if}
		{:else}
			<span class="text-muted-foreground">Not connected</span>
		{/if}
	</div>

	<button
		type="button"
		class="flex h-8 w-full max-w-md min-w-40 flex-[2] basis-0 items-center gap-2 rounded-lg border bg-background px-2.5 text-[13px] text-muted-foreground shadow-panel transition-[border-color,color] duration-(--dur-micro) hover:border-rule-2 hover:text-foreground"
		onclick={onOpenPalette}
	>
		<SearchIcon class="size-3.5 shrink-0" />
		<span class="truncate">Go to table, tab, connection…</span>
		<kbd class="key ml-auto">⌘K</kbd>
	</button>

	<div class="flex flex-1 basis-0 items-center justify-end gap-1" data-tauri-drag-region>
		<Button
			size="sm"
			variant="outline"
			disabled={!context}
			onclick={onNewQuery}
			title="New query (⌘T)"
		>
			<SquareTerminalIcon /> New query
		</Button>
		<Tooltip.Root>
			<Tooltip.Trigger>
				{#snippet child({ props })}
					<Button
						{...props}
						size="sm"
						variant={aiOpen ? 'secondary' : 'ghost'}
						aria-pressed={aiOpen}
						onclick={onToggleAi}
					>
						<BotIcon /> AI
					</Button>
				{/snippet}
			</Tooltip.Trigger>
			<Tooltip.Content>AI assistant</Tooltip.Content>
		</Tooltip.Root>
		<AppearanceSettings />
	</div>
</header>

{#if context}
	<BackupDialog bind:open={backupOpen} id={context.connection.id} database={context.database} />
	<SchemaCompare
		bind:open={compareOpen}
		sourceId={context.connection.id}
		sourceDatabase={context.database}
	/>
	<Dialog.Root bind:open={sessionsOpen}
		><Dialog.Content class="h-[70vh] sm:max-w-5xl"
			><Dialog.Header
				><Dialog.Title>Server sessions</Dialog.Title><Dialog.Description
					>Current database activity.</Dialog.Description
				></Dialog.Header
			>{#if sessionsOpen}<SessionMonitor
					connectionId={context.connection.id}
					database={context.database}
				/>{/if}</Dialog.Content
		></Dialog.Root
	>
{/if}
