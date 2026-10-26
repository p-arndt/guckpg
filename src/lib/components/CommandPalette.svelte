<script lang="ts">
	import { untrack } from 'svelte';
	import SearchIcon from '@lucide/svelte/icons/search';
	import TableIcon from '@lucide/svelte/icons/table-2';
	import ServerIcon from '@lucide/svelte/icons/server';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import NetworkIcon from '@lucide/svelte/icons/network';
	import UserIcon from '@lucide/svelte/icons/user';
	import PlugIcon from '@lucide/svelte/icons/plug';
	import HistoryIcon from '@lucide/svelte/icons/history';
	import SunMoonIcon from '@lucide/svelte/icons/sun-moon';
	import XIcon from '@lucide/svelte/icons/x';
	import ListFilterIcon from '@lucide/svelte/icons/list-filter';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { setMode, userPrefersMode } from 'mode-watcher';
	import { toast } from 'svelte-sonner';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { api, errorMessage, type CompletionTable } from '#lib/api.js';
	import { fuzzyFilter } from '#lib/fuzzy.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { sidebar } from '#lib/state/sidebar.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import { groupByKind } from '#lib/palette.js';

	interface Props {
		open: boolean;
		onNewQuery: () => void;
		onNewConnection: () => void;
		onFilterConnections: () => void;
	}

	let { open = $bindable(), onNewQuery, onNewConnection, onFilterConnections }: Props = $props();

	interface Item {
		key: string;
		label: string;
		detail?: string;
		kind: string;
		/** What Enter does, shown on the selected row. */
		hint?: string;
		icon: typeof TableIcon;
		color?: string | null;
		run: () => void | Promise<void>;
	}

	const MAX_ITEMS = 60;

	let query = $state('');
	let selected = $state(0);
	let list = $state<HTMLElement | null>(null);
	let loading = $state(0);
	/** Tables per connection, database and tree revision, so a refresh reloads them. */
	let tables = $state<Record<string, CompletionTable[]>>({});

	type Target = { connectionId: string; database: string; key: string };

	/** Every database the user is working in: each connection's default plus those of open tabs. */
	const targets = $derived.by(() => {
		const seen: Record<string, Target> = {};
		const add = (connectionId: string, database: string) => {
			if (connections.status[connectionId] !== 'connected') return;
			const key = `${connectionId}\u0000${database}\u0000${connections.revision[connectionId] ?? 0}`;
			seen[key] ??= { connectionId, database, key };
		};
		for (const c of connections.list) add(c.id, connections.info[c.id]?.database ?? c.database);
		for (const t of tabs.items) add(t.connectionId, t.database);
		return Object.values(seen);
	});

	$effect(() => {
		if (open) query = '';
	});

	/** Keys being fetched; not state, nothing renders from it. */
	// eslint-disable-next-line svelte/prefer-svelte-reactivity
	const fetching = new Set<string>();

	$effect(() => {
		if (!open) return;
		const wanted = targets;
		untrack(() => {
			for (const t of wanted) {
				if (tables[t.key] || fetching.has(t.key)) continue;
				fetching.add(t.key);
				loading++;
				api
					.completionTables(t.connectionId, t.database)
					.then(
						(list) => (tables[t.key] = list),
						() => (tables[t.key] = [])
					)
					.finally(() => {
						fetching.delete(t.key);
						loading--;
					});
			}
		});
	});

	function close() {
		open = false;
	}

	const items = $derived.by(() => {
		const all: Item[] = [];
		const multipleTargets = targets.length > 1;

		for (const t of tabs.items) {
			all.push({
				key: `tab:${t.id}`,
				label: t.title,
				detail: `${connections.get(t.connectionId)?.name ?? ''} / ${t.database}`,
				kind: 'Tab',
				hint: 'Switch',
				icon:
					t.kind === 'table'
						? TableIcon
						: t.kind === 'diagram'
							? NetworkIcon
							: t.kind === 'role'
								? UserIcon
								: SquareTerminalIcon,
				color: connections.get(t.connectionId)?.color,
				run: () => {
					tabs.activeId = t.id;
				}
			});
		}

		for (const target of targets) {
			const connection = connections.get(target.connectionId);
			for (const table of tables[target.key] ?? []) {
				const name = table.schema === 'public' ? table.name : `${table.schema}.${table.name}`;
				all.push({
					key: `table:${target.key}:${table.schema}.${table.name}`,
					label: name,
					detail: multipleTargets ? `${connection?.name} / ${target.database}` : target.database,
					kind: 'Table',
					hint: 'Open',
					icon: TableIcon,
					color: connection?.color,
					run: () => {
						tabs.openTable(target.connectionId, target.database, table.schema, table.name);
					}
				});
			}
			const schemas = new Set((tables[target.key] ?? []).map((t) => t.schema));
			for (const schema of schemas) {
				all.push({
					key: `diagram:${target.key}:${schema}`,
					label: `ER diagram: ${schema}`,
					detail: multipleTargets ? `${connection?.name} / ${target.database}` : target.database,
					kind: 'Diagram',
					hint: 'Open',
					icon: NetworkIcon,
					color: connection?.color,
					run: () => tabs.openDiagram(target.connectionId, target.database, schema)
				});
			}
		}

		for (const c of connections.list) {
			const connected = connections.status[c.id] === 'connected';
			all.push({
				key: `connection:${c.id}`,
				label: c.name,
				detail: `${c.folder ? `${c.folder} / ` : ''}${c.host}`,
				kind: 'Connection',
				hint: connected ? 'New query' : 'Connect',
				icon: ServerIcon,
				color: c.color,
				run: async () => {
					if (connected) {
						tabs.openQuery(c.id, connections.info[c.id]?.database ?? c.database);
						return;
					}
					try {
						await connections.connect(c.id);
						sidebar.setOpen(c.id, true);
						toast.success(`Connected to ${c.name}`);
					} catch (e) {
						toast.error(`${c.name}: ${errorMessage(e)}`);
					}
				}
			});
		}

		const action = (key: string, label: string, icon: typeof TableIcon, run: () => void) =>
			all.push({ key: `action:${key}`, label, kind: 'Action', hint: 'Run', icon, run });
		action('new-query', 'New query', SquareTerminalIcon, onNewQuery);
		action('new-connection', 'New connection', PlugIcon, onNewConnection);
		action('filter', 'Filter connections', ListFilterIcon, onFilterConnections);
		if (tabs.closed.length) {
			action('reopen', 'Reopen closed tab', HistoryIcon, () =>
				tabs.reopenClosed((id) => !!connections.get(id))
			);
		}
		if (tabs.items.length) action('close-all', 'Close all tabs', XIcon, () => tabs.closeAll());
		const nextMode = { light: 'dark', dark: 'system', system: 'light' } as const;
		const mode = nextMode[userPrefersMode.current ?? 'system'];
		action('theme', `Theme: switch to ${mode}`, SunMoonIcon, () => setMode(mode));
		return all;
	});

	const results = $derived(
		groupByKind(
			query.trim()
				? fuzzyFilter(items, query, (i) => `${i.label} ${i.detail ?? ''}`).slice(0, MAX_ITEMS)
				: // Without a query: tabs, connections and actions first, tables are a search away.
					[
						...items.filter((i) => i.kind !== 'Table' && i.kind !== 'Diagram'),
						...items.filter((i) => i.kind === 'Table')
					].slice(0, MAX_ITEMS)
		)
	);

	const GROUP_TITLES: Record<string, string> = {
		Tab: 'Open tabs',
		Table: 'Tables',
		Diagram: 'Diagrams',
		Connection: 'Connections',
		Action: 'Actions'
	};

	$effect(() => {
		void query;
		selected = 0;
	});

	$effect(() => {
		list?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: 'nearest' });
	});

	function choose(item: Item | undefined) {
		if (!item) return;
		close();
		void item.run();
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) {
			e.preventDefault();
			selected = Math.min(selected + 1, results.length - 1);
		} else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) {
			e.preventDefault();
			selected = Math.max(selected - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			choose(results[selected]);
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content
		class="palette top-[15%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
		showCloseButton={false}
	>
		<Dialog.Title class="sr-only">Command palette</Dialog.Title>
		<div class="flex items-center gap-3 border-b px-4">
			<SearchIcon class="size-4 shrink-0 text-muted-foreground" />
			<!-- svelte-ignore a11y_autofocus -->
			<input
				bind:value={query}
				autofocus
				data-palette-input
				aria-label="Search"
				placeholder="Search tables, tabs, connections…"
				spellcheck="false"
				autocomplete="off"
				class="h-12 min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-muted-foreground"
				onkeydown={onKeydown}
			/>
			{#if loading > 0}
				<LoaderIcon class="size-4 shrink-0 animate-spin text-muted-foreground" />
			{/if}
		</div>
		<div
			bind:this={list}
			class="max-h-[min(60vh,26rem)] scroll-py-8 overflow-y-auto px-1.5 pb-1.5"
			role="listbox"
			aria-label="Results"
		>
			{#each results as item, index (item.key)}
				{@const Icon = item.icon}
				{@const active = index === selected}
				{#if index === 0 || results[index - 1].kind !== item.kind}
					<div
						class="px-2.5 pt-3 pb-1 text-xs font-medium text-muted-foreground"
						role="presentation"
					>
						{GROUP_TITLES[item.kind] ?? item.kind}
					</div>
				{/if}
				<button
					type="button"
					role="option"
					aria-selected={active}
					data-index={index}
					class="palette-item flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] {active
						? 'bg-hover text-foreground'
						: 'text-ink-2'}"
					onmousemove={() => (selected = index)}
					onclick={() => choose(item)}
				>
					<Icon
						class="size-4 shrink-0 text-muted-foreground"
						style={item.color ? `color: ${item.color}` : ''}
					/>
					<span class="min-w-0 shrink truncate">{item.label}</span>
					{#if item.detail}
						<span class="min-w-0 flex-1 truncate text-xs text-muted-foreground">{item.detail}</span>
					{:else}
						<span class="flex-1"></span>
					{/if}
					{#if active && item.hint}
						<span class="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
							{item.hint}<kbd class="key">↵</kbd>
						</span>
					{/if}
				</button>
			{:else}
				<p class="px-3 py-10 text-center text-[13px] text-muted-foreground">
					{loading > 0 ? 'Loading tables…' : `Nothing matches “${query}”.`}
				</p>
			{/each}
		</div>
		<div class="flex items-center gap-4 border-t px-4 py-2 text-xs text-muted-foreground">
			<span class="flex items-center gap-1.5"
				><kbd class="key">↑</kbd><kbd class="key">↓</kbd> navigate</span
			>
			<span class="flex items-center gap-1.5"><kbd class="key">↵</kbd> open</span>
			<span class="flex items-center gap-1.5"><kbd class="key">esc</kbd> close</span>
		</div>
	</Dialog.Content>
</Dialog.Root>
