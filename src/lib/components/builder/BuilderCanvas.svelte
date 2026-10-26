<script lang="ts" module>
	export const TABLE_DRAG_TYPE = 'application/x-guckpg-table';
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import {
		Background,
		ConnectionMode,
		Controls,
		SvelteFlow,
		useSvelteFlow,
		type Connection,
		type Edge,
		type Node
	} from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { mode } from 'mode-watcher';
	import MousePointerClickIcon from '@lucide/svelte/icons/mouse-pointer-click';
	import {
		isSelected,
		newId,
		removeTable,
		toggleColumn,
		type BuilderModel,
		type BuilderTable
	} from '#lib/query-builder.js';
	import BuilderTableNode, {
		columnOfHandle,
		leftHandle,
		rightHandle,
		type BuilderNodeColumn,
		type BuilderNodeData
	} from './BuilderTableNode.svelte';
	import JoinEdge, { type JoinEdgeData } from './JoinEdge.svelte';

	interface Props {
		model: BuilderModel;
		columnsOf: (table: BuilderTable) => { columns: BuilderNodeColumn[]; loading: boolean };
		onadd: (schema: string, name: string, position: { x: number; y: number }) => void;
	}

	let { model, columnsOf, onadd }: Props = $props();

	const nodeTypes = { builderTable: BuilderTableNode };
	const edgeTypes = { join: JoinEdge };
	const flow = useSvelteFlow();

	let nodes = $state.raw<Node<BuilderNodeData>[]>([]);
	let edges = $state.raw<Edge<JoinEdgeData>[]>([]);

	// Rebuilt from the model; existing nodes keep what Svelte Flow measured about them.
	$effect(() => {
		nodes = model.tables.map((table) => {
			const existing = untrack(() => nodes.find((node) => node.id === table.id));
			const { columns, loading } = columnsOf(table);
			return {
				...existing,
				id: table.id,
				type: 'builderTable',
				position: { x: table.x, y: table.y },
				data: {
					alias: table.alias,
					name: table.name,
					schema: table.schema,
					columns,
					loading,
					selected: model.columns.filter((c) => c.tableId === table.id).map((c) => c.column),
					ontoggle: (column) => toggleColumn(model, { tableId: table.id, column }),
					ontoggleall: (on) => {
						for (const column of columns) {
							const ref = { tableId: table.id, column: column.name };
							if (isSelected(model, ref) !== on) toggleColumn(model, ref);
						}
					},
					onremove: () => removeTable(model, table.id)
				}
			};
		});
	});

	$effect(() => {
		const byId = new Map(model.tables.map((table) => [table.id, table]));
		edges = model.joins.flatMap((join) => {
			const from = byId.get(join.from.tableId);
			const to = byId.get(join.to.tableId);
			if (!from || !to) return [];
			// Lines leave from the facing sides so they do not cross the tables.
			const fromIsLeft = from.x <= to.x;
			return [
				{
					id: join.id,
					type: 'join',
					source: from.id,
					target: to.id,
					sourceHandle: (fromIsLeft ? rightHandle : leftHandle)(join.from.column),
					targetHandle: (fromIsLeft ? leftHandle : rightHandle)(join.to.column),
					data: {
						type: join.type,
						fromAlias: from.alias,
						toAlias: to.alias,
						onchange: (type) => (join.type = type),
						onremove: () => (model.joins = model.joins.filter((j) => j.id !== join.id))
					}
				}
			];
		});
	});

	let container = $state<HTMLDivElement | null>(null);

	const NODE_WIDTH = 240;
	const GAP = 48;

	function visibleArea() {
		const rect = container!.getBoundingClientRect();
		const a = flow.screenToFlowPosition({ x: rect.left, y: rect.top });
		const b = flow.screenToFlowPosition({ x: rect.right, y: rect.bottom });
		return { x0: a.x, y0: a.y, x1: b.x, y1: b.y };
	}

	function boxes() {
		return flow.getNodes().map((node) => ({
			x: node.position.x,
			y: node.position.y,
			w: node.measured?.width ?? NODE_WIDTH,
			h: node.measured?.height ?? 200
		}));
	}

	/** A spot in view where a table of this height overlaps nothing, else right of everything. */
	export function freeSpot(height: number): { x: number; y: number } {
		if (!container) return { x: 40, y: 40 };
		const view = visibleArea();
		const taken = boxes();
		const clear = (x: number, y: number) =>
			!taken.some(
				(b) =>
					b.x - GAP < x + NODE_WIDTH &&
					x < b.x + b.w + GAP &&
					b.y - GAP < y + height &&
					y < b.y + b.h + GAP
			);
		for (let y = view.y0 + 24; y + Math.min(height, 200) <= view.y1; y += 24) {
			for (let x = view.x0 + 24; x + NODE_WIDTH <= view.x1; x += 24) {
				if (clear(x, y)) return { x, y };
			}
		}
		const right = Math.max(view.x0, ...taken.map((b) => b.x + b.w));
		return { x: right + GAP, y: view.y0 + 24 };
	}

	// Pan to a new table that landed outside the view; moving one around does not.
	// The delay lets Svelte Flow measure the new node first.
	let tableCount = 0;
	$effect(() => {
		const count = model.tables.length;
		if (count > tableCount) setTimeout(reveal, 60);
		tableCount = count;
	});

	function reveal() {
		const newest = model.tables.at(-1);
		const node = newest && flow.getNode(newest.id);
		if (!node || !container) return;
		const view = visibleArea();
		const w = node.measured?.width ?? NODE_WIDTH;
		const h = node.measured?.height ?? 200;
		const { x, y } = node.position;
		if (x >= view.x0 && y >= view.y0 && x + w <= view.x1 && y + Math.min(h, 200) <= view.y1) return;
		void flow.setCenter(x + w / 2, y + Math.min(h, view.y1 - view.y0) / 2, {
			zoom: flow.getViewport().zoom,
			duration: 200
		});
	}

	function connect(connection: Connection) {
		const fromColumn = columnOfHandle(connection.sourceHandle);
		const toColumn = columnOfHandle(connection.targetHandle);
		if (!fromColumn || !toColumn || connection.source === connection.target) return;
		const same = (a: string, b: string, c: string, d: string) =>
			model.joins.some(
				(j) =>
					j.from.tableId === a && j.from.column === b && j.to.tableId === c && j.to.column === d
			);
		if (
			same(connection.source, fromColumn, connection.target, toColumn) ||
			same(connection.target, toColumn, connection.source, fromColumn)
		)
			return;
		model.joins.push({
			id: newId('j'),
			type: 'INNER',
			from: { tableId: connection.source, column: fromColumn },
			to: { tableId: connection.target, column: toColumn }
		});
	}

	function drop(event: DragEvent) {
		const raw = event.dataTransfer?.getData(TABLE_DRAG_TYPE);
		if (!raw) return;
		event.preventDefault();
		const { schema, name } = JSON.parse(raw) as { schema: string; name: string };
		const point = flow.screenToFlowPosition({ x: event.clientX, y: event.clientY });
		onadd(schema, name, { x: point.x - 40, y: point.y - 16 });
	}
