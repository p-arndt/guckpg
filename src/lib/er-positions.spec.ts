import { describe, expect, it } from 'vitest';
import {
	applyPositions,
	clearPositions,
	loadPositions,
	positionsKey,
	savePositions
} from './er-positions.js';

function memoryStore() {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k)
	};
}

describe('positions', () => {
	const key = positionsKey('conn', 'db', 'public', 'keys');

	it('keys by connection, database, schema and column mode', () => {
		expect(key).not.toBe(positionsKey('conn', 'db', 'public', 'all'));
		expect(key).not.toBe(positionsKey('conn', 'db', 'other', 'keys'));
	});

	it('round-trips rounded positions', () => {
		const store = memoryStore();
		savePositions(store, key, [{ id: 'a', position: { x: 10.4, y: 20.6 } }]);
		expect(loadPositions(store, key)).toEqual({ a: { x: 10, y: 21 } });
	});

	it('survives missing and broken data', () => {
		const store = memoryStore();
		expect(loadPositions(store, key)).toEqual({});
		store.setItem(key, 'not json');
		expect(loadPositions(store, key)).toEqual({});
		store.setItem(key, JSON.stringify({ a: { x: 'no' }, b: { x: 1, y: 2 } }));
		expect(loadPositions(store, key)).toEqual({ b: { x: 1, y: 2 } });
	});

	it('clears', () => {
		const store = memoryStore();
		savePositions(store, key, [{ id: 'a', position: { x: 1, y: 2 } }]);
		clearPositions(store, key);
		expect(loadPositions(store, key)).toEqual({});
	});

	it('applies saved positions and leaves new tables where the layout put them', () => {
		const nodes = [
			{ id: 'a', position: { x: 0, y: 0 } },
			{ id: 'b', position: { x: 5, y: 5 } }
		];
		expect(applyPositions(nodes, { a: { x: 100, y: 200 }, gone: { x: 1, y: 1 } })).toEqual([
			{ id: 'a', position: { x: 100, y: 200 } },
			{ id: 'b', position: { x: 5, y: 5 } }
		]);
	});
});
