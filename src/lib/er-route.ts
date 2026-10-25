/**
 * Orthogonal edge routing around tables. The space between the tables is
 * turned into a sparse grid: its lines are the table borders (kept at a small
 * distance), the lanes halfway between them, and the rows of the ports. A
 * shortest path search on that grid that charges extra for every bend gives
 * lines with few corners that never cross a table.
 */

export type Side = 'left' | 'right';

export interface Rect {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface Point {
	x: number;
	y: number;
}

/** Where a line may leave or enter a table: a y on its left or right border. */
export interface Port {
	rect: Rect;
	y: number;
	side: Side;
}

export interface Route {
	points: Point[];
	sourceSide: Side;
	targetSide: Side;
}

/** Distance kept between lines and tables. */
const CLEARANCE = 16;
/** Cost of a corner, in pixels of line length. */
const BEND_COST = 60;
/** Cost of running along a line another edge already uses, per pixel. */
const SHARED_COST = 0.3;

const RIGHT = 0;
const LEFT = 1;
const DOWN = 2;
const UP = 3;

function uniqueSorted(values: number[]): number[] {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted.filter((v, i) => i === 0 || v - sorted[i - 1] > 0.5);
}

/** With lanes halfway between neighbouring lines that are far enough apart. */
function withLanes(values: number[]): number[] {
	const lines = uniqueSorted(values);
	const lanes: number[] = [];
	for (let i = 1; i < lines.length; i++) {
		if (lines[i] - lines[i - 1] > 2 * CLEARANCE) lanes.push((lines[i] + lines[i - 1]) / 2);
	}
	return uniqueSorted([...lines, ...lanes]);
}

/** First index with `values[i] > v` (binary search). */
function upperBound(values: number[], v: number): number {
	let lo = 0;
	let hi = values.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (values[mid] <= v) lo = mid + 1;
		else hi = mid;
	}
	return lo;
}

/** First index with `values[i] >= v`. */
function lowerBound(values: number[], v: number): number {
	let lo = 0;
	let hi = values.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (values[mid] < v) lo = mid + 1;
		else hi = mid;
	}
	return lo;
}

class MinHeap {
	private keys: number[] = [];
	private values: number[] = [];

	get size() {
		return this.keys.length;
	}

	push(key: number, value: number) {
		const { keys, values } = this;
		let i = keys.length;
		keys.push(key);
		values.push(value);
		while (i > 0) {
			const parent = (i - 1) >> 1;
			if (keys[parent] <= key) break;
			keys[i] = keys[parent];
			values[i] = values[parent];
			i = parent;
		}
		keys[i] = key;
		values[i] = value;
	}

	peekKey() {
		return this.keys[0];
	}

	pop(): number {
		const { keys, values } = this;
		const top = values[0];
		const lastKey = keys.pop()!;
		const lastValue = values.pop()!;
		if (keys.length) {
			let i = 0;
			for (;;) {
				const l = 2 * i + 1;
				const r = l + 1;
				let m = i;
				let mKey = lastKey;
				if (l < keys.length && keys[l] < mKey) {
					m = l;
					mKey = keys[l];
				}
				if (r < keys.length && keys[r] < mKey) m = r;
				if (m === i) break;
				keys[i] = keys[m];
				values[i] = values[m];
				i = m;
			}
			keys[i] = lastKey;
			values[i] = lastValue;
		}
		return top;
	}
}

const stubX = (port: Port) =>
	port.side === 'right' ? port.rect.x + port.rect.width + CLEARANCE : port.rect.x - CLEARANCE;
const borderX = (port: Port) =>
	port.side === 'right' ? port.rect.x + port.rect.width : port.rect.x;

export class OrthogonalRouter {
	private xs: number[];
	private ys: number[];
	/** Per grid point: may a line go one step right / down from here. */
	private openRight: Uint8Array;
	private openDown: Uint8Array;
	/** Per grid segment (point index * 2 + 0 for right, + 1 for down): edges already on it. */
	private usage: Uint16Array;

