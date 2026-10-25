import { describe, expect, it } from 'vitest';
import type { Column, Diagram } from './api.js';
import {
	HEADER_PX,
	ROW_PX,
	layoutDiagram,
	focusDiagram,
	handleColumn,
	keyColumns,
	nodeSize,
	parseColumnMode,
	portY,
	routeDiagram,
	tableGroups,
	TABLE_HANDLE,
	parseEdgeStyle,
	packColumns,
	pickHandles
} from './er-layout.js';

const col = (name: string, primaryKey = false): Column => ({
	name,
	dataType: 'integer',
	notNull: primaryKey,
	default: null,
	primaryKey
});

const diagram: Diagram = {
	tables: [
		{ name: 'customers', kind: 'table', columns: [col('id', true), col('name')] },
		{ name: 'orders', kind: 'table', columns: [col('id', true), col('customer_id')] },
		{
			name: 'employees',
			kind: 'table',
			columns: [col('id', true), col('manager_id')]
		},
		{ name: 'lonely', kind: 'view', columns: [] }
	],
	foreignKeys: [
		{
			name: 'orders_customer_fk',
			fromTable: 'orders',
			fromColumns: ['customer_id'],
			toTable: 'customers',
			toColumns: ['id']
		},
		{
			name: 'employees_manager_fk',
			fromTable: 'employees',
			fromColumns: ['manager_id'],
			toTable: 'employees',
			toColumns: ['id']
		}
	]
};

describe('nodeSize', () => {
	it('grows with columns and clamps width', () => {
		const small = nodeSize([col('id')], 't');
		expect(small.width).toBe(200);
		expect(small.height).toBe(HEADER_PX + ROW_PX + 8);
		expect(nodeSize([{ ...col('x'.repeat(200)) }], 't').width).toBe(380);
	});
});

describe('layoutDiagram', () => {
	const { nodes, edges } = layoutDiagram(diagram);
	const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));

	it('creates one node per table', () => {
		expect(nodes.map((n) => n.id).sort()).toEqual(['customers', 'employees', 'lonely', 'orders']);
	});

	it('connects the FK column to the referenced column', () => {
		const e = edges.find((e) => e.id === 'orders.orders_customer_fk');
		expect(e).toMatchObject({
			source: 'orders',
			target: 'customers',
			sourceHandle: expect.stringMatching(/^customer_id:out:(left|right)$/),
			targetHandle: expect.stringMatching(/^id:in:(left|right)$/)
		});
	});

	it('docks edges on the sides that face each other', () => {
		const e = edges.find((e) => e.id === 'orders.orders_customer_fk')!;
		const { sourceSide, targetSide } = pickHandles(byId.orders, byId.customers);
		expect(e.sourceHandle).toBe(`customer_id:out:${sourceSide}`);
		expect(e.targetHandle).toBe(`id:in:${targetSide}`);
	});

	it('keeps self references as edges', () => {
		expect(edges.some((e) => e.source === 'employees' && e.target === 'employees')).toBe(true);
	});

	it('marks foreign key columns', () => {
		expect(byId.orders.data.foreignKeyColumns).toEqual(['customer_id']);
		expect(byId.customers.data.foreignKeyColumns).toEqual([]);
	});

	it('does not overlap nodes', () => {
		for (const a of nodes) {
			for (const b of nodes) {
				if (a === b) continue;
				const apart =
					a.position.x + a.width <= b.position.x ||
					b.position.x + b.width <= a.position.x ||
					a.position.y + a.height <= b.position.y ||
					b.position.y + b.height <= a.position.y;
				expect(apart, `${a.id} overlaps ${b.id}`).toBe(true);
			}
		}
	});
});

type Nodes = ReturnType<typeof layoutDiagram>['nodes'];

function expectNoOverlap(nodes: Nodes) {
	for (const a of nodes) {
		for (const b of nodes) {
			if (a === b) continue;
			const apart =
				a.position.x + a.width <= b.position.x ||
				b.position.x + b.width <= a.position.x ||
				a.position.y + a.height <= b.position.y ||
				b.position.y + b.height <= a.position.y;
			expect(apart, `${a.id} overlaps ${b.id}`).toBe(true);
		}
	}
}

function bounds(nodes: Nodes) {
	const minX = Math.min(...nodes.map((n) => n.position.x));
	const maxX = Math.max(...nodes.map((n) => n.position.x + n.width));
	const minY = Math.min(...nodes.map((n) => n.position.y));
	const maxY = Math.max(...nodes.map((n) => n.position.y + n.height));
	return { width: maxX - minX, height: maxY - minY };
}

