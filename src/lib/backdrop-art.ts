/** Shapes for the backdrop art. Seeded, so the art looks the same on every launch. */

export const ART_WIDTH = 480;
export const ART_HEIGHT = 440;

/** Cheap seeded random, so the art looks the same on every launch. */
function seeded(seed: number): () => number {
	let s = seed >>> 0;
	return () => {
		s = (s * 1664525 + 1013904223) >>> 0;
		return s / 2 ** 32;
	};
}

export interface Contour {
	d: string;
	/** 0..1, fainter further down so the lines dissolve downwards. */
	opacity: number;
}

export function contourLines(count = 20, seed = 7): Contour[] {
	const rand = seeded(seed);
	// Shared waves make neighbouring lines run roughly parallel, like terrain; a small per-line
	// wave keeps them from being copies of each other.
	const shared = [
		{ amp: 9, freq: (2 * Math.PI) / 260, phase: rand() * 6 },
		{ amp: 4, freq: (2 * Math.PI) / 110, phase: rand() * 6 }
	];
	const lines: Contour[] = [];
	for (let i = 0; i < count; i++) {
		const base = 6 + i * ((ART_HEIGHT - 12) / (count - 1));
		const own = {
			amp: 2 + rand() * 3,
			freq: (2 * Math.PI) / (60 + rand() * 80),
			phase: rand() * 6
		};
		// Lines lean in towards a "peak" on the right, as contour lines bunch up around a summit.
		const pinch = 1 - 0.35 * Math.sin((i / (count - 1)) * Math.PI);
		const points: string[] = [];
		for (let x = 0; x <= ART_WIDTH; x += 6) {
			const wave =
				shared.reduce((sum, w) => sum + w.amp * Math.sin(x * w.freq + w.phase + i * 0.35), 0) *
					pinch +
				own.amp * Math.sin(x * own.freq + own.phase);
			const y = Math.min(ART_HEIGHT, Math.max(0, base + wave));
			points.push(`${x === 0 ? 'M' : 'L'}${x} ${y.toFixed(1)}`);
		}
		lines.push({ d: points.join(''), opacity: 0.3 * (1 - i / count) + 0.05 });
	}
	return lines;
}

/** Stacked ridge silhouettes for the "dusk" art, back to front. */
export function ridges(count = 5, seed = 11): string[] {
	const rand = seeded(seed);
	const out: string[] = [];
	for (let i = 0; i < count; i++) {
		// Ridges sit in the upper third; below them the art is just the fading foot of the hills.
		const base = ART_HEIGHT * (0.14 + i * 0.1);
		const waves = [
			{ amp: 14 - i * 2, freq: (2 * Math.PI) / (180 + rand() * 120), phase: rand() * 6 },
			{ amp: 5, freq: (2 * Math.PI) / (50 + rand() * 40), phase: rand() * 6 },
			{ amp: 1.5, freq: (2 * Math.PI) / (14 + rand() * 8), phase: rand() * 6 }
		];
		let d = `M0 ${ART_HEIGHT}`;
		for (let x = 0; x <= ART_WIDTH; x += 4) {
			const y = base + waves.reduce((sum, w) => sum + w.amp * Math.sin(x * w.freq + w.phase), 0);
			d += `L${x} ${Math.min(ART_HEIGHT, Math.max(0, y)).toFixed(1)}`;
		}
		out.push(`${d}L${ART_WIDTH} ${ART_HEIGHT}Z`);
	}
	return out;
}

