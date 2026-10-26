export const APPEARANCE_KEY = 'guckpg.appearance';

export const backgrounds = [
	'none',
	'aurora',
	'dusk',
	'contours',
	'tide',
	'nebula',
	'ember',
	'custom'
] as const;
export const accents = ['blue', 'green', 'purple'] as const;
export type Background = (typeof backgrounds)[number];
export type Art = Exclude<Background, 'none' | 'custom'>;
export type Accent = (typeof accents)[number];

export interface Appearance {
	background: Background;
	accent: Accent;
	/** 0..100: how much of the backdrop shows through the panels' chrome. */
	glass: number;
	/** A data: URL, only for the custom background. */
	image: string;
}

export const defaults: Appearance = { background: 'aurora', accent: 'blue', glass: 60, image: '' };

const IMAGE_URL = /^data:image\/(png|jpeg|webp);base64,[a-z0-9+/]*={0,2}$/i;

function oneOf<T extends string>(list: readonly T[], value: unknown, fallback: T): T {
	return list.includes(value as T) ? (value as T) : fallback;
}

export function parseAppearance(value: string | null): Appearance {
	let data: Record<string, unknown>;
	try {
		data = JSON.parse(value ?? '{}') ?? {};
	} catch {
		return { ...defaults };
	}
	const image = typeof data.image === 'string' && IMAGE_URL.test(data.image) ? data.image : '';
	let background = oneOf(backgrounds, data.background, defaults.background);
	// A custom background without its image would be an empty window.
	if (background === 'custom' && !image) background = defaults.background;
	return {
		background,
		accent: oneOf(accents, data.accent, defaults.accent),
		glass:
			typeof data.glass === 'number' && Number.isFinite(data.glass)
				? Math.max(0, Math.min(100, Math.round(data.glass)))
				: defaults.glass,
		image
	};
}
