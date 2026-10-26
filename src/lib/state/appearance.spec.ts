import { describe, expect, it } from 'vitest';
import { APPEARANCE_KEY, defaults } from '#lib/appearance.js';
import { AppearanceState } from './appearance.svelte.js';
import { memoryStorage } from './test-storage.js';

describe('AppearanceState', () => {
	it('starts from what was saved', () => {
		const storage = memoryStorage();
		storage.setItem(APPEARANCE_KEY, '{"background":"dusk","accent":"purple"}');
		const state = new AppearanceState(storage);
		expect(state.background).toBe('dusk');
		expect(state.accent).toBe('purple');
		expect(state.glass).toBe(defaults.glass);
	});

	it('saves every change', () => {
		const storage = memoryStorage();
		new AppearanceState(storage).set({ background: 'contours', glass: 30 });
		expect(new AppearanceState(storage).background).toBe('contours');
		expect(new AppearanceState(storage).glass).toBe(30);
	});

	it('reports a failed save instead of throwing', () => {
		const storage = memoryStorage();
		storage.setItem = () => {
			throw new Error('quota');
		};
		const state = new AppearanceState(storage);
		state.set({ accent: 'green' });
		expect(state.accent).toBe('green');
		expect(state.error).toMatch(/could not save/i);
	});

	it('mirrors the settings onto the root element', () => {
		const state = new AppearanceState(memoryStorage());
		state.set({ background: 'dusk', accent: 'green', glass: 40 });
		const root = { dataset: {} as Record<string, string>, style: new Map<string, string>() };
		state.apply({
			dataset: root.dataset,
			style: { setProperty: (k: string, v: string) => root.style.set(k, v) }
		} as unknown as HTMLElement);
		expect(root.dataset).toEqual({ background: 'dusk', accent: 'green' });
		expect(root.style.get('--glass')).toBe('0.4');
		expect(root.style.get('--custom-background')).toBe('none');
	});
});
