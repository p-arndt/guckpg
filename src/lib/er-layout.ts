import type { Column, Diagram, RelationKind } from './api.js';
import { OrthogonalRouter, type Port, type Route, type Side } from './er-route.js';

export type { Side } from './er-route.js';

export const HEADER_PX = 34;
export const ROW_PX = 22;
/** Vertical padding of the column list, top and bottom. */
export const BODY_PAD_PX = 3;
const BORDER_PX = 1;
const CHAR_PX = 6.8;
const MIN_WIDTH = 200;
const MAX_WIDTH = 380;

export interface TableNodeData extends Record<string, unknown> {
	name: string;
	kind: RelationKind;
	/** The columns shown, after the column mode is applied. */
	columns: Column[];
	/** Columns left out by the column mode. */
	hiddenColumns: number;
	/** Header only, no column list. */
	compact: boolean;
	/** Columns that are part of a foreign key of this table. */
	foreignKeyColumns: string[];
	/** Name prefix shared with other tables, and the hue it is drawn in. */
	group: string | null;
	hue: number | null;
}

export interface ErNode {
	id: string;
	type: 'table';
	position: { x: number; y: number };
	width: number;
	height: number;
	data: TableNodeData;
}

export interface ErEdge {
	id: string;
	source: string;
	target: string;
	sourceHandle: string;
	targetHandle: string;
	label?: string;
	data: { columns: string; fromColumn: string; toColumn: string };
}

/** How edges are drawn; ids are xyflow edge types, `routed` is ours. */
export const EDGE_STYLES = [
	{ id: 'routed', label: 'Orthogonal' },
	{ id: 'straight', label: 'Straight' },
	{ id: 'default', label: 'Curved' }
] as const;
export type EdgeStyle = (typeof EDGE_STYLES)[number]['id'];
export const EDGE_STYLE_KEY = 'guckpg.er.edgeStyle';

/** A stored edge style, or routed orthogonal lines when nothing valid is stored. */
export function parseEdgeStyle(value: string | null | undefined): EdgeStyle {
	return EDGE_STYLES.find((s) => s.id === value)?.id ?? 'routed';
}

/** Which columns a table shows. */
export const COLUMN_MODES = [
	{ id: 'all', label: 'All' },
	{ id: 'keys', label: 'Keys' },
	{ id: 'none', label: 'None' }
] as const;
export type ColumnMode = (typeof COLUMN_MODES)[number]['id'];
export const COLUMN_MODE_KEY = 'guckpg.er.columns';

/** A stored column mode, or key columns only when nothing valid is stored. */
export function parseColumnMode(value: string | null | undefined): ColumnMode {
	return COLUMN_MODES.find((m) => m.id === value)?.id ?? 'keys';
}

/** Handle of the table header, used for edges whose column is hidden. */
export const TABLE_HANDLE = '#table';

/** Every column has a source and a target handle on both sides. */
export const sourceHandle = (column: string, side: Side = 'right') => `${column}:out:${side}`;
export const targetHandle = (column: string, side: Side = 'left') => `${column}:in:${side}`;

interface Placed {
	position: { x: number; y: number };
	width: number;
}

interface PlacedNode extends Placed {
	height: number;
	data: Pick<TableNodeData, 'columns' | 'compact'>;
}

/**
 * Which sides an edge leaves and enters: the facing sides when the tables sit
 * next to each other, the right side of both when they are stacked.
 */
export function pickHandles(
	source: Placed,
	target: Placed
): { sourceSide: Side; targetSide: Side } {
	if (target.position.x >= source.position.x + source.width)
		return { sourceSide: 'right', targetSide: 'left' };
	if (target.position.x + target.width <= source.position.x)
		return { sourceSide: 'left', targetSide: 'right' };
	return { sourceSide: 'right', targetSide: 'right' };
}

/** The column's own handle when it is shown, otherwise the table header. */
export function handleColumn(node: PlacedNode, column: string): string {
	return !node.data.compact && node.data.columns.some((c) => c.name === column)
		? column
		: TABLE_HANDLE;
}