	constructor(obstacles: Rect[], portYs: number[]) {
		const inflated = obstacles.map((r) => ({
			l: r.x - CLEARANCE,
			r: r.x + r.width + CLEARANCE,
			t: r.y - CLEARANCE,
			b: r.y + r.height + CLEARANCE
		}));
		this.xs = withLanes(inflated.flatMap((o) => [o.l, o.r]));
		this.ys = withLanes([...inflated.flatMap((o) => [o.t, o.b]), ...portYs]);
		const nx = this.xs.length;
		const ny = this.ys.length;
		this.openRight = new Uint8Array(nx * ny).fill(1);
		this.openDown = new Uint8Array(nx * ny).fill(1);
		this.usage = new Uint16Array(nx * ny * 2);
		for (let i = 0; i < nx; i++) this.openDown[i + (ny - 1) * nx] = 0;
		for (let j = 0; j < ny; j++) this.openRight[nx - 1 + j * nx] = 0;

		const { xs, ys } = this;
		for (const o of inflated) {
			// Horizontal steps whose middle lies inside the table, on rows strictly inside it.
			const j0 = upperBound(ys, o.t);
			const j1 = lowerBound(ys, o.b);
			const i0 = Math.max(0, upperBound(xs, o.l) - 1);
			const i1 = lowerBound(xs, o.r);
			for (let j = j0; j < j1; j++) {
				for (let i = i0; i < i1; i++) {
					const mid = (xs[i] + xs[i + 1]) / 2;
					if (mid > o.l && mid < o.r) this.openRight[i + j * nx] = 0;
				}
			}
			// Vertical steps likewise, on columns strictly inside it.
			const ci0 = upperBound(xs, o.l);
			const ci1 = lowerBound(xs, o.r);
			const rj0 = Math.max(0, upperBound(ys, o.t) - 1);
			const rj1 = lowerBound(ys, o.b);
			for (let i = ci0; i < ci1; i++) {
				for (let j = rj0; j < rj1; j++) {
					const mid = (ys[j] + ys[j + 1]) / 2;
					if (mid > o.t && mid < o.b) this.openDown[i + j * nx] = 0;
				}
			}
		}
	}

	private indexOf(p: Point): number {
		const i = lowerBound(this.xs, p.x - 0.5);
		const j = lowerBound(this.ys, p.y - 0.5);
		if (Math.abs(this.xs[i] - p.x) > 0.5 || Math.abs(this.ys[j] - p.y) > 0.5) return -1;
		return i + j * this.xs.length;
	}

	/**
	 * Cheapest line from any source port to any target port, or null when the
	 * tables wall every port in. Later routes avoid running on top of earlier ones.
	 */
	route(sources: Port[], targets: Port[]): Route | null {
		const { xs, ys, openRight, openDown, usage } = this;
		const nx = xs.length;
		const states = nx * ys.length * 4;
		const cost = new Float64Array(states).fill(Infinity);
		const from = new Int32Array(states).fill(-1);
		const heap = new MinHeap();

		const goals = new Map<number, { port: Port; dir: number }>();
		for (const t of targets) {
			const idx = this.indexOf({ x: stubX(t), y: t.y });
			if (idx >= 0) goals.set(idx, { port: t, dir: t.side === 'left' ? RIGHT : LEFT });
		}
		if (!goals.size) return null;
		const goalPoints = [...goals.keys()].map((idx) => ({ x: xs[idx % nx], y: ys[(idx / nx) | 0] }));
		const h = (idx: number) => {
			const x = xs[idx % nx];
			const y = ys[(idx / nx) | 0];
			let best = Infinity;
			for (const g of goalPoints) best = Math.min(best, Math.abs(g.x - x) + Math.abs(g.y - y));
			return best;
		};

		const starts = new Map<number, Port>();
		for (const s of sources) {
			const idx = this.indexOf({ x: stubX(s), y: s.y });
			if (idx < 0) continue;
			starts.set(idx, s);
			const state = idx * 4 + (s.side === 'right' ? RIGHT : LEFT);
			cost[state] = 0;
			heap.push(h(idx), state);
		}

		let best = Infinity;
		let bestState = -1;
		while (heap.size) {
			if (heap.peekKey() >= best) break;
			const state = heap.pop();
			const idx = state >> 2;
			const dir = state & 3;
			const g = cost[state];
			const goal = goals.get(idx);
			if (goal) {
				const total = g + (dir === goal.dir ? 0 : BEND_COST);
				if (total < best) {
					best = total;
					bestState = state;
				}
			}
			const i = idx % nx;
			const j = (idx / nx) | 0;
			for (let d = 0; d < 4; d++) {
				if ((dir ^ d) === 1 && dir >> 1 === d >> 1) continue; // no U-turns
				let next = -1;
				let length = 0;
				let segment = 0;
				if (d === RIGHT && openRight[idx]) {
					next = idx + 1;
					length = xs[i + 1] - xs[i];
					segment = idx * 2;
				} else if (d === LEFT && i > 0 && openRight[idx - 1]) {
					next = idx - 1;
					length = xs[i] - xs[i - 1];
					segment = (idx - 1) * 2;
				} else if (d === DOWN && openDown[idx]) {
					next = idx + nx;
					length = ys[j + 1] - ys[j];
					segment = idx * 2 + 1;
				} else if (d === UP && j > 0 && openDown[idx - nx]) {
					next = idx - nx;
					length = ys[j] - ys[j - 1];
					segment = (idx - nx) * 2 + 1;
				}
				if (next < 0) continue;
				const c =
					g +
					length * (1 + SHARED_COST * Math.min(usage[segment], 3)) +
					(d === dir ? 0 : BEND_COST);
				const nextState = next * 4 + d;
				if (c < cost[nextState]) {
					cost[nextState] = c;
					from[nextState] = state;
					heap.push(c + h(next), nextState);
				}
			}
		}
		if (bestState < 0) return null;

		const cells: number[] = [];
		for (let s = bestState; s >= 0; s = from[s]) cells.push(s >> 2);
		cells.reverse();
		for (let k = 1; k < cells.length; k++) {
			const a = cells[k - 1];
			const b = cells[k];
			const lo = Math.min(a, b);
			usage[lo * 2 + (Math.abs(a - b) === nx ? 1 : 0)]++;
		}

		const source = starts.get(cells[0])!;
		const target = goals.get(cells[cells.length - 1])!.port;
		const grid = cells.map((idx) => ({ x: xs[idx % nx], y: ys[(idx / nx) | 0] }));
		const points = simplify([
			{ x: borderX(source), y: source.y },
			...grid,
			{ x: borderX(target), y: target.y }
		]);
		return { points, sourceSide: source.side, targetSide: target.side };
	}
}

