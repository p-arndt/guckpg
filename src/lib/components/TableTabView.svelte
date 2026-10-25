<script lang="ts">
	import { onMount } from 'svelte';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { api, errorMessage, isAppError } from '#lib/api.js';
	import { formatCount, formatDuration } from '#lib/format.js';
	import { tabs, type TableTab } from '#lib/state/tabs.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { confirmDiscard } from '#lib/state/confirm.svelte.js';
	import ResultGrid from './ResultGrid.svelte';

	interface Props {
		tab: TableTab;
	}

	let { tab }: Props = $props();

	const result = $derived(tab.output?.results[0]);
	const connection = $derived(connections.get(tab.connectionId));
	const columnTypes = $derived(Object.fromEntries(tab.columns.map((c) => [c.name, c.dataType])));
	const primaryKeys = $derived(tab.columns.filter((c) => c.primaryKey).map((c) => c.name));

	async function load() {
		tab.running = true;
		tab.error = null;
		try {
			const out = await api.browseTable(tab.id, tab.connectionId, tab.database, {
				schema: tab.schema,
				table: tab.table,
				sort: tab.sort,
				limit: tab.limit,
				offset: tab.offset
			});
			tab.lastSql = out.sql;
			tab.output = { results: out.results, elapsedMs: out.elapsedMs };
		} catch (e) {
			tab.error = isAppError(e) ? e : { kind: 'message', message: errorMessage(e) };
		} finally {
			tab.running = false;
		}
	}

	onMount(() => {
		if (tab.output) return;
		api
			.listColumns(tab.connectionId, tab.database, tab.schema, tab.table)
			.then((cols) => (tab.columns = cols))
			.catch(() => {});
		void load();
	});

	/** Run an action that reloads the rows, unless the user keeps unsaved edits. */
	async function guarded(action: () => void) {
		if (await confirmDiscard(tab.dirty)) action();
	}

	function sortBy(column: string) {
		const s = tab.sort;
		tab.sort =
			s?.column !== column
				? { column, dir: 'asc' }
				: s.dir === 'asc'
					? { column, dir: 'desc' }
					: null;
		tab.offset = 0;
		void load();
	}

	function page(delta: number) {
		tab.offset = Math.max(0, tab.offset + delta * tab.limit);
		void load();
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-10 shrink-0 items-center gap-1.5 border-b px-2">
		<Button
			size="icon-sm"
			variant="ghost"
			disabled={tab.running}
			onclick={() => guarded(load)}
			title="Refresh"
		>
			<RefreshCwIcon class={tab.running ? 'animate-spin' : ''} />
		</Button>
		<Button
			size="icon-sm"
			variant="ghost"
			disabled={tab.running || tab.offset === 0}
			onclick={() => guarded(() => page(-1))}
			title="Previous page"
		>
			<ChevronLeftIcon />
		</Button>
		<span class="min-w-24 text-center text-xs text-muted-foreground tabular-nums">
			{#if result?.rows.length}
				{formatCount(tab.offset + 1)}–{formatCount(tab.offset + result.rows.length)}
			{:else}
				–
			{/if}
		</span>
		<Button
			size="icon-sm"
			variant="ghost"
			disabled={tab.running || (result?.rows.length ?? 0) < tab.limit}
			onclick={() => guarded(() => page(1))}
			title="Next page"
		>
			<ChevronRightIcon />
		</Button>
		<Select.Root
			type="single"
			value={String(tab.limit)}
			onValueChange={(v) =>
				guarded(() => {
					tab.limit = Number(v);
					tab.offset = 0;
					void load();
				})}
		>
			<Select.Trigger size="sm" class="h-7 text-xs">{tab.limit} / page</Select.Trigger>
			<Select.Content>
				{#each [100, 200, 500, 1000, 5000] as n (n)}
					<Select.Item value={String(n)} label={String(n)} />
				{/each}
			</Select.Content>
		</Select.Root>

		<code class="ml-2 min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground select-text">
			{tab.lastSql}
		</code>
		<Button
			size="sm"
			variant="outline"
			onclick={() => tabs.openQuery(tab.connectionId, tab.database, tab.lastSql + ';')}
		>
			<SquareTerminalIcon /> Open as query
		</Button>
	</div>

	<div class="min-h-0 flex-1">
		{#if tab.error}
			<div
				class="m-3 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive select-text"
			>
				{errorMessage(tab.error)}
			</div>
		{:else if result}
			<ResultGrid
				columns={result.columns}
				rows={result.rows}
				rowOffset={tab.offset}
				sort={tab.sort}
				onSort={(column) => guarded(() => sortBy(column))}
				{primaryKeys}
				{columnTypes}
				source={result.source}
				editTarget={connection
					? {
							connectionId: tab.connectionId,
							database: tab.database,
							readOnly: connection.readOnly
						}
					: null}
				onSaved={load}
				onDirtyChange={(n) => (tab.dirty = n)}
			/>
		{:else}
			<div class="flex h-full items-center justify-center text-muted-foreground">
				<LoaderIcon class="size-5 animate-spin" />
			</div>
		{/if}
	</div>

	<div class="flex h-7 shrink-0 items-center gap-3 border-t px-3 text-xs text-muted-foreground">
		{#if tab.output}
			<span>{formatCount(result?.rows.length ?? 0)} rows</span>
			<span>{formatDuration(tab.output.elapsedMs)}</span>
		{/if}
		<span class="ml-auto font-mono">{tab.database} · {tab.schema}.{tab.table}</span>
	</div>
</div>