describe('layout of a star schema', () => {
	// Sixty tables pointing at one hub, plus twenty without relations: the
	// shape that used to come out as a single tall column.
	const children = Array.from({ length: 60 }, (_, i) => ({
		name: `child_${String(i).padStart(2, '0')}`,
		kind: 'table' as const,
		columns: [col('id', true), col('user_id'), col('a'), col('b')]
	}));
	const loners = Array.from({ length: 20 }, (_, i) => ({
		name: `loner_${i}`,
		kind: 'table' as const,
		columns: [col('id', true)]
	}));
	const star: Diagram = {
		tables: [{ name: 'users', kind: 'table', columns: [col('id', true)] }, ...children, ...loners],
		foreignKeys: children.map((c) => ({
			name: `${c.name}_fk`,
			fromTable: c.name,
			fromColumns: ['user_id'],
			toTable: 'users',
			toColumns: ['id']
		}))
	};
	const { nodes } = layoutDiagram(star);
	const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));

	it('stays roughly screen shaped', () => {
		const { width, height } = bounds(nodes);
		expect(height / width).toBeLessThan(1.5);
		expect(width / height).toBeLessThan(4);
	});

	it('puts the hub in the middle of the tables referencing it', () => {
		const center = (n: (typeof nodes)[number]) => ({
			x: n.position.x + n.width / 2,
			y: n.position.y + n.height / 2
		});
		const hub = center(byId.users);
		const around = children.map((c) => center(byId[c.name]));
		expect(around.some((p) => p.x < hub.x)).toBe(true);
		expect(around.some((p) => p.x > hub.x)).toBe(true);
		expect(around.some((p) => p.y < hub.y)).toBe(true);
		expect(around.some((p) => p.y > hub.y)).toBe(true);
	});

	it('puts tables without relations after the related ones', () => {
		const related = [byId.users, ...children.map((c) => byId[c.name])];
		const right = Math.max(...related.map((n) => n.position.x + n.width));
		const bottom = Math.max(...related.map((n) => n.position.y + n.height));
		for (const l of loners) {
			const p = byId[l.name].position;
			expect(p.x >= right || p.y >= bottom, `${l.name} is not after the related tables`).toBe(true);
		}
	});

	it('does not overlap', () => {
		const sorted = [...nodes].sort((a, b) => a.position.x - b.position.x);
		for (let i = 0; i < sorted.length; i++) {
			for (let j = i + 1; j < sorted.length; j++) {
				const a = sorted[i];
				const b = sorted[j];
				if (b.position.x >= a.position.x + a.width) break;
				const apart =
					a.position.y + a.height <= b.position.y || b.position.y + b.height <= a.position.y;
				expect(apart, `${a.id} overlaps ${b.id}`).toBe(true);
			}
		}
	});
});

describe('packColumns', () => {
	const box = (name: string, height: number) => ({ name, width: 100, height });

	it('wraps when the column is full', () => {
		const cols = packColumns([box('a', 100), box('b', 100), box('c', 100)], 250);
		expect(cols.map((c) => c.map((b) => b.name))).toEqual([['a', 'b'], ['c']]);
	});

	it('puts a box taller than the limit into its own column', () => {
		const cols = packColumns([box('a', 500), box('b', 10)], 100);
		expect(cols.map((c) => c.map((b) => b.name))).toEqual([['a'], ['b']]);
	});
});

