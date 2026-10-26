<script lang="ts">
	import { onMount } from 'svelte';
	import {
		SvelteFlow,
		Background,
		Controls,
		MiniMap,
		MarkerType,
		type Edge,
		type Node
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { mode } from 'mode-watcher';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import LayoutGridIcon from '@lucide/svelte/icons/layout-grid';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import FocusIcon from '@lucide/svelte/icons/focus';
	import XIcon from '@lucide/svelte/icons/x';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { api, errorMessage, type Diagram } from '#lib/api.js';
	import {
		COLUMN_MODES,
		COLUMN_MODE_KEY,
		EDGE_STYLES,
		EDGE_STYLE_KEY,
		focusDiagram,
		layoutDiagram,
		parseColumnMode,
		parseEdgeStyle,
		routeDiagram,
		routeEdge,
		type ColumnMode,
		type EdgeStyle,
		type ErEdge,
		type TableNodeData
	} from '#lib/er-layout.js';
	import type { Route } from '#lib/er-route.js';
	import {
		applyPositions,
		clearPositions,
		loadPositions,
		positionsKey,
		savePositions
	} from '#lib/er-positions.js';
	import { tabs, type DiagramTab } from '#lib/state/tabs.svelte.js';
	import TableNode from './TableNode.svelte';
	import RoutedEdge from './RoutedEdge.svelte';

	interface Props {
		tab: DiagramTab;
	}

	let { tab }: Props = $props();

	const nodeTypes = { table: TableNode };
	const edgeTypes = { routed: RoutedEdge };

	let diagram = $state.raw<Diagram | null>(null);
	let nodes = $state.raw<Node<TableNodeData>[]>([]);
	let edges = $state.raw<Edge[]>([]);
	let baseEdges = $state.raw<ErEdge[]>([]);
	let routes = $state.raw(new Map<string, Route>());
	/** Clicked table whose relations stand out. */
	let highlighted = $state<string | null>(null);
	/** Table whose neighbourhood is shown instead of the whole schema. */
	let focusTable = $state<string | null>(null);
	let columnMode = $state<ColumnMode>(parseColumnMode(localStorage.getItem(COLUMN_MODE_KEY)));
	let edgeStyle = $state<EdgeStyle>(parseEdgeStyle(localStorage.getItem(EDGE_STYLE_KEY)));
	/** Remounts the flow so `fitView` applies to a fresh layout. */
	let layoutRun = $state(0);

	const storageKey = () => positionsKey(tab.connectionId, tab.database, tab.schema, columnMode);

	async function load() {
		tab.running = true;
		tab.error = null;
		try {
			diagram = await api.schemaDiagram(tab.connectionId, tab.database, tab.schema);
			if (focusTable && !diagram.tables.some((t) => t.name === focusTable)) focusTable = null;
			relayout();
		} catch (e) {
			tab.error = { kind: 'message', message: errorMessage(e) };
		} finally {
			tab.running = false;
		}
	}

	/** Lay the diagram out again; hand-placed positions win unless a focus is shown. */
	function relayout() {
		if (!diagram) return;
		const layout = layoutDiagram(
			focusTable ? focusDiagram(diagram, focusTable) : diagram,
			columnMode
		);
		nodes = focusTable
			? layout.nodes
			: applyPositions(layout.nodes, loadPositions(localStorage, storageKey()));
		baseEdges = layout.edges;
		highlighted = focusTable;
		reroute(nodes);
		layoutRun++;
	}

	function autoLayout() {
		if (!focusTable) clearPositions(localStorage, storageKey());
		relayout();
	}

	function placed(list: Node<TableNodeData>[]) {
		return list.map((n) => ({
			id: n.id,
			position: n.position,
			width: n.width ?? n.measured?.width ?? 0,
			height: n.height ?? n.measured?.height ?? 0,
			data: n.data
		}));
	}

	function reroute(list: Node<TableNodeData>[]) {
		routes = edgeStyle === 'routed' ? routeDiagram(placed(list), baseEdges) : new Map();
	}

	function setColumnMode(value: ColumnMode) {
		columnMode = value;
		localStorage.setItem(COLUMN_MODE_KEY, value);
		relayout();
	}

	function setEdgeStyle(value: EdgeStyle) {
		edgeStyle = value;
		localStorage.setItem(EDGE_STYLE_KEY, value);
		reroute(nodes);
	}

	function setFocus(table: string | null) {
		focusTable = table;
		relayout();
	}

	// Lines of dragged tables fall back to simple steps until the drop routes them again.
	function dragStart(dragged: Node[]) {
		const moving = new Set(dragged.map((n) => n.id));
		const touching = new Set(
			baseEdges.filter((e) => moving.has(e.source) || moving.has(e.target)).map((e) => e.id)
		);
		routes = new Map([...routes].filter(([id]) => !touching.has(id)));
	}

	function dragStop(dragged: Node[]) {
		const moved = new Map(dragged.map((n) => [n.id, n.position]));
		const current = nodes.map((n) => (moved.has(n.id) ? { ...n, position: moved.get(n.id)! } : n));
		if (!focusTable) savePositions(localStorage, storageKey(), current);
		reroute(current);
	}

	// Edges of the clicked table stand out; the rest fades. Unrouted lines dock
	// on the sides that face each other at the current positions.
	$effect(() => {
		const h = highlighted;
		const style = edgeStyle;
		const byId = new Map(placed(nodes).map((n) => [n.id, n]));
		edges = baseEdges.map((base) => {
			const source = byId.get(base.source);
			const target = byId.get(base.target);
			const route = style === 'routed' ? routes.get(base.id) : undefined;
			let e: Edge = base;
			if (source && target) {
				e = route
					? {
							...routeEdge(base, source, target, route),
							data: { ...base.data, points: route.points }
						}
					: routeEdge(base, source, target);
			}
			const active = h !== null && (e.source === h || e.target === h);
			const dimmed = h !== null && !active;
			const color = active ? 'var(--primary)' : 'var(--muted-foreground)';
			return {
				...e,
				type: style,
				animated: active,
				zIndex: active ? 10 : 0,
				markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16, color },
				style: `stroke: ${color}; stroke-width: ${active ? 2 : 1.25}; opacity: ${dimmed ? 0.2 : 0.8}`,
				labelStyle: 'font-size: 10px; font-family: var(--font-mono)'
			};
		});
	});

	onMount(load);

	let schemas = $state<string[]>([]);

	async function loadSchemas() {
		try {
			schemas = await api.listSchemas(tab.connectionId, tab.database);
		} catch {
			schemas = [];
		}
	}

	function switchSchema(schema: string) {
		if (!schema || tabs.switchDiagramSchema(tab.id, schema) !== tab) return;
		focusTable = null;
		highlighted = null;
		void load();
	}

	const tableCount = $derived(diagram?.tables.length ?? 0);
	const fkCount = $derived(diagram?.foreignKeys.length ?? 0);