/** Soft ribbons for the "aurora" art: closed bands that bend across the sky. */
export function ribbons(count = 4, seed = 23): string[] {
	const rand = seeded(seed);
	const out: string[] = [];
	for (let i = 0; i < count; i++) {
		const center = ART_HEIGHT * (0.07 + i * 0.13);
		const thick = 14 + rand() * 10;
		const wave = {
			amp: 18 + rand() * 10,
			freq: (2 * Math.PI) / (200 + rand() * 140),
			phase: rand() * 6
		};
		const top: string[] = [];
		const bottom: string[] = [];
		for (let x = 0; x <= ART_WIDTH; x += 8) {
			const y = center + wave.amp * Math.sin(x * wave.freq + wave.phase);
			const t = thick * (0.6 + 0.4 * Math.sin(x * wave.freq * 1.7 + wave.phase));
			top.push(`${x === 0 ? 'M' : 'L'}${x} ${(y - t).toFixed(1)}`);
			bottom.unshift(`L${x} ${(y + t).toFixed(1)}`);
		}
		out.push(`${top.join('')}${bottom.join('')}Z`);
	}
	return out;
}

/** Scattered stars: x, y, radius, opacity. Kept in the upper part, where the sky is darkest. */
export function stars(count = 60, seed = 5): { x: number; y: number; r: number; o: number }[] {
	const rand = seeded(seed);
	return Array.from({ length: count }, () => ({
		x: rand() * ART_WIDTH,
		y: rand() * ART_HEIGHT * 0.7,
		r: 0.35 + rand() * 0.45,
		o: 0.3 + rand() * 0.6
	}));
}

export interface Wave {
	/** The filled band, from its crest down to the bottom edge. */
	d: string;
	/** Just the crest, for a thin line of light along the top of the band. */
	crest: string;
}

/** Rolling swells for the "tide" art, back to front, flatter and closer together in the distance. */
export function waves(count = 6, seed = 31): Wave[] {
	const rand = seeded(seed);
	const out: Wave[] = [];
	for (let i = 0; i < count; i++) {
		const base = ART_HEIGHT * (0.22 + i * i * 0.018 + i * 0.05);
		const swell = [
			{ amp: 4 + i * 2.5, freq: (2 * Math.PI) / (150 + rand() * 90), phase: rand() * 6 },
			{ amp: 1.5 + i * 0.8, freq: (2 * Math.PI) / (45 + rand() * 30), phase: rand() * 6 }
		];
		const points: string[] = [];
		for (let x = 0; x <= ART_WIDTH; x += 4) {
			const y = base + swell.reduce((sum, w) => sum + w.amp * Math.sin(x * w.freq + w.phase), 0);
			points.push(`${x === 0 ? 'M' : 'L'}${x} ${Math.min(ART_HEIGHT, Math.max(0, y)).toFixed(1)}`);
		}
		const crest = points.join('');
		out.push({ d: `${crest}L${ART_WIDTH} ${ART_HEIGHT}L0 ${ART_HEIGHT}Z`, crest });
	}
	return out;
}

/** Soft, rotated clouds of gas for the "nebula" art. */
export function clouds(
	count = 7,
	seed = 41
): { cx: number; cy: number; rx: number; ry: number; angle: number }[] {
	const rand = seeded(seed);
	return Array.from({ length: count }, (_, i) => ({
		// Strung along a diagonal, so the clouds read as one arm rather than scattered blobs.
		cx: ART_WIDTH * (0.1 + (i / count) * 0.8 + (rand() - 0.5) * 0.15),
		cy: ART_HEIGHT * (0.12 + (i / count) * 0.3 + (rand() - 0.5) * 0.15),
		rx: 60 + rand() * 70,
		ry: 22 + rand() * 30,
		angle: 15 + rand() * 30
	}));
}

/** Embers rising from a fire below the frame: denser and brighter towards the bottom. */
export function sparks(count = 50, seed = 17): { x: number; y: number; r: number; o: number }[] {
	const rand = seeded(seed);
	return Array.from({ length: count }, () => {
		const height = rand() ** 0.7;
		return {
			x: ART_WIDTH * (0.15 + rand() * 0.75),
			y: ART_HEIGHT * (0.12 + height * 0.6),
			r: 0.5 + rand() * 1.1,
			o: 0.25 + height * 0.6 * rand() + 0.15
		};
	});
}