describe('layout of a mixed schema', () => {
	// Tables of different widths, a hub, a few chains and several unrelated
	// groups: the shape of a real application schema.
	const wide = (name: string, n: number) => ({
		name,
		kind: 'table' as const,
		columns: [
			col('id', true),
			col('account_id'),
			col('parent_id'),
			...Array.from({ length: n }, (_, i) =>
				col(`${'some_long_column_name_'.slice(0, 4 + ((i * 7) % 18))}${i}`)
			)
		]
	});
	const tables = [wide('accounts', 12)];
	const foreignKeys: Diagram['foreignKeys'] = [];
	const fk = (from: string, column: string, to: string) =>
		foreignKeys.push({
			name: `${from}_${column}_fk`,
			fromTable: from,
			fromColumns: [column],
			toTable: to,
			toColumns: ['id']
		});
	for (let i = 0; i < 40; i++) {
		tables.push(wide(`t${i}`, 2 + (i % 9)));
		fk(`t${i}`, 'account_id', 'accounts');
		if (i % 4 === 1) fk(`t${i}`, 'parent_id', `t${i - 1}`);
	}
	for (let g = 0; g < 6; g++) {
		tables.push(wide(`g${g}_a`, 3), wide(`g${g}_b`, 5));
		fk(`g${g}_b`, 'parent_id', `g${g}_a`);
	}
	for (let i = 0; i < 10; i++) tables.push(wide(`chain${i}`, 2));
	for (let i = 1; i < 10; i++) fk(`chain${i}`, 'parent_id', `chain${i - 1}`);

	const { nodes } = layoutDiagram({ tables, foreignKeys });

	it('stays roughly screen shaped', () => {
		const { width, height } = bounds(nodes);
		expect(width / height).toBeLessThan(3);
		expect(height / width).toBeLessThan(1.5);
	});

	it('does not overlap', () => expectNoOverlap(nodes));
});

describe('pickHandles', () => {
	const at = (x: number) => ({ position: { x, y: 0 }, width: 100 });

	it('uses the facing sides of tables next to each other', () => {
		expect(pickHandles(at(0), at(300))).toEqual({ sourceSide: 'right', targetSide: 'left' });
		expect(pickHandles(at(300), at(0))).toEqual({ sourceSide: 'left', targetSide: 'right' });
	});

	it('uses the right side of both for stacked tables', () => {
		expect(pickHandles(at(0), at(50))).toEqual({ sourceSide: 'right', targetSide: 'right' });
	});
});

describe('parseEdgeStyle', () => {
	it('keeps a known style', () => {
		expect(parseEdgeStyle('straight')).toBe('straight');
		expect(parseEdgeStyle('default')).toBe('default');
	});

	it('falls back to routed lines', () => {
		expect(parseEdgeStyle(null)).toBe('routed');
		expect(parseEdgeStyle('bogus')).toBe('routed');
	});
});

describe('layout of two linked hubs', () => {
	const hubs = ['left_hub', 'right_hub'];
	const tables: Diagram['tables'] = hubs.map((name) => ({
		name,
		kind: 'table',
		columns: [col('id', true), col('other_id')]
	}));
	const foreignKeys: Diagram['foreignKeys'] = [
		{
			name: 'hubs_fk',
			fromTable: 'left_hub',
			fromColumns: ['other_id'],
			toTable: 'right_hub',
			toColumns: ['id']
		}
	];
	for (const hub of hubs) {
		for (let i = 0; i < 12; i++) {
			const name = `${hub}_child_${i}`;
			tables.push({ name, kind: 'table', columns: [col('id', true), col('hub_id'), col('x')] });
			foreignKeys.push({
				name: `${name}_fk`,
				fromTable: name,
				fromColumns: ['hub_id'],
				toTable: hub,
				toColumns: ['id']
			});
		}
	}
	const { nodes } = layoutDiagram({ tables, foreignKeys });
	const center = (id: string) => {
		const n = nodes.find((n) => n.id === id)!;
		return { x: n.position.x + n.width / 2, y: n.position.y + n.height / 2 };
	};
	const distance = (a: string, b: string) => {
		const p = center(a);
		const q = center(b);
		return Math.hypot(p.x - q.x, p.y - q.y);
	};

	it('keeps every table closer to its own hub than to the other one', () => {
		for (const [own, other] of [hubs, [...hubs].reverse()]) {
			for (let i = 0; i < 12; i++) {
				const child = `${own}_child_${i}`;
				expect(distance(child, own), child).toBeLessThan(distance(child, other));
			}
		}
	});

	it('does not overlap', () => expectNoOverlap(nodes));
});

describe('nodeSize modes', () => {
	it('adds a row for hidden columns', () => {
		expect(nodeSize([col('id')], 't', 3).height).toBe(nodeSize([col('id'), col('x')], 't').height);
	});

	it('is only the header when compact', () => {
		expect(nodeSize([], 't', 0, true).height).toBe(HEADER_PX + 2);
	});
});

describe('parseColumnMode', () => {
	it('keeps a known mode and falls back to keys', () => {
		expect(parseColumnMode('all')).toBe('all');
		expect(parseColumnMode('none')).toBe('none');
		expect(parseColumnMode(null)).toBe('keys');
		expect(parseColumnMode('bogus')).toBe('keys');
	});
});