/** Flow y of a column's handle, or of the header for hidden columns. */
export function portY(node: PlacedNode, column: string): number {
	const index =
		handleColumn(node, column) === TABLE_HANDLE
			? -1
			: node.data.columns.findIndex((c) => c.name === column);
	if (index < 0) return node.position.y + BORDER_PX + HEADER_PX / 2;
	return node.position.y + BORDER_PX + HEADER_PX + BODY_PAD_PX + index * ROW_PX + ROW_PX / 2;
}

/** The edge with handles on the given sides, or on the facing sides at the current positions. */
export function routeEdge(
	edge: ErEdge,
	source: PlacedNode,
	target: PlacedNode,
	sides = pickHandles(source, target)
): ErEdge {
	return {
		...edge,
		sourceHandle: sourceHandle(handleColumn(source, edge.data.fromColumn), sides.sourceSide),
		targetHandle: targetHandle(handleColumn(target, edge.data.toColumn), sides.targetSide)
	};
}

/**
 * Orthogonal routes for all edges around all tables. Edges between close
 * tables go first so they get the short, straight lines.
 */
export function routeDiagram(
	nodes: (PlacedNode & { id: string })[],
	edges: ErEdge[]
): Map<string, Route> {
	const byId = new Map(nodes.map((n) => [n.id, n]));
	const rect = (n: PlacedNode) => ({
		x: n.position.x,
		y: n.position.y,
		width: n.width,
		height: n.height
	});
	const ports = (n: PlacedNode, column: string): Port[] =>
		(['left', 'right'] as const).map((side) => ({ rect: rect(n), y: portY(n, column), side }));

	const jobs = edges.flatMap((e) => {
		const source = byId.get(e.source);
		const target = byId.get(e.target);
		if (!source || !target) return [];
		const distance =
			Math.abs(source.position.x - target.position.x) +
			Math.abs(source.position.y - target.position.y);
		return [
			{ e, from: ports(source, e.data.fromColumn), to: ports(target, e.data.toColumn), distance }
		];
	});
	jobs.sort((a, b) => a.distance - b.distance || a.e.id.localeCompare(b.e.id));

	const router = new OrthogonalRouter(
		nodes.map(rect),
		jobs.flatMap((j) => [j.from[0].y, j.to[0].y])
	);
	const routes = new Map<string, Route>();
	for (const job of jobs) {
		const route = router.route(job.from, job.to);
		if (route) routes.set(job.e.id, route);
	}
	return routes;
}

export function nodeSize(
	columns: Column[],
	name: string,
	hiddenColumns = 0,
	compact = false
): { width: number; height: number } {
	const longest = Math.max(
		name.length + 4,
		...(compact ? [] : columns.map((c) => c.name.length + c.dataType.length + 6))
	);
	const rows = Math.max(1, columns.length + (hiddenColumns > 0 ? 1 : 0));
	return {
		width: Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, longest * CHAR_PX + 32))),
		height: compact
			? HEADER_PX + 2 * BORDER_PX
			: HEADER_PX + rows * ROW_PX + 2 * BODY_PAD_PX + 2 * BORDER_PX
	};
}

/** Columns that take part in a key: primary keys and both ends of foreign keys. */
export function keyColumns(diagram: Diagram): Map<string, Set<string>> {
	const keys = new Map<string, Set<string>>();
	const add = (table: string, column: string) => {
		const set = keys.get(table) ?? new Set();
		set.add(column);
		keys.set(table, set);
	};
	for (const t of diagram.tables) for (const c of t.columns) if (c.primaryKey) add(t.name, c.name);
	for (const fk of diagram.foreignKeys) {
		fk.fromColumns.forEach((c) => add(fk.fromTable, c));
		fk.toColumns.forEach((c) => add(fk.toTable, c));
	}
	return keys;
}

