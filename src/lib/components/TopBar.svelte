<script lang="ts">
	import PlugIcon from '@lucide/svelte/icons/plug';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import Table2Icon from '@lucide/svelte/icons/table-2';
	import NetworkIcon from '@lucide/svelte/icons/network';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import UnplugIcon from '@lucide/svelte/icons/unplug';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { toast } from 'svelte-sonner';
	import { errorMessage } from '#lib/api.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Separator } from '#lib/components/ui/separator/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';

	interface Props {
		onNewConnection: () => void;
		onNewQuery: () => void;
		onOpenPalette: () => void;
	}

	let { onNewConnection, onNewQuery, onOpenPalette }: Props = $props();

	/**
	 * The connection and database the toolbar acts on: the active tab's, otherwise the
	 * first connected connection. Schema only matters for the ER diagram, so table and
	 * diagram tabs pass theirs through and everything else falls back to `public`.
	 */
	const context = $derived.by(() => {
		const tab = tabs.active;
		if (tab) {
			const connection = connections.get(tab.connectionId);
			if (!connection) return null;
			return {
				connection,
				database: tab.database,
				schema: tab.kind === 'query' ? 'public' : tab.schema
			};
		}
		const connection = connections.list.find((c) => connections.status[c.id] === 'connected');
		if (!connection) return null;
		return {
			connection,
			database: connections.info[connection.id]?.database ?? connection.database,
			schema: 'public'
		};
	});

	function newTable() {
		if (!context) return;
		tabs.openQuery(context.connection.id, context.database, 'SELECT *\nFROM \nLIMIT 100;');
	}

	function newDiagram() {
		if (!context) return;
		tabs.openDiagram(context.connection.id, context.database, context.schema);
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

{#snippet toolButton(
	label: string,
	tip: string,
	icon: typeof PlugIcon,
	onclick: () => void,
	disabled: boolean
)}
	<Tooltip.Root>
		<Tooltip.Trigger>
			{#snippet child({ props })}
				{@const Icon = icon}
				<Button
					{...props}
					variant="ghost"
					class="h-auto w-14 flex-col gap-1 px-1 py-1.5 text-[11px] font-normal"
					{disabled}
					{onclick}
				>
					<Icon class="size-[18px]" />
					<span>{label}</span>
				</Button>
			{/snippet}
		</Tooltip.Trigger>
		<Tooltip.Content>{tip}</Tooltip.Content>
	</Tooltip.Root>
{/snippet}

<div class="flex h-[52px] shrink-0 items-center gap-1 border-b bg-sidebar px-2">
	{@render toolButton('Connection', 'New connection', PlugIcon, onNewConnection, false)}
	{@render toolButton('New Query', 'New query (⌘T)', SquareTerminalIcon, onNewQuery, !context)}
	{@render toolButton('Table', 'New query for a table', Table2Icon, newTable, !context)}
	{@render toolButton('ER Diagram', 'Open ER diagram', NetworkIcon, newDiagram, !context)}
	<Separator orientation="vertical" class="mx-1 h-8" />
	{@render toolButton('Refresh', 'Reload the schema tree', RefreshCwIcon, refresh, !context)}
	{@render toolButton('Disconnect', 'Close tabs and disconnect', UnplugIcon, disconnect, !context)}

	<button
		class="mx-auto flex h-7 w-full max-w-72 min-w-36 items-center gap-2 rounded-md border bg-background px-2.5 text-xs text-muted-foreground hover:border-foreground/30"
		onclick={onOpenPalette}
	>
		<SearchIcon class="size-3.5" />
		<span class="truncate">Go to table, tab, connection…</span>
		<kbd class="ml-auto font-mono text-[10px]">⌘K</kbd>
	</button>

	{#if context}
		<div
			class="flex min-w-0 items-center gap-2 rounded-md border bg-background px-2.5 py-1 text-xs"
		>
			<span
				class="size-2 shrink-0 rounded-full"
				style:background-color={context.connection.color ?? 'var(--muted-foreground)'}
			></span>
			<span class="truncate font-medium">{context.connection.name}</span>
			<span class="text-muted-foreground">·</span>
			<span class="truncate text-muted-foreground">{context.database}</span>
			{#if context.connection.readOnly}
				<Badge variant="outline">read-only</Badge>
			{/if}
		</div>
	{/if}
</div>
