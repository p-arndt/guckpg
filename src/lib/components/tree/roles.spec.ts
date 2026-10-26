import { describe, expect, it } from 'vitest';
import type { DatabaseRole } from '#lib/api.js';
import { groupRoles, roleTitle } from './roles.js';

const role = (name: string, overrides: Partial<DatabaseRole> = {}): DatabaseRole => ({
	name,
	canLogin: true,
	superuser: false,
	createDb: false,
	createRole: false,
	...overrides
});

describe('groupRoles', () => {
	it('splits login roles from group roles and keeps order', () => {
		const groups = groupRoles([role('app'), role('readers', { canLogin: false }), role('bob')]);
		expect(groups.map((g) => [g.label, g.roles.map((r) => r.name)])).toEqual([
			['Users', ['app', 'bob']],
			['Group roles', ['readers']]
		]);
	});

	it('drops empty groups', () => {
		expect(groupRoles([role('app')]).map((g) => g.label)).toEqual(['Users']);
		expect(groupRoles([])).toEqual([]);
	});
});

describe('roleTitle', () => {
	it('lists every attribute that is set', () => {
		expect(roleTitle(role('postgres', { superuser: true, createDb: true, createRole: true }))).toBe(
			'postgres: can log in, superuser, can create databases, can create roles'
		);
	});

	it('marks roles without login', () => {
		expect(roleTitle(role('readers', { canLogin: false }))).toBe('readers: cannot log in');
	});
});
