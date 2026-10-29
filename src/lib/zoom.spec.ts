import { describe, expect, it } from 'vitest';
import { zoomForKey } from './zoom';

const key = (key: string, modifiers = {}) => ({
	key,
	ctrlKey: false,
	metaKey: false,
	altKey: false,
	...modifiers
});

describe('zoom shortcuts', () => {
	it('zooms with control or command and resets to 100%', () => {
		expect(zoomForKey(key('+', { ctrlKey: true }), 1)).toBe(1.1);
		expect(zoomForKey(key('=', { metaKey: true }), 1)).toBe(1.1);
		expect(zoomForKey(key('-', { metaKey: true }), 1)).toBe(0.9);
		expect(zoomForKey(key('0', { ctrlKey: true }), 1.5)).toBe(1);
	});
	it('limits zoom and ignores unrelated shortcuts', () => {
		expect(zoomForKey(key('+', { ctrlKey: true }), 2)).toBe(2);
		expect(zoomForKey(key('-', { ctrlKey: true }), 0.5)).toBe(0.5);
		expect(zoomForKey(key('+'), 1)).toBeNull();
		expect(zoomForKey(key('+', { ctrlKey: true, altKey: true }), 1)).toBeNull();
	});
});