describe('column modes', () => {
	const wide: Diagram = {
		tables: [
			{
				name: 'orders',
				kind: 'table',
				columns: [col('id', true), col('customer_id'), col('note'), col('total')]
			},
			{ name: 'customers', kind: 'table', columns: [col('id', true), col('name')] }
		],
		foreignKeys: [
			{
				name: 'fk',
				fromTable: 'orders',
				fromColumns: ['customer_id'],
				toTable: 'customers',
				toColumns: ['id']
			}
		]
	};

	it('collects primary keys and both ends of foreign keys', () => {
		const keys = keyColumns(wide);
		expect([...keys.get('orders')!].sort()).toEqual(['customer_id', 'id']);
		expect([...keys.get('customers')!]).toEqual(['id']);
	});

	it('shows only key columns in keys mode', () => {
		const orders = layoutDiagram(wide, 'keys').nodes.find((n) => n.id === 'orders')!;
		expect(orders.data.columns.map((c) => c.name)).toEqual(['id', 'customer_id']);
		expect(orders.data.hiddenColumns).toBe(2);
		expect(orders.data.compact).toBe(false);
	});

	it('shows only headers in none mode and docks edges there', () => {
		const { nodes, edges } = layoutDiagram(wide, 'none');
		expect(nodes.every((n) => n.data.compact && n.height === HEADER_PX + 2)).toBe(true);
		expect(edges[0].sourceHandle).toMatch(new RegExp(`^${TABLE_HANDLE}:out:`));
		expect(edges[0].targetHandle).toMatch(new RegExp(`^${TABLE_HANDLE}:in:`));
	});

	it('gets smaller with fewer columns', () => {
		const height = (mode: 'all' | 'keys') =>
			layoutDiagram(wide, mode).nodes.find((n) => n.id === 'orders')!.height;
		expect(height('keys')).toBeLessThan(height('all'));
	});
});

describe('ports', () => {
	const node = {
		position: { x: 100, y: 200 },
		width: 200,
		height: 100,
		data: { columns: [col('id', true), col('name')], compact: false }
	};

	it('puts a column port in the middle of its row', () => {
		expect(portY(node, 'name')).toBe(200 + 1 + HEADER_PX + 3 + ROW_PX + ROW_PX / 2);
	});

	it('falls back to the header for hidden columns', () => {
		expect(handleColumn(node, 'hidden')).toBe(TABLE_HANDLE);
		expect(portY(node, 'hidden')).toBe(200 + 1 + HEADER_PX / 2);
	});
});

describe('tableGroups', () => {
	it('groups by the first word, plurals included', () => {
		const groups = tableGroups([
			'cliques',
			'clique_log',
			'clique_members',
			'casino_dice',
			'casino_roulette',
			'identities',
			'identity_links',
			'accounts',
			'world_clock'
		]);
		expect(groups.get('cliques')).toBe('clique');
		expect(groups.get('clique_log')).toBe('clique');
		expect(groups.get('casino_dice')).toBe('casino');
		expect(groups.get('identities')).toBe('identity');
		expect(groups.get('identity_links')).toBe('identity');
	});

	it('leaves tables without a shared prefix alone', () => {
		const groups = tableGroups(['accounts', 'world_clock', 'business']);
		expect(groups.size).toBe(0);
	});

	it('colours every member of a group alike', () => {
		const { nodes } = layoutDiagram({
			tables: ['casino_dice', 'casino_roulette', 'lonely'].map((name) => ({
				name,
				kind: 'table' as const,
				columns: [col('id', true)]
			})),
			foreignKeys: []
		});
		const hue = (id: string) => nodes.find((n) => n.id === id)!.data.hue;
		expect(hue('casino_dice')).not.toBeNull();
		expect(hue('casino_dice')).toBe(hue('casino_roulette'));
		expect(hue('lonely')).toBeNull();
	});
});

describe('focusDiagram', () => {
	it('keeps the table and its direct neighbours in both directions', () => {
		const chain: Diagram = {
			tables: ['a', 'b', 'c', 'd'].map((name) => ({
				name,
				kind: 'table' as const,
				columns: [col('id', true), col('ref')]
			})),
			foreignKeys: [
				{ name: 'ab', fromTable: 'a', fromColumns: ['ref'], toTable: 'b', toColumns: ['id'] },
				{ name: 'bc', fromTable: 'b', fromColumns: ['ref'], toTable: 'c', toColumns: ['id'] },
				{ name: 'cd', fromTable: 'c', fromColumns: ['ref'], toTable: 'd', toColumns: ['id'] }
			]
		};
		const focus = focusDiagram(chain, 'b');
		expect(focus.tables.map((t) => t.name)).toEqual(['a', 'b', 'c']);
		expect(focus.foreignKeys.map((fk) => fk.name)).toEqual(['ab', 'bc']);
	});
});

