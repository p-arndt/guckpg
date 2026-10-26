<script lang="ts">
	import { onMount } from 'svelte';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { api, errorMessage, isAppError } from '#lib/api.js';
	import { formatCount, formatDuration } from '#lib/format.js';
	import { tabs, type TableTab } from '#lib/state/tabs.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { confirmDiscard } from '#lib/state/confirm.svelte.js';
	import ResultGrid from './ResultGrid.svelte';
	import TableDesigner from './TableDesigner.svelte';
	import CsvTransfer from './CsvTransfer.svelte';
	import TestDataDialog from './TestDataDialog.svelte';

	interface Props {
		tab: TableTab;
	}

	let { tab }: Props = $props();

	const result = $derived(tab.output?.results[0]);
	const connection = $derived(connections.get(tab.connectionId));
	const columnTypes = $derived(Object.fromEntries(tab.columns.map((c) => [c.name, c.dataType])));
	const primaryKeys = $derived(tab.columns.filter((c) => c.primaryKey).map((c) => c.name));
	let testDataOpen = $state(false);
	let filterColumn = $state('');
	let filterValue = $state('');
	let appliedFilter = $state<{ column: string; value: string } | null>(null);
	let gridDirty = $state(0);
	let structureDirty = $state(0);
	let structureBusy = $state(false);
	let gridVersion = $state(0);
	let designerOpened = $state(false);
	$effect(() => {
		tab.dirty = gridDirty + structureDirty;
		tab.structureBusy = structureBusy;
	});
	$effect(() => {
		if (tab.view === 'structure') designerOpened = true;
	});
	$effect(() => {
		const requested = tab.requestedView;
		if (requested && !tab.running && !structureBusy) {
			tab.requestedView = null;
			void switchView(requested);
		}
	});

	async function switchView(view: 'data' | 'structure') {
		if (tab.view === view || structureBusy || tab.running) return;
		if (view === 'structure' && gridDirty) {
			if (!(await confirmDiscard(gridDirty))) return;
			gridDirty = 0;
			gridVersion++;
		}
		tab.view = view;
	}

	async function structureApplied(name: string) {
		tab.table = name;
		tab.title = tab.schema === 'public' ? name : `${tab.schema}.${name}`;
		tab.sort = null;
		filterColumn = '';
		filterValue = '';
		appliedFilter = null;
		tab.offset = 0;
		tab.output = null;
		structureDirty = 0;
		connections.refresh(tab.connectionId);
		try {
			tab.columns = await api.listColumns(tab.connectionId, tab.database, tab.schema, name);
			await load();
		} catch (e) {
			tab.error = isAppError(e) ? e : { kind: 'message', message: errorMessage(e) };
		}
	}

	async function load() {
		tab.running = true;
		tab.error = null;
		try {
			const out = await api.browseTable(tab.id, tab.connectionId, tab.database, {
				schema: tab.schema,
				table: tab.table,
				sort: tab.sort,
				filter: appliedFilter,
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

	function applyFilter() {
		void guarded(() => {
			appliedFilter =
				filterColumn && filterValue ? { column: filterColumn, value: filterValue } : null;
			tab.offset = 0;
			void load();
		});
	}

	function page(delta: number) {
		tab.offset = Math.max(0, tab.offset + delta * tab.limit);
		void load();
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<div
		class="flex h-10 shrink-0 items-center gap-1 border-b px-3"
		role="tablist"
		aria-label="Table view"
	>
		{#each ['data', 'structure'] as view (view)}
			<Button
				role="tab"
				aria-selected={tab.view === view}
				variant={tab.view === view ? 'secondary' : 'ghost'}
				size="sm"
				disabled={structureBusy || tab.running}
				onclick={() => switchView(view as 'data' | 'structure')}
				>{view === 'data' ? 'Data' : 'Structure'}{view === 'structure' && structureDirty
					? ` (${structureDirty})`
					: ''}</Button
			>
		{/each}
	</div>
	<div
		class="min-h-0 flex-1 flex-col"
		class:flex={tab.view === 'data'}
		class:hidden={tab.view !== 'data'}
	>
		<div class="flex h-11 shrink-0 items-center gap-1 border-b px-2">
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running}
				onclick={() => guarded(load)}
				title="Refresh"
				aria-label="Refresh"
			>
				<RefreshCwIcon class={tab.running ? 'animate-spin' : ''} />
			</Button>
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running || tab.offset === 0}
				onclick={() => guarded(() => page(-1))}
				title="Previous page"
				aria-label="Previous page"
			>
				<ChevronLeftIcon />
			</Button>
			<span class="min-w-28 text-center text-xs text-muted-foreground">
				{#if result?.rows.length}
					<span class="measured text-ink-2">{formatCount(tab.offset + 1)}</span>
					to
					<span class="measured text-ink-2">{formatCount(tab.offset + result.rows.length)}</span>
				{:else}
					no rows
				{/if}
			</span>
			<Button
				size="icon-sm"
				variant="ghost"
				disabled={tab.running || (result?.rows.length ?? 0) < tab.limit}
				onclick={() => guarded(() => page(1))}
				title="Next page"
				aria-label="Next page"
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
				<Select.Trigger size="sm" class="ml-1 h-7 text-[13px]" aria-label="Rows per page"
					>{tab.limit} / page</Select.Trigger
				>
				<Select.Content>
					{#each [100, 200, 500, 1000, 5000] as n (n)}
						<Select.Item value={String(n)} label={String(n)} />
					{/each}
				</Select.Content>
			</Select.Root>

			<Button
				size="sm"
				variant="outline"
				disabled={tab.running || tab.dirty > 0 || (connection?.readOnly ?? true)}
				onclick={() => (testDataOpen = true)}>Generate test data</Button
			>
			<CsvTransfer
				connectionId={tab.connectionId}
				database={tab.database}
				schema={tab.schema}
				table={tab.table}
				readOnly={connection?.readOnly ?? true}
				disabled={tab.running || tab.dirty > 0}
				onImported={load}
			/>
			<code class="ml-3 min-w-0 flex-1 truncate text-xs text-muted-foreground select-text">
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

		<form
			class="flex shrink-0 items-center gap-2 border-b px-3 py-1"
			onsubmit={(event) => {
				event.preventDefault();
				applyFilter();
			}}
		>
			<label for="filter-column-{tab.id}" class="text-xs text-muted-foreground">Filter</label>
			<select
				id="filter-column-{tab.id}"
				class="h-8 max-w-44 rounded-md border bg-background px-2 text-sm"
				bind:value={filterColumn}
				disabled={tab.running}
			>
				<option value="">Choose column</option>
				{#each tab.columns as column (column.name)}<option value={column.name}>{column.name}</option
					>{/each}
			</select>
			<Input
				class="h-8 max-w-64"
				aria-label="Filter text"
				placeholder="Contains (case insensitive)"
				bind:value={filterValue}
				disabled={tab.running}
			/>
			<Button
				size="sm"
				variant="outline"
				type="submit"
				disabled={tab.running || (!filterColumn && !!filterValue)}>Apply</Button
			>
			{#if appliedFilter}<Button
					size="sm"
					variant="ghost"
					type="button"
					disabled={tab.running}
					onclick={() => {
						filterValue = '';
						applyFilter();
					}}>Clear</Button
				>{/if}
		</form>
		<div class="min-h-0 flex-1">
			{#if tab.error}
				<div
					class="m-3 flex max-w-3xl items-start gap-2.5 rounded-2xl border border-[color-mix(in_oklab,var(--color-bad),transparent_55%)] bg-bad-soft p-4 select-text"
					role="alert"
				>
					<CircleAlertIcon class="mt-0.5 size-4 shrink-0 text-bad" />
					<p class="min-w-0 font-medium break-words whitespace-pre-wrap">
						{errorMessage(tab.error)}
					</p>
				</div>
			{:else if result}
				{#key gridVersion}
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
						onDirtyChange={(n) => (gridDirty = n)}
					/>
				{/key}
			{:else}
				<div
					class="h-full overflow-hidden bg-background"
					aria-busy="true"
					aria-label="Loading rows"
				>
					<div class="h-[32px] border-b"></div>
					{#each { length: 12 }, i (i)}
						<div class="border-rule/50 flex h-[28px] items-center gap-6 border-b px-3">
							<span class="h-2 w-6 rounded-sm bg-hover"></span>
							<span class="h-2 rounded-sm bg-hover" style:width="{90 + ((i * 37) % 70)}px"></span>
							<span class="h-2 rounded-sm bg-hover" style:width="{140 + ((i * 53) % 90)}px"></span>
							<span class="h-2 w-20 rounded-sm bg-hover"></span>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<div
			class="flex h-8 shrink-0 items-center gap-4 border-t px-3 text-xs text-muted-foreground"
			role="status"
		>
			{#if tab.running}
				<LoaderIcon class="size-3.5 animate-spin text-cool" />
			{/if}
			{#if tab.output}
				<span
					><span class="measured text-cool">{formatCount(result?.rows.length ?? 0)}</span> rows</span
				>
				<span class="measured text-cool">{formatDuration(tab.output.elapsedMs)}</span>
			{/if}
			<span class="measured ml-auto truncate"
				>{tab.database} / <span class="text-ink-2">{tab.schema}.{tab.table}</span></span
			>
		</div>
	</div>
	{#if designerOpened}
		<div class="min-h-0 flex-1" class:hidden={tab.view !== 'structure'}>
			<TableDesigner
				connectionId={tab.connectionId}
				database={tab.database}
				schema={tab.schema}
				table={tab.table}
				onApplied={structureApplied}
				onDirtyChange={(count) => (structureDirty = count)}
				onBusyChange={(busy) => (structureBusy = busy)}
			/>
		</div>
	{/if}
</div>

<TestDataDialog
	bind:open={testDataOpen}
	connectionId={tab.connectionId}
	database={tab.database}
	schema={tab.schema}
	table={tab.table}
	readOnly={connection?.readOnly ?? true}
	onInserted={load}
/>
