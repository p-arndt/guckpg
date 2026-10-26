<script lang="ts">
	import { SvelteFlowProvider } from '@xyflow/svelte';
	import SearchIcon from '@lucide/svelte/icons/search';
	import PlayIcon from '@lucide/svelte/icons/play';
	import SquarePenIcon from '@lucide/svelte/icons/square-pen';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import EraserIcon from '@lucide/svelte/icons/eraser';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import { toast } from 'svelte-sonner';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Resizable from '#lib/components/ui/resizable/index.js';
	import { api, errorMessage, type CompletionTable, type Diagram } from '#lib/api.js';
	import { highlightSql } from '#lib/highlight.js';
	import {
		addTable,
		buildSql,
		emptyModel,
		foreignKeyJoins,
		type BuilderModel,
		type BuilderTable
	} from '#lib/query-builder.js';
	import { kindStyle } from '../tree/kinds.js';
	import BuilderCanvas, { TABLE_DRAG_TYPE } from './BuilderCanvas.svelte';
	import ClausePanel from './ClausePanel.svelte';
	import type { BuilderNodeColumn } from './BuilderTableNode.svelte';

	interface Props {
		model: BuilderModel;
		connectionId: string;
		database: string;
		tables: CompletionTable[];
		running: boolean;
		onrun: (sql: string) => void;
		onopen: (sql: string) => void;
	}

	let { model, connectionId, database, tables, running, onrun, onopen }: Props = $props();

	let search = $state('');
	let canvas = $state<BuilderCanvas | null>(null);
	/** Per schema: its tables with types and keys, loaded when a table of it is first added. */
	let diagrams = $state<Record<string, Diagram | 'loading' | 'failed'>>({});

	const filtered = $derived.by(() => {
		const needle = search.trim().toLowerCase();
		const hits = needle
			? tables.filter((t) => `${t.schema}.${t.name}`.toLowerCase().includes(needle))
			: tables;
		const groups: Record<string, CompletionTable[]> = {};
		for (const table of hits) (groups[table.schema] ??= []).push(table);
		return Object.entries(groups);
	});

	const onCanvas = $derived(
		model.tables.reduce<Record<string, number>>((count, t) => {
			const key = `${t.schema}.${t.name}`;
			count[key] = (count[key] ?? 0) + 1;
			return count;
		}, {})
	);

	async function loadSchema(schema: string): Promise<Diagram | null> {
		const known = diagrams[schema];
		if (known && known !== 'loading' && known !== 'failed') return known;
		diagrams[schema] = 'loading';
		try {
			const diagram = await api.schemaDiagram(connectionId, database, schema);
			diagrams[schema] = diagram;
			return diagram;
		} catch (e) {
			diagrams[schema] = 'failed';
			toast.error(`Could not read keys of ${schema}: ${errorMessage(e)}`);
			return null;
		}
	}

	async function add(schema: string, name: string, position?: { x: number; y: number }) {
		const columnCount = tables.find((t) => t.schema === schema && t.name === name)?.columns.length;
		// Header, padding and one 24px row per column, capped where the node starts scrolling.
		const height = Math.min(40 + (columnCount ?? 6) * 24, 330);
		const { id } = addTable(model, schema, name, position ?? canvas?.freeSpot(height));
		const diagram = await loadSchema(schema);
		const table = model.tables.find((t) => t.id === id);
		if (!diagram || !table) return;
		const joins = foreignKeyJoins(model, table, diagram.foreignKeys);
		model.joins.push(...joins);
		if (joins.length) {
			const others = joins
				.flatMap((j) => [j.from.tableId, j.to.tableId])
				.filter((tableId) => tableId !== id);
			const names = model.tables.filter((t) => others.includes(t.id)).map((t) => t.alias);
			toast.success(`Joined ${table.alias} to ${names.join(', ')} by foreign key`);
		}
	}

	function columnsOf(table: BuilderTable): { columns: BuilderNodeColumn[]; loading: boolean } {
		const diagram = diagrams[table.schema];
		const detailed =
			diagram && typeof diagram === 'object'
				? diagram.tables.find((t) => t.name === table.name)
				: undefined;
		if (detailed) {
			return {
				columns: detailed.columns.map((c) => ({
					name: c.name,
					dataType: c.dataType,
					primaryKey: c.primaryKey
				})),
				loading: false
			};
		}
		const basic = tables.find((t) => t.schema === table.schema && t.name === table.name);
		return {
			columns: (basic?.columns ?? []).map((name) => ({ name, dataType: '', primaryKey: false })),
			loading: diagram === 'loading'
		};
	}

	// Tables restored with the tab still need their column details.
	$effect(() => {
		for (const schema of new Set(model.tables.map((t) => t.schema))) {
			if (!(schema in diagrams)) void loadSchema(schema);
		}
	});

	const columnOptions = $derived(
		model.tables.flatMap((table) =>
			columnsOf(table).columns.map((c) => ({
				value: `${table.id}.${c.name}`,
				label: `${table.alias}.${c.name}`
			}))
		)
	);

	const built = $derived(buildSql(model));

	function clear() {
		Object.assign(model, emptyModel());
	}

	function keydown(event: KeyboardEvent) {
		if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && built.sql) {
			event.preventDefault();
			onrun(built.sql);
		}
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="h-full min-h-0 border-t" onkeydown={keydown}>
	<Resizable.PaneGroup direction="horizontal" autoSaveId="guckpg.builder" class="h-full">
		<Resizable.Pane defaultSize={15} minSize={10} class="flex min-h-0 flex-col bg-chrome">
			<div class="flex h-9 shrink-0 items-center gap-1.5 border-b px-2.5">
				<SearchIcon class="size-3.5 shrink-0 text-muted-foreground" />
				<input
					class="h-full min-w-0 flex-1 bg-transparent text-[13px] placeholder:text-muted-foreground focus:outline-none"
					placeholder="Find table"
					aria-label="Find table"
					bind:value={search}
				/>
			</div>
			<div class="min-h-0 flex-1 overflow-y-auto py-1">
				{#each filtered as [schema, list] (schema)}
					<p class="px-2.5 pt-2 pb-1 text-xs text-muted-foreground">{schema}</p>
					{#each list as table (table.name)}
						{@const count = onCanvas[`${table.schema}.${table.name}`] ?? 0}
						{@const style = kindStyle('table')}
						<button
							type="button"
							draggable="true"
							class="group flex h-7 w-full items-center gap-1.5 rounded-lg px-2.5 text-left text-[13px] text-ink-2 hover:bg-hover hover:text-foreground"
							title="Click or drag onto the canvas"
							ondragstart={(e) => {
								e.dataTransfer?.setData(
									TABLE_DRAG_TYPE,
									JSON.stringify({ schema: table.schema, name: table.name })
								);
								if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
							}}
							onclick={() => add(table.schema, table.name)}
						>
							<style.icon class="size-3.5 shrink-0 {style.color}" />
							<span class="min-w-0 flex-1 truncate">{table.name}</span>
							{#if count}
								<span class="measured text-[11px] text-signal-text" title="On the canvas"
									>{count > 1 ? `×${count}` : '●'}</span
								>
							{/if}
						</button>
					{/each}
				{:else}
					<p class="px-2.5 py-2 text-xs text-muted-foreground">
						{tables.length ? 'No table matches.' : 'Loading tables…'}
					</p>
				{/each}
			</div>
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={49} minSize={25}>
			<SvelteFlowProvider>
				<BuilderCanvas bind:this={canvas} {model} {columnsOf} onadd={add} />
			</SvelteFlowProvider>
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={36} minSize={22} class="flex min-h-0 flex-col bg-chrome">
			<div class="flex h-9 shrink-0 items-center gap-2 border-b pr-1 pl-3">
				<span class="text-[13px] font-medium">Query</span>
				<Button
					size="sm"
					variant="ghost"
					class="ml-auto text-muted-foreground"
					disabled={model.tables.length === 0}
					onclick={clear}
				>
					<EraserIcon /> Clear
				</Button>
			</div>
			<Resizable.PaneGroup
				direction="vertical"
				autoSaveId="guckpg.builder.side"
				class="min-h-0 flex-1"
			>
				<Resizable.Pane defaultSize={58} minSize={20} class="!overflow-y-auto">
					{#if model.tables.length}
						<ClausePanel {model} columns={columnOptions} />
					{:else}
						<p class="p-3 text-xs text-muted-foreground">
							Columns, joins, conditions and sorting appear here once a table is on the canvas.
						</p>
					{/if}
				</Resizable.Pane>
				<Resizable.Handle />
				<Resizable.Pane defaultSize={42} minSize={15} class="flex min-h-0 flex-col">
					<div class="flex h-9 shrink-0 items-center gap-1 px-2">
						<span class="pl-1 text-xs text-muted-foreground">SQL</span>
						<Button
							size="icon-sm"
							variant="ghost"
							class="ml-auto text-muted-foreground"
							aria-label="Copy SQL"
							title="Copy SQL"
							disabled={!built.sql}
							onclick={async () => {
								await navigator.clipboard.writeText(built.sql);
								toast.success('Copied SQL');
							}}><CopyIcon /></Button
						>
						<Button
							size="sm"
							variant="ghost"
							disabled={!built.sql}
							title="Put the query into the SQL editor"
							onclick={() => onopen(built.sql)}><SquarePenIcon /> Edit SQL</Button
						>
						<Button
							size="sm"
							class="pr-1.5"
							disabled={!built.sql || running}
							onclick={() => onrun(built.sql)}
						>
							<PlayIcon class="fill-current" /> Run
							<kbd
								class="ml-1 rounded-[4px] bg-primary-foreground/15 px-1 text-[11px] leading-4 font-normal"
								>⌘↵</kbd
							>
						</Button>
					</div>
					{#if built.problems.length && model.tables.length}
						<ul
							class="mx-2 mb-2 flex flex-col gap-0.5 rounded-lg bg-signal-soft px-2.5 py-1.5 text-xs"
						>
							{#each built.problems as problem (problem)}
								<li class="flex gap-1.5">
									<TriangleAlertIcon class="mt-0.5 size-3 shrink-0 text-signal-text" />{problem}
								</li>
							{/each}
						</ul>
					{/if}
					<pre
						class="measured mx-2 mb-2 min-h-0 flex-1 overflow-auto rounded-lg border bg-work p-2.5 text-xs leading-relaxed whitespace-pre-wrap select-text">{#if built.sql}{#each highlightSql(built.sql) as token, t (t)}{#if token.classes}<span
										class={token.classes}>{token.text}</span
									>{:else}{token.text}{/if}{/each}{:else}<span class="text-muted-foreground"
								>-- The query appears here as you build it.</span
							>{/if}</pre>
				</Resizable.Pane>
			</Resizable.PaneGroup>
		</Resizable.Pane>
	</Resizable.PaneGroup>
</div>
