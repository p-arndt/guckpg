import { describe, expect, it } from 'vitest';
import { formatCount, formatDuration, formatEstimate, plural } from './format';

describe('format', () => {
	it('counts with separators', () => {
		expect(formatCount(1234567)).toBe('1,234,567');
	});

	it('formats durations', () => {
		expect(formatDuration(12)).toBe('12 ms');
		expect(formatDuration(1234)).toBe('1.23 s');
		expect(formatDuration(12_345)).toBe('12.3 s');
		expect(formatDuration(125_000)).toBe('2 min 5 s');
	});

	it('formats row estimates', () => {
		expect(formatEstimate(null)).toBe('');
		expect(formatEstimate(-1)).toBe('');
		expect(formatEstimate(42)).toBe('~42');
		expect(formatEstimate(1234)).toBe('~1.2k');
		expect(formatEstimate(123_456)).toBe('~123k');
		expect(formatEstimate(2_500_000)).toBe('~2.5M');
	});

	it('pluralizes with a count', () => {
		expect(plural(1, 'row')).toBe('1 row');
		expect(plural(0, 'row')).toBe('0 rows');
		expect(plural(1234, 'change')).toBe('1,234 changes');
		expect(plural(2, 'more column', 'more columns')).toBe('2 more columns');
	});
});
