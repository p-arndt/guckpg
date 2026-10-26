import { describe, expect, it } from 'vitest';
import { defaults, parseAppearance } from './appearance';
import { clouds, contourLines, ribbons, ridges, sparks, stars, waves } from './backdrop-art';
import { ART_HEIGHT, ART_WIDTH } from './backdrop-art';

describe('parseAppearance', () => {
	it('falls back to the defaults for missing or malformed data', () => {
		expect(parseAppearance(null)).toEqual(defaults);
		expect(parseAppearance('{')).toEqual(defaults);
		expect(parseAppearance('null')).toEqual(defaults);
	});

	it('keeps valid values and replaces invalid ones one by one', () => {
		expect(
			parseAppearance(
				'{"background":"unknown","accent":"green","glass":200,"image":"javascript:bad"}'
			)
		).toEqual({ ...defaults, accent: 'green', glass: 100 });
		expect(parseAppearance('{"background":"contours","glass":-5}')).toEqual({
			...defaults,
			background: 'contours',
			glass: 0
		});
	});

	it('accepts every built-in design', () => {
		for (const background of ['tide', 'nebula', 'ember'] as const) {
			expect(parseAppearance(JSON.stringify({ background })).background).toBe(background);
		}
	});

	it('restores a custom photo', () => {
		const image = 'data:image/png;base64,abc=';
		expect(parseAppearance(JSON.stringify({ background: 'custom', image }))).toEqual({
			...defaults,
			background: 'custom',
			image
		});
	});

	it('does not restore a custom background whose photo is gone', () => {
		expect(parseAppearance('{"background":"custom"}').background).toBe(defaults.background);
	});
});

describe('backdrop art', () => {
	it('draws the same shapes on every launch', () => {
		expect(contourLines()).toEqual(contourLines());
		expect(ribbons()).toEqual(ribbons());
		expect(ridges()).toEqual(ridges());
		expect(stars()).toEqual(stars());
		expect(waves()).toEqual(waves());
		expect(clouds()).toEqual(clouds());
		expect(sparks()).toEqual(sparks());
	});

	it('keeps every shape inside the art', () => {
		const inside = (x: number, y: number) => x >= 0 && x <= ART_WIDTH && y >= 0 && y <= ART_HEIGHT;
		expect(sparks().every((s) => inside(s.x, s.y) && s.o > 0 && s.o <= 1)).toBe(true);
		expect(clouds().every((c) => inside(c.cx, c.cy))).toBe(true);
		for (const wave of waves()) {
			const ys = [...wave.crest.matchAll(/[ML][\d.]+ ([\d.]+)/g)].map((m) => Number(m[1]));
			expect(ys.length).toBeGreaterThan(0);
			expect(ys.every((y) => y >= 0 && y <= ART_HEIGHT)).toBe(true);
		}
	});
});
