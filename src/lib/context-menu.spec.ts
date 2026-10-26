import { describe, expect, it } from 'vitest';
import { allowNativeContextMenu } from './context-menu.js';

/** Stands in for an element nested in the given selectors' matches. */
const inside = (...matches: string[]) => ({
	closest: (selector: string) =>
		selector.split(',').some((s) => matches.includes(s.trim())) ? {} : null
});

const plain = { hasSelection: false, shiftKey: false, dev: false };

describe('allowNativeContextMenu', () => {
	it('blocks the menu on plain chrome', () => {
		expect(allowNativeContextMenu(inside(), plain)).toBe(false);
		expect(allowNativeContextMenu(null, plain)).toBe(false);
	});

	it('keeps it in text fields and the SQL editor', () => {
		expect(allowNativeContextMenu(inside('input'), plain)).toBe(true);
		expect(allowNativeContextMenu(inside('textarea'), plain)).toBe(true);
		expect(allowNativeContextMenu(inside('.cm-editor'), plain)).toBe(true);
	});

	it('keeps it when text is selected, for copy', () => {
		expect(allowNativeContextMenu(inside(), { ...plain, hasSelection: true })).toBe(true);
	});

	it('keeps Shift+right-click for Inspect only in development', () => {
		expect(allowNativeContextMenu(inside(), { ...plain, shiftKey: true, dev: true })).toBe(true);
		expect(allowNativeContextMenu(inside(), { ...plain, shiftKey: true })).toBe(false);
	});
});