/** `cliques` and `clique_log` belong together, so plurals count as their singular. */
function singular(word: string): string {
	if (word.endsWith('ies') && word.length > 4) return `${word.slice(0, -3)}y`;
	if (word.endsWith('sses') || word.endsWith('xes')) return word.slice(0, -2);
	if (word.endsWith('s') && !word.endsWith('ss') && word.length > 3) return word.slice(0, -1);
	return word;
}

/**
 * Tables grouped by the first word of their name (`casino_dice`,
 * `casino_roulette`, ...). Only prefixes shared by at least two tables form a
 * group. Returns the group of every grouped table.
 */
export function tableGroups(names: string[]): Map<string, string> {
	const byPrefix = new Map<string, string[]>();
	for (const name of names) {
		const prefix = singular(name.toLowerCase().split('_')[0]);
		if (!prefix) continue;
		byPrefix.set(prefix, [...(byPrefix.get(prefix) ?? []), name]);
	}
	const groups = new Map<string, string>();
	for (const [prefix, members] of byPrefix) {
		if (members.length >= 2) for (const m of members) groups.set(m, prefix);
	}
	return groups;
}

/** A table with every table it references or is referenced by. */
export function focusDiagram(diagram: Diagram, table: string): Diagram {
	const keep = new Set([table]);
	for (const fk of diagram.foreignKeys) {
		if (fk.fromTable === table) keep.add(fk.toTable);
		if (fk.toTable === table) keep.add(fk.fromTable);
	}
	return {
		tables: diagram.tables.filter((t) => keep.has(t.name)),
		foreignKeys: diagram.foreignKeys.filter((fk) => keep.has(fk.fromTable) && keep.has(fk.toTable))
	};
}

const MARGIN = 20;
const NODE_GAP = 28;
const COLUMN_GAP = 36;
/** Horizontal gap between aligned columns; the edges run in it. */
const LANE_GAP = 96;
/** Vertical gap between tables in a column. */
const ROW_GAP = 40;
/** Free space kept around every table in a connected group. */
const TABLE_GAP = 44;
/** Space between packed groups. */
const GROUP_GAP = 120;
/** Target width:height of the whole diagram, roughly a screen. */
const ASPECT = 1.6;
const ITERATIONS = 400;
const GRAVITY = 0.15;
/** Pull between tables of one name group, relative to a foreign key. */
const GROUP_PULL = 0.6;

interface Box {
	name: string;
	width: number;
	height: number;
}

/** Fill columns top to bottom, starting a new one when `maxHeight` is reached. */
export function packColumns<T extends Box>(boxes: T[], maxHeight: number): T[][] {
	const columns: T[][] = [];
	let current: T[] = [];
	let height = 0;
	for (const box of boxes) {
		if (current.length && height + box.height > maxHeight) {
			columns.push(current);
			current = [];
			height = 0;
		}
		current.push(box);
		height += box.height + NODE_GAP;
	}
	if (current.length) columns.push(current);
	return columns;
}

interface Block {
	width: number;
	height: number;
	/** Top-left corners relative to the block. */
	positions: Map<string, { x: number; y: number }>;
}

const extent = (boxes: Box[]) => boxes.reduce((h, b) => h + b.height + NODE_GAP, -NODE_GAP);

/** Tables without relations: a plain grid, roughly screen shaped. */
function gridBlock(boxes: Box[]): Block {
	const area = boxes.reduce((sum, b) => sum + (b.width + COLUMN_GAP) * (b.height + NODE_GAP), 0);
	const maxHeight = Math.max(...boxes.map((b) => b.height), Math.sqrt(area / ASPECT));
	const positions = new Map<string, { x: number; y: number }>();
	let x = 0;
	let height = 0;
	for (const column of packColumns(boxes, maxHeight)) {
		let y = 0;
		for (const b of column) {
			positions.set(b.name, { x, y });
			y += b.height + NODE_GAP;
		}
		x += Math.max(...column.map((b) => b.width)) + COLUMN_GAP;
		height = Math.max(height, extent(column));
	}
	return { width: x - COLUMN_GAP, height, positions };
}

interface Body extends Box {
	x: number;
	y: number;
	/** Radius of a circle with the table's area. */
	r: number;
}

