import { describe, expect, it } from 'vitest';
import { shortcut } from './shortcut.js';

describe('shortcut', () => {
	it('uses the platform modifier and shift labels', () => {
		expect(shortcut('T', false, 'MacIntel')).toBe('⌘T');
		expect(shortcut('T', true, 'MacIntel')).toBe('⇧⌘T');
		expect(shortcut('T', false, 'Win32')).toBe('Ctrl+T');
		expect(shortcut('T', true, 'Win32')).toBe('Ctrl+Shift+T');
		expect(shortcut('↵', true, 'Linux x86_64')).toBe('Ctrl+Shift+↵');
	});
});
