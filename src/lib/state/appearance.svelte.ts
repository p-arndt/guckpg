import { APPEARANCE_KEY, parseAppearance, type Appearance } from '#lib/appearance.js';

/** The user's backdrop, accent and glass settings, persisted in localStorage. */
export class AppearanceState implements Appearance {
	background = $state<Appearance['background']>('aurora');
	accent = $state<Appearance['accent']>('blue');
	glass = $state(0);
	image = $state('');
	/** Set when the settings could not be saved, usually because the image is too big. */
	error = $state('');

	constructor(private storage: Storage | null) {
		Object.assign(this, parseAppearance(storage?.getItem(APPEARANCE_KEY) ?? null));
	}

	set(patch: Partial<Appearance>) {
		Object.assign(this, patch);
		const saved: Appearance = {
			background: this.background,
			accent: this.accent,
			glass: this.glass,
			image: this.image
		};
		try {
			this.storage?.setItem(APPEARANCE_KEY, JSON.stringify(saved));
			this.error = '';
		} catch {
			this.error = 'Could not save the appearance. Try a smaller image.';
		}
	}

	/** Mirrors the settings onto <html>, where the stylesheet picks them up. */
	apply(root: HTMLElement) {
		root.dataset.background = this.background;
		root.dataset.accent = this.accent;
		root.style.setProperty('--glass', String(this.glass / 100));
		root.style.setProperty('--custom-background', this.image ? `url("${this.image}")` : 'none');
	}
}

export const appearance = new AppearanceState(
	typeof localStorage === 'undefined' ? null : localStorage
);