/**
 * Force-directed placement of one connected group: related tables pull
 * together, all tables push each other apart, and a weak pull towards the
 * middle keeps the group compact and screen shaped. Hub tables end up in the
 * middle with the tables that reference them around them, so edges stay short
 * instead of fanning across the whole diagram. Deterministic for the same input.
 */
function forceBlock(
	boxes: Box[],
	edges: { source: string; target: string }[],
	groups: Map<string, string>
): Block {
	const index = new Map(boxes.map((b, i) => [b.name, i]));
	const links = edges
		.map((e) => [index.get(e.source)!, index.get(e.target)!] as const)
		.filter(([a, b]) => a !== b);
	// Tables of one name group pull together too, a bit weaker than a foreign
	// key. The pull is shared out over the group, so a big group does not
	// collapse into a ball and drown out the foreign keys.
	const groupSize = new Map<string, number>();
	for (const b of boxes) {
		const g = groups.get(b.name);
		if (g) groupSize.set(g, (groupSize.get(g) ?? 0) + 1);
	}
	const weighted: [number, number, number][] = links.map(([a, b]) => [a, b, 1]);
	for (let a = 0; a < boxes.length; a++) {
		for (let b = a + 1; b < boxes.length; b++) {
			const g = groups.get(boxes[a].name);
			if (g && g === groups.get(boxes[b].name)) {
				weighted.push([a, b, GROUP_PULL / (groupSize.get(g)! - 1)]);
			}
		}
	}
	const degree = boxes.map(() => 0);
	const neighbours = boxes.map(() => [] as number[]);
	for (const [a, b] of links) {
		degree[a]++;
		degree[b]++;
		neighbours[a].push(b);
		neighbours[b].push(a);
	}

	// Start on a spiral in breadth-first order from the best connected table,
	// so neighbours start close to each other.
	const start = boxes.reduce((best, b, i) => (degree[i] > degree[best] ? i : best), 0);
	const order = [start];
	const seen = new Set(order);
	for (let i = 0; i < order.length; i++) {
		const next = neighbours[order[i]]
			.filter((n) => !seen.has(n))
			.sort((a, b) => degree[b] - degree[a] || boxes[a].name.localeCompare(boxes[b].name));
		for (const n of next) {
			if (seen.has(n)) continue;
			seen.add(n);
			order.push(n);
		}
	}
	const bodies: Body[] = boxes.map((b) => ({
		...b,
		x: 0,
		y: 0,
		r: Math.sqrt(b.width * b.height) / 2
	}));
	const spacing = bodies.reduce((s, b) => s + b.r, 0) / bodies.length + TABLE_GAP;
	order.forEach((i, k) => {
		const angle = k * 2.399963; // golden angle
		bodies[i].x = Math.cos(angle) * spacing * Math.sqrt(k) * ASPECT;
		bodies[i].y = Math.sin(angle) * spacing * Math.sqrt(k);
	});

	const n = bodies.length;
	const dx = new Float64Array(n);
	const dy = new Float64Array(n);
	for (let it = 0; it < ITERATIONS; it++) {
		dx.fill(0);
		dy.fill(0);
		for (let i = 0; i < n; i++) {
			for (let j = i + 1; j < n; j++) {
				const a = bodies[i];
				const b = bodies[j];
				let vx = a.x - b.x;
				let vy = a.y - b.y;
				let d = Math.hypot(vx, vy);
				if (d < 1) {
					vx = i - j;
					vy = 1;
					d = Math.hypot(vx, vy);
				}
				const k = a.r + b.r + TABLE_GAP;
				const f = (k * k) / d / d;
				dx[i] += vx * f;
				dy[i] += vy * f;
				dx[j] -= vx * f;
				dy[j] -= vy * f;
			}
		}
		for (const [i, j, weight] of weighted) {
			const a = bodies[i];
			const b = bodies[j];
			const vx = a.x - b.x;
			const vy = a.y - b.y;
			const d = Math.max(1, Math.hypot(vx, vy));
			const k = a.r + b.r + TABLE_GAP;
			const f = (d / k) * weight;
			dx[i] -= vx * f;
			dy[i] -= vy * f;
			dx[j] += vx * f;
			dy[j] += vy * f;
		}
		const cx = bodies.reduce((s, b) => s + b.x, 0) / n;
		const cy = bodies.reduce((s, b) => s + b.y, 0) / n;
		const maxStep = spacing * (1 - it / ITERATIONS) + 2;
		for (let i = 0; i < n; i++) {
			const b = bodies[i];
			dx[i] -= (b.x - cx) * GRAVITY;
			dy[i] -= (b.y - cy) * GRAVITY * ASPECT * ASPECT;
			const len = Math.hypot(dx[i], dy[i]);
			const step = len > maxStep ? maxStep / len : 1;
			b.x += dx[i] * step;
			b.y += dy[i] * step;
		}
	}

	return alignColumns(bodies);
}

