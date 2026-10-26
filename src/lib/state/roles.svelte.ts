/** Bumped after a role is created, changed or dropped, so open role lists reload. */
class RoleEvents {
	revision = $state(0);

	changed() {
		this.revision++;
	}
}

export const roleEvents = new RoleEvents();
