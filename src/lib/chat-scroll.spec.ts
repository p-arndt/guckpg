import { describe, expect, it, vi } from 'vitest';
import { followChatEnd, isNearChatEnd, nextFollowing, stopsFollowing } from './chat-scroll';

describe('chat scrolling', () => {
	it('follows new content at the bottom without moving someone reading earlier messages', () => {
		const element = { scrollHeight: 1000, scrollTop: 680, clientHeight: 300, scrollTo: vi.fn() };
		expect(isNearChatEnd(element)).toBe(true);
		element.scrollHeight = 1200;
		followChatEnd(element, true);
		expect(element.scrollTo).toHaveBeenCalledWith({ top: 1200, behavior: 'auto' });
		element.scrollTop = 300;
		followChatEnd(element, isNearChatEnd(element));
		expect(element.scrollTo).toHaveBeenCalledTimes(1);
	});
});

describe('nextFollowing', () => {
	const atEnd = { scrollHeight: 1000, scrollTop: 700, clientHeight: 300 };

	it('stops following on a small upward trackpad scroll that is still near the end', () => {
		const element = { ...atEnd, scrollTop: 697 };
		expect(isNearChatEnd(element)).toBe(true);
		expect(nextFollowing({ scrollTop: 700, following: true }, element)).toBe(false);
	});

	it('keeps following when the browser clamps scrollTop because content shrank', () => {
		const element = { scrollHeight: 950, scrollTop: 650, clientHeight: 300 };
		expect(nextFollowing({ scrollTop: 700, following: true }, element)).toBe(true);
	});

	it('does not start following on a clamp while the reader had already left', () => {
		const element = { scrollHeight: 950, scrollTop: 650, clientHeight: 300 };
		expect(nextFollowing({ scrollTop: 700, following: false }, element)).toBe(false);
	});

	it('resumes following once the reader scrolls back down near the end', () => {
		expect(nextFollowing({ scrollTop: 400, following: false }, { ...atEnd, scrollTop: 680 })).toBe(
			true
		);
	});

	it('stays detached while scrolling down but still far from the end', () => {
		expect(nextFollowing({ scrollTop: 200, following: false }, { ...atEnd, scrollTop: 400 })).toBe(
			false
		);
	});

	it('keeps following when a programmatic follow moves scrollTop down', () => {
		const grown = { scrollHeight: 1200, scrollTop: 900, clientHeight: 300 };
		expect(nextFollowing({ scrollTop: 700, following: true }, grown)).toBe(true);
	});

	it('keeps the previous state when nothing moved', () => {
		const far = { ...atEnd, scrollTop: 100 };
		expect(nextFollowing({ scrollTop: 100, following: false }, far)).toBe(false);
	});
});

describe('stopsFollowing', () => {
	it('treats wheel up as leaving even before the log can scroll', () => {
		expect(stopsFollowing({ type: 'wheel', deltaY: -4 })).toBe(true);
		expect(stopsFollowing({ type: 'wheel', deltaY: 4 })).toBe(false);
		expect(stopsFollowing({ type: 'wheel', deltaY: 0 })).toBe(false);
	});

	it('treats upward navigation keys as leaving', () => {
		for (const key of ['ArrowUp', 'PageUp', 'Home']) {
			expect(stopsFollowing({ type: 'keydown', key })).toBe(true);
		}
		for (const key of ['ArrowDown', 'PageDown', 'End', 'a']) {
			expect(stopsFollowing({ type: 'keydown', key })).toBe(false);
		}
	});
});
