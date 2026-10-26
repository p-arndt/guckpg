import { describe, expect, it } from 'vitest';
import type { ObjectPrivileges, RoleDetails } from '#lib/api.js';
import {
	draftOf,
	filterObjects,
	grantorsOf,
	grantsOf,
	latestRequest,
	membershipCandidates,
	objectKey,
	planChanges,
	roleSummary,
	targetKey,
	toggleColumn,
	toggleGrant
} from './role-editor.js';

const role: RoleDetails = {
	name: 'app',
	canLogin: true,
	superuser: false,
	createDb: false,
	createRole: false,
	inherit: true,
	replication: false,
	bypassRls: false,
	connectionLimit: -1,
	validUntil: null,
	comment: null,
	memberOf: [{ role: 'readers', admin: false }],
	members: [],
	databases: [],
	otherRoles: ['readers', 'writers']
};

const table = (name: string, granted: string[] = [], effective = granted): ObjectPrivileges => ({
	schema: 'public',
	name,
	kind: 'table',
	owner: false,
	granted,
	grants: granted.map((privilege) => ({ privilege, grantor: 'postgres' })),
	effective
});

const usersKey = targetKey({ kind: 'table', schema: 'public', name: 'users' });
const ordersKey = targetKey({ kind: 'table', schema: 'public', name: 'orders' });

describe('planChanges', () => {
	it('has nothing to do for an untouched draft', () => {
		const grants = grantsOf([table('users', ['SELECT'])]);
		expect(planChanges(role, draftOf(role), grants, grants)).toEqual({
			changes: { name: 'app' },
			count: 0,
			problem: null
		});
	});

	it('collects attribute, password, comment, membership and rename edits', () => {
		const draft = {
			...draftOf(role),
			name: ' web ',
			canLogin: false,
			createDb: true,
			connectionLimit: '5',
			validUntil: '2027-01-01',
			password: 'secret',
			comment: 'Web app',
			memberOf: ['writers']
		};
		const { changes, count, problem } = planChanges(role, draft, {}, {});
		expect(problem).toBeNull();
		expect(changes).toEqual({
			name: 'app',
			rename: 'web',
			attributes: {
				canLogin: false,
				createDb: true,
				connectionLimit: 5,
				validUntil: '2027-01-01'
			},
			password: 'secret',
			comment: 'Web app',
			grantRoles: ['writers'],
			revokeRoles: ['readers']
		});
		expect(count).toBe(9);
	});

	it('clears limit, expiry and comment', () => {
		const limited = { ...role, connectionLimit: 3, validUntil: '2027-01-01', comment: 'x' };
		const draft = { ...draftOf(limited), connectionLimit: '', validUntil: ' ', comment: '' };
		expect(planChanges(limited, draft, {}, {}).changes).toEqual({
			name: 'app',
			attributes: { connectionLimit: -1, validUntil: null },
			comment: null
		});
	});

	it('prefers a new password over removing it', () => {
		const draft = { ...draftOf(role), password: 'x', removePassword: true };
		expect(planChanges(role, draft, {}, {}).changes).toEqual({ name: 'app', password: 'x' });
		const removing = { ...draftOf(role), removePassword: true };
		expect(planChanges(role, removing, {}, {}).changes).toEqual({
			name: 'app',
			removePassword: true
		});
	});

	it('turns grant differences into grants and revokes', () => {
		const before = grantsOf([table('users', ['SELECT', 'DELETE']), table('orders')]);
		const after = { ...before, [usersKey]: ['SELECT', 'INSERT'], [ordersKey]: ['SELECT'] };
		const { changes, count } = planChanges(role, draftOf(role), before, after);
		expect(changes.privileges).toEqual([
			{
				target: { kind: 'table', schema: 'public', name: 'users' },
				privilege: 'INSERT',
				grant: true
			},
			{
				target: { kind: 'table', schema: 'public', name: 'users' },
				privilege: 'DELETE',
				grant: false
			},
			{
				target: { kind: 'table', schema: 'public', name: 'orders' },
				privilege: 'SELECT',
				grant: true
			}
		]);
		expect(count).toBe(3);
	});

	it('reports invalid input', () => {
		expect(planChanges(role, { ...draftOf(role), name: ' ' }, {}, {}).problem).toMatch(/name/);
		expect(planChanges(role, { ...draftOf(role), connectionLimit: '-5' }, {}, {}).problem).toMatch(
			/Connection limit/
		);
	});
});