describe('routeDiagram', () => {
	const tables = Array.from({ length: 12 }, (_, i) => ({
		name: `t${i}`,
		kind: 'table' as const,
		columns: [col('id', true), col('ref'), col('other')]
	}));
	const foreignKeys = tables.slice(1).map((t, i) => ({
		name: `${t.name}_fk`,
		fromTable: t.name,
		fromColumns: ['ref'],
		toTable: i % 3 === 0 ? 't0' : `t${i}`,
		toColumns: ['id']
	}));
	const { nodes, edges } = layoutDiagram({ tables, foreignKeys }, 'all');
	const routes = routeDiagram(nodes, edges);

	it('routes every edge with straight horizontal and vertical runs', () => {
		expect(routes.size).toBe(edges.length);
		for (const route of routes.values()) {
			for (let k = 1; k < route.points.length; k++) {
				const a = route.points[k - 1];
				const b = route.points[k];
				expect(a.x === b.x || a.y === b.y).toBe(true);
			}
		}
	});

	it('never runs through a table', () => {
		for (const [id, route] of routes) {
			for (let k = 1; k < route.points.length; k++) {
				const a = route.points[k - 1];
				const b = route.points[k];
				for (const n of nodes) {
					const inside =
						Math.max(a.x, b.x) > n.position.x + 1 &&
						Math.min(a.x, b.x) < n.position.x + n.width - 1 &&
						Math.max(a.y, b.y) > n.position.y + 1 &&
						Math.min(a.y, b.y) < n.position.y + n.height - 1;
					expect(inside, `${id} crosses ${n.id}`).toBe(false);
				}
			}
		}
	});

	it('starts and ends on the column rows', () => {
		const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
		for (const e of edges) {
			const r = routes.get(e.id)!;
			expect(r.points[0].y).toBe(portY(byId[e.source], 'ref'));
			expect(r.points[r.points.length - 1].y).toBe(portY(byId[e.target], 'id'));
		}
	});
});

describe('name groups in the layout', () => {
	// Two groups hanging off one hub: members of a group should sit together.
	const names = [
		...Array.from({ length: 6 }, (_, i) => `casino_${i}`),
		...Array.from({ length: 6 }, (_, i) => `push_${i}`)
	];
	const tables = [
		{ name: 'hub', kind: 'table' as const, columns: [col('id', true)] },
		...names.map((name) => ({
			name,
			kind: 'table' as const,
			columns: [col('id', true), col('hub_id')]
		}))
	];
	const foreignKeys = names.map((name) => ({
		name: `${name}_fk`,
		fromTable: name,
		fromColumns: ['hub_id'],
		toTable: 'hub',
		toColumns: ['id']
	}));
	const { nodes } = layoutDiagram({ tables, foreignKeys });
	const center = (id: string) => {
		const n = nodes.find((n) => n.id === id)!;
		return { x: n.position.x + n.width / 2, y: n.position.y + n.height / 2 };
	};
	const spread = (prefix: string, other: string) => {
		const own = names.filter((n) => n.startsWith(prefix));
		const rest = names.filter((n) => n.startsWith(other));
		const dist = (a: string, b: string) =>
			Math.hypot(center(a).x - center(b).x, center(a).y - center(b).y);
		const mean = (pairs: [string, string][]) =>
			pairs.reduce((s, [a, b]) => s + dist(a, b), 0) / pairs.length;
		const within = own.flatMap((a) =>
			own.filter((b) => b !== a).map((b) => [a, b] as [string, string])
		);
		const across = own.flatMap((a) => rest.map((b) => [a, b] as [string, string]));
		return { within: mean(within), across: mean(across) };
	};

	it('keeps tables of one prefix closer to each other than to the other prefix', () => {
		const casino = spread('casino_', 'push_');
		expect(casino.within).toBeLessThan(casino.across);
		const push = spread('push_', 'casino_');
		expect(push.within).toBeLessThan(push.across);
	});
});
