import { describe, expect, it } from 'vitest';
import { fuzzyFilter, fuzzyScore } from './fuzzy.js';

describe('fuzzyScore', () => {
	it('matches subsequences case-insensitively', () => {
		expect(fuzzyScore('USR', 'users')).not.toBeNull();
		expect(fuzzyScore('pub ord', 'public.orders')).not.toBeNull();
		expect(fuzzyScore('xyz', 'users')).toBeNull();
		expect(fuzzyScore('', 'anything')).toBe(0);
	});

	it('ranks prefixes and word starts higher', () => {
		const items = ['bonus_entries', 'audit_user_roles', 'users'];
		expect(fuzzyFilter(items, 'user', (s) => s)).toEqual([
			'users',
			'audit_user_roles',
			'bonus_entries'
		]);
	});

	it('keeps input order for an empty query', () => {
		expect(fuzzyFilter(['b', 'a'], '', (s) => s)).toEqual(['b', 'a']);
	});
});
