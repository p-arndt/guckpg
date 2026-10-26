import type { DatabaseRole } from '#lib/api.js';

export interface RoleGroup {
	label: string;
	roles: DatabaseRole[];
}

/** Login roles are what people call "users"; the rest are group roles. */
export function groupRoles(roles: DatabaseRole[]): RoleGroup[] {
	const groups: RoleGroup[] = [
		{ label: 'Users', roles: roles.filter((r) => r.canLogin) },
		{ label: 'Group roles', roles: roles.filter((r) => !r.canLogin) }
	];
	return groups.filter((g) => g.roles.length > 0);
}

export function roleTitle(role: DatabaseRole): string {
	const attributes = [
		role.canLogin ? 'can log in' : 'cannot log in',
		role.superuser && 'superuser',
		role.createDb && 'can create databases',
		role.createRole && 'can create roles'
	].filter(Boolean);
	return `${role.name}: ${attributes.join(', ')}`;
}