</script>

<div
	class="relative h-full"
	bind:this={container}
	role="application"
	aria-label="Query canvas"
	ondragover={(e) => {
		if (e.dataTransfer?.types.includes(TABLE_DRAG_TYPE)) {
			e.preventDefault();
			e.dataTransfer.dropEffect = 'copy';
		}
	}}
	ondrop={drop}
>
	<SvelteFlow
		bind:nodes
		bind:edges
		{nodeTypes}
		{edgeTypes}
		colorMode={mode.current ?? 'light'}
		connectionMode={ConnectionMode.Loose}
		connectionRadius={12}
		minZoom={0.25}
		maxZoom={1.5}
		deleteKey={['Delete', 'Backspace']}
		proOptions={{ hideAttribution: true }}
		onconnect={connect}
		onnodedragstop={({ nodes: moved }) => {
			for (const node of moved) {
				const table = model.tables.find((t) => t.id === node.id);
				if (table) [table.x, table.y] = [node.position.x, node.position.y];
			}
		}}
		ondelete={({ nodes: gone, edges: cut }) => {
			for (const node of gone) removeTable(model, node.id);
			const ids = new Set(cut.map((edge) => edge.id));
			if (ids.size) model.joins = model.joins.filter((join) => !ids.has(join.id));
		}}
		style="--xy-background-color: transparent; --xy-connectionline-stroke-default: var(--color-accent)"
	>
		<Background gap={20} />
		<Controls showLock={false} />
	</SvelteFlow>
	{#if model.tables.length === 0}
		<div
			class="pointer-events-none absolute inset-0 grid place-items-center text-center text-sm text-muted-foreground"
		>
			<div class="flex flex-col items-center gap-2">
				<MousePointerClickIcon class="size-5" />
				<p>Drag tables here, or click one in the list.</p>
				<p class="text-xs">Drag from a column onto a column of another table to join them.</p>
			</div>
		</div>
	{/if}
</div>