/** Drop points in the middle of straight runs. */
export function simplify(points: Point[]): Point[] {
	const out: Point[] = [];
	for (const p of points) {
		const last = out[out.length - 1];
		if (last && last.x === p.x && last.y === p.y) continue;
		const prev = out[out.length - 2];
		if (
			prev &&
			last &&
			((prev.x === last.x && last.x === p.x) || (prev.y === last.y && last.y === p.y))
		) {
			out[out.length - 1] = p;
		} else {
			out.push(p);
		}
	}
	return out;
}

/** SVG path through the points with rounded corners. */
export function roundedPath(points: Point[], radius = 8): string {
	if (points.length < 2) return '';
	let d = `M${points[0].x},${points[0].y}`;
	for (let k = 1; k < points.length - 1; k++) {
		const a = points[k - 1];
		const b = points[k];
		const c = points[k + 1];
		const r = Math.min(
			radius,
			Math.hypot(b.x - a.x, b.y - a.y) / 2,
			Math.hypot(c.x - b.x, c.y - b.y) / 2
		);
		const inX = b.x - Math.sign(b.x - a.x) * r;
		const inY = b.y - Math.sign(b.y - a.y) * r;
		const outX = b.x + Math.sign(c.x - b.x) * r;
		const outY = b.y + Math.sign(c.y - b.y) * r;
		d += ` L${inX},${inY} Q${b.x},${b.y} ${outX},${outY}`;
	}
	const last = points[points.length - 1];
	return `${d} L${last.x},${last.y}`;
}

/** The point halfway along the line, for labels. */
export function midpoint(points: Point[]): Point {
	const lengths = points.slice(1).map((p, k) => Math.hypot(p.x - points[k].x, p.y - points[k].y));
	let rest = lengths.reduce((a, b) => a + b, 0) / 2;
	for (let k = 0; k < lengths.length; k++) {
		if (rest <= lengths[k]) {
			const t = lengths[k] ? rest / lengths[k] : 0;
			return {
				x: points[k].x + (points[k + 1].x - points[k].x) * t,
				y: points[k].y + (points[k + 1].y - points[k].y) * t
			};
		}
		rest -= lengths[k];
	}
	return points[points.length - 1] ?? { x: 0, y: 0 };
}

/**
 * Move the ends of a route onto the handles as drawn. The first and last runs
 * are horizontal, so shifting both of their points keeps every corner square.
 * Gives up (null) when the route no longer fits the handles, e.g. mid-drag.
 */
export function snapEnds(points: Point[], source: Point, target: Point): Point[] | null {
	if (points.length < 2) return null;
	const out = points.map((p) => ({ ...p }));
	const first = out[0];
	const last = out[out.length - 1];
	if (Math.abs(first.x - source.x) > 4 || Math.abs(last.x - target.x) > 4) return null;
	if (Math.abs(first.y - source.y) > 8 || Math.abs(last.y - target.y) > 8) return null;
	if (out.length === 2) return Math.abs(source.y - target.y) < 0.5 ? [source, target] : null;
	out[0] = { ...source };
	out[1].y = source.y;
	out[out.length - 1] = { ...target };
	out[out.length - 2].y = target.y;
	return out;
}