/**
 * Snap the force layout onto columns without losing its grouping: tables
 * whose centres lie close together horizontally share a column, and each
 * table keeps its height from the force layout, moved down only as far as
 * needed to clear the table above. The gaps between columns become clear
 * lanes for the edges.
 */
function alignColumns(bodies: Body[]): Block {
	const byX = [...bodies].sort((a, b) => a.x - b.x || a.y - b.y);
	const columns: Body[][] = [];
	let anchor = -Infinity;
	let reach = 0;
	for (const b of byX) {
		if (b.x - anchor > reach) {
			columns.push([]);
			anchor = b.x;
			reach = 0;
		}
		columns[columns.length - 1].push(b);
		reach = Math.max(reach, (b.width * 3) / 4);
	}

	const top = Math.min(...bodies.map((b) => b.y - b.height / 2));
	const positions = new Map<string, { x: number; y: number }>();
	let x = 0;
	let height = 0;
	for (const column of columns) {
		let next = 0;
		for (const b of [...column].sort((p, q) => p.y - q.y)) {
			const y = Math.max(next, Math.round(b.y - b.height / 2 - top));
			positions.set(b.name, { x, y });
			next = y + b.height + ROW_GAP;
		}
		x += Math.max(...column.map((b) => b.width)) + LANE_GAP;
		height = Math.max(height, next - ROW_GAP);
	}
	return { width: x - LANE_GAP, height, positions };
}

/** Connected groups of tables, largest first. */
function components(boxes: Box[], edges: ErEdge[]): Box[][] {
	const parent = new Map(boxes.map((b) => [b.name, b.name]));
	const find = (n: string): string => {
		while (parent.get(n) !== n) {
			parent.set(n, parent.get(parent.get(n)!)!);
			n = parent.get(n)!;
		}
		return n;
	};
	for (const e of edges) parent.set(find(e.source), find(e.target));
	const groups = new Map<string, Box[]>();
	for (const b of boxes) {
		const root = find(b.name);
		groups.set(root, [...(groups.get(root) ?? []), b]);
	}
	return [...groups.values()].sort((a, b) => b.length - a.length);
}

/**
 * Nodes and edges for the diagram. Every connected group of tables gets a
 * force-directed layout, tables without any relation form a grid, and all of
 * these blocks are then packed row by row into a screen-shaped area.
 */
