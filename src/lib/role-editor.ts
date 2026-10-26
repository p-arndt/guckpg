import type {
	ObjectPrivileges,
	PrivilegeTarget,
	RoleAttributeChanges,
	RoleChanges,
	RoleDetails
} from '#lib/api.js';

export const DATABASE_PRIVILEGES = ['CONNECT', 'CREATE', 'TEMPORARY'] as const;
export const SCHEMA_PRIVILEGES = ['USAGE', 'CREATE'] as const;
export const TABLE_PRIVILEGES = [
	'SELECT',
	'INSERT',
	'UPDATE',
	'DELETE',
	'TRUNCATE',
	'REFERENCES',
	'TRIGGER'
] as const;

export interface RoleDraft {
	name: string;
	comment: string;
	canLogin: boolean;
	superuser: boolean;
	createDb: boolean;
	createRole: boolean;
	inherit: boolean;
	replication: boolean;
	bypassRls: boolean;
	/** Empty means unlimited. */
	connectionLimit: string;
	/** Empty means the password never expires. */
	validUntil: string;
	/** Empty keeps the current password. */
	password: string;
	removePassword: boolean;
	memberOf: string[];
}

/** Directly granted privileges per object, keyed by `targetKey`. */
export type Grants = Record<string, string[]>;

const FLAGS = [
	'canLogin',
	'superuser',
	'createDb',
	'createRole',
	'inherit',
	'replication',
	'bypassRls'
] as const;

export function draftOf(role: RoleDetails): RoleDraft {
	return {
		name: role.name,
		comment: role.comment ?? '',
		canLogin: role.canLogin,
		superuser: role.superuser,
		createDb: role.createDb,
		createRole: role.createRole,
		inherit: role.inherit,
		replication: role.replication,
		bypassRls: role.bypassRls,
		connectionLimit: role.connectionLimit < 0 ? '' : String(role.connectionLimit),
		validUntil: role.validUntil ?? '',
		password: '',
		removePassword: false,
		memberOf: role.memberOf.map((m) => m.role)
	};
}

export function targetOf(object: ObjectPrivileges): PrivilegeTarget {
	if (object.kind === 'database') return { kind: 'database', name: object.name };
	if (object.kind === 'schema') return { kind: 'schema', name: object.name };
	return { kind: 'table', schema: object.schema ?? '', name: object.name };
}

export function targetKey(target: PrivilegeTarget): string {
	return target.kind === 'table'
		? JSON.stringify([target.kind, target.schema, target.name])
		: JSON.stringify([target.kind, target.name]);
}

function parseKey(key: string): PrivilegeTarget {
	const [kind, a, b] = JSON.parse(key) as [PrivilegeTarget['kind'], string, string?];
	return kind === 'table' ? { kind, schema: a, name: b ?? '' } : { kind, name: a };
}

export function grantsOf(objects: ObjectPrivileges[]): Grants {
	return Object.fromEntries(objects.map((o) => [targetKey(targetOf(o)), [...o.granted]]));
}

/** Grant `privilege` on every key, or revoke it when all of them already have it. */
export function toggleColumn(grants: Grants, keys: string[], privilege: string): Grants {
	const all = keys.every((k) => grants[k]?.includes(privilege));
	const next = { ...grants };
	for (const key of keys) {
		const current = next[key] ?? [];
		next[key] = all
			? current.filter((p) => p !== privilege)
			: current.includes(privilege)
				? current
				: [...current, privilege];
	}
	return next;
}

export function toggleGrant(grants: Grants, key: string, privilege: string): Grants {
	const current = grants[key] ?? [];
	return {
		...grants,
		[key]: current.includes(privilege)
			? current.filter((p) => p !== privilege)
			: [...current, privilege]
	};
}

export interface PlannedChanges {
	changes: RoleChanges;
	/** Number of separate edits, for the "n pending changes" label. */
	count: number;
	/** Set when the draft cannot be turned into changes. */
	problem: string | null;
}

export function planChanges(
	role: RoleDetails,
	draft: RoleDraft,
	originalGrants: Grants,
	grants: Grants
): PlannedChanges {
	const original = draftOf(role);
	const changes: RoleChanges = { name: role.name };
	let count = 0;
	let problem: string | null = null;

	const attributes: RoleAttributeChanges = {};
	for (const flag of FLAGS) {
		if (draft[flag] !== original[flag]) {
			attributes[flag] = draft[flag];
			count++;
		}
	}
	const limit = draft.connectionLimit.trim();
	if (limit !== original.connectionLimit) {
		if (limit === '') {
			attributes.connectionLimit = -1;
			count++;
		} else if (/^\d+$/.test(limit)) {
			attributes.connectionLimit = Number(limit);
			count++;
		} else {
			problem = 'Connection limit must be a whole number, or empty for unlimited.';
		}
	}
	const validUntil = draft.validUntil.trim();
	if (validUntil !== original.validUntil) {
		attributes.validUntil = validUntil || null;
		count++;
	}
	if (Object.keys(attributes).length) changes.attributes = attributes;

	if (draft.password) {
		changes.password = draft.password;
		count++;
	} else if (draft.removePassword) {
		changes.removePassword = true;
		count++;
	}
	const comment = draft.comment.trim();
	if (comment !== original.comment) {
		changes.comment = comment || null;
		count++;
	}

	const grantRoles = draft.memberOf.filter((r) => !original.memberOf.includes(r));
	const revokeRoles = original.memberOf.filter((r) => !draft.memberOf.includes(r));
	if (grantRoles.length) changes.grantRoles = grantRoles;
	if (revokeRoles.length) changes.revokeRoles = revokeRoles;
	count += grantRoles.length + revokeRoles.length;

	const privileges: NonNullable<RoleChanges['privileges']> = [];
	for (const key of new Set([...Object.keys(originalGrants), ...Object.keys(grants)])) {
		const before = originalGrants[key] ?? [];
		const after = grants[key] ?? [];
		const target = parseKey(key);
		for (const privilege of after.filter((p) => !before.includes(p)))
			privileges.push({ target, privilege, grant: true });
		for (const privilege of before.filter((p) => !after.includes(p)))
			privileges.push({ target, privilege, grant: false });
	}
	if (privileges.length) changes.privileges = privileges;
	count += privileges.length;

	const name = draft.name.trim();
	if (!name) problem = 'The role needs a name.';
	else if (name !== role.name) {
		changes.rename = name;
		count++;
	}
	return { changes, count, problem };
}

/** Tables matching `query` by name or schema-qualified name; `onlyGranted` hides untouched ones. */
export function filterObjects(
	objects: ObjectPrivileges[],
	query: string,
	onlyGranted: boolean,
	grants: Grants
): ObjectPrivileges[] {
	const q = query.trim().toLowerCase();
	return objects.filter((o) => {
		if (q && !`${o.schema}.${o.name}`.toLowerCase().includes(q)) return false;
		if (!onlyGranted) return true;
		return o.owner || o.effective.length > 0 || (grants[targetKey(targetOf(o))]?.length ?? 0) > 0;
	});
}
