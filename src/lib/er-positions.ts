import type { ColumnMode } from './er-layout.js';

export type Positions = Record<string, { x: number; y: number }>;

type Store = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

/**
 * Hand-placed positions belong to one schema and one column mode: tables
 * change size with the mode, so positions from another mode would overlap.
 */
export function positionsKey(
	connectionId: string,
	database: string,
	schema: string,
	mode: ColumnMode
): string {
	return `guckpg.er.positions:${connectionId}:${database}:${schema}:${mode}`;
}

export function loadPositions(store: Store, key: string): Positions {
	try {
		const parsed: unknown = JSON.parse(store.getItem(key) ?? '{}');
		if (!parsed || typeof parsed !== 'object') return {};
		const out: Positions = {};
		for (const [name, p] of Object.entries(parsed)) {
			if (p && typeof p.x === 'number' && typeof p.y === 'number') out[name] = { x: p.x, y: p.y };
		}
		return out;
	} catch {
		return {};
	}
}

export function savePositions(
	store: Store,
	key: string,
	nodes: { id: string; position: { x: number; y: number } }[]
) {
	const positions: Positions = {};
	for (const n of nodes) {
		positions[n.id] = { x: Math.round(n.position.x), y: Math.round(n.position.y) };
	}
	store.setItem(key, JSON.stringify(positions));
}

export function clearPositions(store: Store, key: string) {
	store.removeItem(key);
}

/** Saved positions win over the computed layout; new tables keep their computed place. */
export function applyPositions<T extends { id: string; position: { x: number; y: number } }>(
	nodes: T[],
	saved: Positions
): T[] {
	return nodes.map((n) => (saved[n.id] ? { ...n, position: saved[n.id] } : n));
}