</script>

{#snippet segmented<T extends string>(
	label: string,
	options: readonly { id: T; label: string }[],
	value: T,
	onchange: (value: T) => void
)}
	<div class="flex shrink-0 items-center gap-1">
		<span class="text-xs text-muted-foreground">{label}</span>
		<div class="flex items-center rounded-md border p-0.5" role="group" aria-label={label}>
			{#each options as option (option.id)}
				<Button
					size="sm"
					variant={value === option.id ? 'secondary' : 'ghost'}
					class="h-6 px-2 text-xs"
					aria-pressed={value === option.id}
					onclick={() => onchange(option.id)}
				>
					{option.label}
				</Button>
			{/each}
		</div>
	</div>
{/snippet}

<div class="flex h-full min-h-0 flex-col">
	<div class="flex h-10 shrink-0 items-center gap-2 overflow-x-auto border-b px-2">
		<Button size="icon-sm" variant="ghost" disabled={tab.running} onclick={load} title="Reload">
			<RefreshCwIcon class={tab.running ? 'animate-spin' : ''} />
		</Button>
		<Button
			size="sm"
			variant="ghost"
			disabled={!diagram}
			onclick={autoLayout}
			title="Discard moved tables and lay everything out again"
		>
			<LayoutGridIcon /> Auto layout
		</Button>
		{@render segmented('Columns', COLUMN_MODES, columnMode, setColumnMode)}
		{@render segmented('Lines', EDGE_STYLES, edgeStyle, setEdgeStyle)}
		{#if focusTable}
			<div
				class="flex shrink-0 items-center gap-1 rounded-md border border-primary/40 bg-primary/10 py-0.5 pr-0.5 pl-2 text-xs"
			>
				<FocusIcon class="size-3.5" />
				<span class="font-mono">{focusTable}</span>
				<Button
					size="icon-sm"
					variant="ghost"
					class="size-5"
					title="Show all tables"
					onclick={() => setFocus(null)}
				>
					<XIcon class="size-3" />
				</Button>
			</div>
		{/if}
		{#if highlighted && highlighted !== focusTable}
			<Button
				size="sm"
				variant="ghost"
				class="shrink-0"
				title="Show only this table and the tables it is related to"
				onclick={() => setFocus(highlighted)}
			>
				<FocusIcon /> Neighbours of {highlighted}
			</Button>
		{/if}
		<span
			class="shrink-0 text-xs whitespace-nowrap text-muted-foreground"
			title="Click a table to highlight its relations, double-click to open its data. Moved tables are remembered."
		>
			{tableCount} tables · {fkCount} foreign keys
		</span>
		<Select.Root
			type="single"
			value={tab.schema}
			onValueChange={switchSchema}
			onOpenChange={(open) => open && loadSchemas()}
			disabled={tab.running}
		>
			<Select.Trigger
				size="sm"
				class="ml-auto h-7 shrink-0 gap-1.5 border-transparent bg-transparent text-[13px] shadow-none hover:bg-hover data-[state=open]:bg-hover dark:bg-transparent dark:hover:bg-hover"
				aria-label="Schema"
			>
				<FolderIcon class="size-3.5 text-muted-foreground" />
				<span class="text-muted-foreground">{tab.database} /</span>
				<span class="font-medium">{tab.schema}</span>
			</Select.Trigger>
			<Select.Content align="end">
				{#each schemas.includes(tab.schema) ? schemas : [tab.schema, ...schemas] as schema (schema)}
					<Select.Item value={schema} label={schema} />
				{/each}
			</Select.Content>
		</Select.Root>
	</div>

	<div class="relative min-h-0 flex-1">
		{#if tab.error}
			<div
				class="m-3 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive select-text"
			>
				{errorMessage(tab.error)}
			</div>
		{:else if !diagram}
			<div class="flex h-full items-center justify-center text-muted-foreground">
				<LoaderIcon class="size-5 animate-spin" />
			</div>
		{:else if tableCount === 0}
			<div class="flex h-full items-center justify-center text-sm text-muted-foreground">
				No tables in {tab.schema}.
			</div>
		{:else}
			{#key layoutRun}
				<SvelteFlow
					bind:nodes
					bind:edges
					{nodeTypes}
					{edgeTypes}
					colorMode={mode.current ?? 'light'}
					fitView
					fitViewOptions={{ padding: 0.1 }}
					minZoom={0.05}
					maxZoom={2}
					nodesConnectable={false}
					elementsSelectable
					deleteKey={null}
					proOptions={{ hideAttribution: true }}
					onnodeclick={({ node, event }) => {
						if (event.detail >= 2)
							tabs.openTable(tab.connectionId, tab.database, tab.schema, node.id);
						else highlighted = node.id;
					}}
					onnodedragstart={({ nodes: dragged }) => dragStart(dragged)}
					onnodedragstop={({ nodes: dragged }) => dragStop(dragged)}
					onpaneclick={() => (highlighted = focusTable)}
					style="--xy-background-color: transparent"
				>
					<Background gap={20} />
					<Controls showLock={false} />
					<MiniMap
						pannable
						zoomable
						class="!bg-card"
						nodeColor={(n) =>
							n.data?.hue == null
								? 'var(--muted-foreground)'
								: `oklch(0.65 0.12 ${n.data.hue as number})`}
					/>
				</SvelteFlow>
			{/key}
		{/if}
	</div>
</div>
