import { describe, expect, it } from 'vitest';
import { isSendKey } from './composer.js';

const key = (key: string, shiftKey = false, isComposing = false) => ({
	key,
	shiftKey,
	isComposing
});

describe('isSendKey', () => {
	it('sends on Enter', () => {
		expect(isSendKey(key('Enter'))).toBe(true);
	});

	it('leaves Shift+Enter for a new line', () => {
		expect(isSendKey(key('Enter', true))).toBe(false);
	});

	it('does not send while an IME composition is confirmed', () => {
		expect(isSendKey(key('Enter', false, true))).toBe(false);
	});

	it('ignores other keys', () => {
		expect(isSendKey(key('a'))).toBe(false);
	});
});