export function layoutDiagram(
	diagram: Diagram,
	columnMode: ColumnMode = 'all'
): { nodes: ErNode[]; edges: ErEdge[] } {
	const keys = keyColumns(diagram);
	const shown = new Map(
		diagram.tables.map((t) => {
			const visible =
				columnMode === 'all'
					? t.columns
					: columnMode === 'keys'
						? t.columns.filter((c) => keys.get(t.name)?.has(c.name))
						: [];
			return [t.name, visible];
		})
	);
	const compact = columnMode === 'none';
	const groups = tableGroups(diagram.tables.map((t) => t.name));
	const groupNames = [...new Set(groups.values())].sort();
	const hues = new Map(groupNames.map((g, i) => [g, Math.round((i * 137.508) % 360)]));
	const size = (t: Diagram['tables'][number]) => {
		const columns = shown.get(t.name)!;
		return nodeSize(columns, t.name, compact ? 0 : t.columns.length - columns.length, compact);
	};

	const fkColumns = new Map<string, Set<string>>();
	for (const fk of diagram.foreignKeys) {
		const set = fkColumns.get(fk.fromTable) ?? new Set();
		fk.fromColumns.forEach((c) => set.add(c));
		fkColumns.set(fk.fromTable, set);
	}

	const tableNames = new Set(diagram.tables.map((t) => t.name));
	const edges: ErEdge[] = [];
	const related = new Set<string>();
	for (const fk of diagram.foreignKeys) {
		if (!tableNames.has(fk.fromTable) || !tableNames.has(fk.toTable)) continue;
		if (fk.fromTable !== fk.toTable) {
			related.add(fk.fromTable);
			related.add(fk.toTable);
		}
		edges.push({
			id: `${fk.fromTable}.${fk.name}`,
			source: fk.fromTable,
			target: fk.toTable,
			sourceHandle: sourceHandle(fk.fromColumns[0]),
			targetHandle: targetHandle(fk.toColumns[0]),
			label: fk.fromColumns.length > 1 ? `(${fk.fromColumns.join(', ')})` : undefined,
			data: {
				columns: `${fk.fromColumns.join(', ')} → ${fk.toTable}(${fk.toColumns.join(', ')})`,
				fromColumn: fk.fromColumns[0],
				toColumn: fk.toColumns[0]
			}
		});
	}

	const boxes: Box[] = diagram.tables.map((t) => ({ name: t.name, ...size(t) }));
	const connected = boxes.filter((b) => related.has(b.name));
	const isolated = boxes
		.filter((b) => !related.has(b.name))
		.sort((a, b) => a.name.localeCompare(b.name));

	const crossEdges = edges.filter((e) => e.source !== e.target);
	const blocks = components(connected, crossEdges).map((group) => {
		const names = new Set(group.map((b) => b.name));
		return forceBlock(
			group,
			crossEdges.filter((e) => names.has(e.source)),
			groups
		);
	});
	if (isolated.length) blocks.push(gridBlock(isolated));

	// Shelf-pack the blocks in order: fill a row, then start the next one below.
	const totalArea = blocks.reduce(
		(sum, b) => sum + (b.width + GROUP_GAP) * (b.height + GROUP_GAP),
		0
	);
	const rowWidth = Math.max(...blocks.map((b) => b.width), Math.sqrt(totalArea * ASPECT));
	const placed = new Map<string, { x: number; y: number }>();
	let x = 0;
	let y = 0;
	let rowHeight = 0;
	for (const block of blocks) {
		if (x > 0 && x + block.width > rowWidth) {
			x = 0;
			y += rowHeight + GROUP_GAP;
			rowHeight = 0;
		}
		for (const [name, p] of block.positions) {
			placed.set(name, { x: MARGIN + x + p.x, y: MARGIN + y + p.y });
		}
		x += block.width + GROUP_GAP;
		rowHeight = Math.max(rowHeight, block.height);
	}

	const nodes: ErNode[] = diagram.tables.map((table) => {
		const { width, height } = size(table);
		const columns = shown.get(table.name)!;
		const group = groups.get(table.name) ?? null;
		return {
			id: table.name,
			type: 'table',
			position: placed.get(table.name) ?? { x: MARGIN, y: MARGIN },
			width,
			height,
			data: {
				name: table.name,
				kind: table.kind,
				columns,
				hiddenColumns: table.columns.length - columns.length,
				compact,
				foreignKeyColumns: [...(fkColumns.get(table.name) ?? [])],
				group,
				hue: group ? hues.get(group)! : null
			}
		};
	});

	const byId = new Map(nodes.map((n) => [n.id, n]));
	return {
		nodes,
		edges: edges.map((e) => routeEdge(e, byId.get(e.source)!, byId.get(e.target)!))
	};
}
