import { describe, expect, it } from 'vitest';
import {
	OrthogonalRouter,
	midpoint,
	roundedPath,
	simplify,
	snapEnds,
	type Port,
	type Rect
} from './er-route.js';

const crosses = (a: { x: number; y: number }, b: { x: number; y: number }, r: Rect) =>
	Math.max(a.x, b.x) > r.x &&
	Math.min(a.x, b.x) < r.x + r.width &&
	Math.max(a.y, b.y) > r.y &&
	Math.min(a.y, b.y) < r.y + r.height;

describe('OrthogonalRouter', () => {
	const left: Rect = { x: 0, y: 0, width: 100, height: 60 };
	const right: Rect = { x: 400, y: 0, width: 100, height: 60 };
	// A wall between the two, taller than both.
	const wall: Rect = { x: 200, y: -100, width: 100, height: 260 };

	it('goes straight across when nothing is in the way', () => {
		const router = new OrthogonalRouter([left, right], [30]);
		const route = router.route(
			[{ rect: left, y: 30, side: 'right' }],
			[{ rect: right, y: 30, side: 'left' }]
		)!;
		expect(route.points).toEqual([
			{ x: 100, y: 30 },
			{ x: 400, y: 30 }
		]);
	});

	it('goes around a table in the way', () => {
		const router = new OrthogonalRouter([left, right, wall], [30]);
		const route = router.route(
			[{ rect: left, y: 30, side: 'right' }],
			[{ rect: right, y: 30, side: 'left' }]
		)!;
		expect(route).not.toBeNull();
		for (let k = 1; k < route.points.length; k++) {
			const a = route.points[k - 1];
			const b = route.points[k];
			expect(a.x === b.x || a.y === b.y).toBe(true);
			for (const r of [left, right, wall]) expect(crosses(a, b, r)).toBe(false);
		}
	});

	it('picks the cheaper side when both are offered', () => {
		const router = new OrthogonalRouter([left, right], [30]);
		const both = (rect: Rect, y: number): Port[] => [
			{ rect, y, side: 'left' },
			{ rect, y, side: 'right' }
		];
		const route = router.route(both(left, 30), both(right, 30))!;
		expect(route.sourceSide).toBe('right');
		expect(route.targetSide).toBe('left');
	});
});

describe('simplify', () => {
	it('drops points in the middle of straight runs and duplicates', () => {
		expect(
			simplify([
				{ x: 0, y: 0 },
				{ x: 5, y: 0 },
				{ x: 5, y: 0 },
				{ x: 10, y: 0 },
				{ x: 10, y: 10 }
			])
		).toEqual([
			{ x: 0, y: 0 },
			{ x: 10, y: 0 },
			{ x: 10, y: 10 }
		]);
	});
});

describe('roundedPath', () => {
	it('rounds corners with a curve', () => {
		const d = roundedPath([
			{ x: 0, y: 0 },
			{ x: 100, y: 0 },
			{ x: 100, y: 100 }
		]);
		expect(d).toBe('M0,0 L92,0 Q100,0 100,8 L100,100');
	});
});

describe('midpoint', () => {
	it('is halfway along the line', () => {
		expect(
			midpoint([
				{ x: 0, y: 0 },
				{ x: 100, y: 0 },
				{ x: 100, y: 100 }
			])
		).toEqual({ x: 100, y: 0 });
	});
});

describe('snapEnds', () => {
	const route = [
		{ x: 0, y: 10 },
		{ x: 50, y: 10 },
		{ x: 50, y: 80 },
		{ x: 100, y: 80 }
	];

	it('moves the end runs onto the drawn handles and keeps corners square', () => {
		expect(snapEnds(route, { x: 2, y: 12 }, { x: 98, y: 79 })).toEqual([
			{ x: 2, y: 12 },
			{ x: 50, y: 12 },
			{ x: 50, y: 79 },
			{ x: 98, y: 79 }
		]);
	});

	it('gives up when the tables have moved away', () => {
		expect(snapEnds(route, { x: 300, y: 10 }, { x: 100, y: 80 })).toBeNull();
	});
});