describe('grant toggles', () => {
	it('toggles one privilege', () => {
		const grants = toggleGrant({}, usersKey, 'SELECT');
		expect(grants[usersKey]).toEqual(['SELECT']);
		expect(toggleGrant(grants, usersKey, 'SELECT')[usersKey]).toEqual([]);
	});

	it('grants a column to all rows, or revokes it when all have it', () => {
		const some = { [usersKey]: ['SELECT'], [ordersKey]: [] };
		const all = toggleColumn(some, [usersKey, ordersKey], 'SELECT');
		expect(all).toEqual({ [usersKey]: ['SELECT'], [ordersKey]: ['SELECT'] });
		expect(toggleColumn(all, [usersKey, ordersKey], 'SELECT')).toEqual({
			[usersKey]: [],
			[ordersKey]: []
		});
	});
});

describe('filterObjects', () => {
	const objects = [table('users', ['SELECT']), table('orders'), table('audit', [], ['SELECT'])];

	it('matches schema-qualified names', () => {
		expect(filterObjects(objects, 'public.us', false, {}).map((o) => o.name)).toEqual(['users']);
	});

	it('hides objects the role cannot touch', () => {
		const grants = { ...grantsOf(objects), [ordersKey]: [] };
		expect(filterObjects(objects, '', true, grants).map((o) => o.name)).toEqual(['users', 'audit']);
	});
});

describe('objectKey', () => {
	it('matches targetKey and is stable per object', () => {
		const users = table('users');
		expect(objectKey(users)).toBe(usersKey);
		expect(objectKey(users)).toBe(objectKey(users));
	});
});

describe('grantorsOf', () => {
	it('lists each grantor of a privilege once', () => {
		const users: ObjectPrivileges = {
			...table('users', ['SELECT', 'UPDATE']),
			grants: [
				{ privilege: 'SELECT', grantor: 'postgres' },
				{ privilege: 'SELECT', grantor: 'admin' },
				{ privilege: 'SELECT', grantor: 'admin' },
				{ privilege: 'UPDATE', grantor: 'postgres' }
			]
		};
		expect(grantorsOf(users, 'SELECT')).toEqual(['postgres', 'admin']);
		expect(grantorsOf(users, 'DELETE')).toEqual([]);
	});
});

describe('membershipCandidates', () => {
	it('offers roles not yet joined, predefined pg_* roles last', () => {
		const details = { ...role, otherRoles: ['pg_read_all_data', 'readers', 'writers', 'admins'] };
		expect(membershipCandidates(details, draftOf(details))).toEqual([
			'writers',
			'admins',
			'pg_read_all_data'
		]);
	});
});

describe('roleSummary', () => {
	it('describes login, memberships, members and expiry', () => {
		expect(roleSummary(role)).toBe('Can log in · member of readers');
		expect(
			roleSummary({
				...role,
				canLogin: false,
				memberOf: [],
				members: [
					{ role: 'a', admin: false },
					{ role: 'b', admin: true }
				],
				validUntil: '2027-01-01'
			})
		).toBe('Cannot log in · 2 members · expires 2027-01-01');
		expect(roleSummary({ ...role, memberOf: [], members: [{ role: 'a', admin: false }] })).toBe(
			'Can log in · 1 member'
		);
	});
});

describe('latestRequest', () => {
	it('only lets the newest request apply its result', () => {
		const begin = latestRequest();
		const first = begin();
		expect(first()).toBe(true);
		const second = begin();
		expect(first()).toBe(false);
		expect(second()).toBe(true);
	});
});
