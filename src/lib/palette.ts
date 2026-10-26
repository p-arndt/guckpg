/**
 * Pulls items of the same kind together, so each kind gets one heading. Kinds keep the order
 * of their first item, and items keep their order within a kind, so the best match still
 * leads its group and the group of the best match comes first.
 */
export function groupByKind<T extends { kind: string }>(items: T[]): T[] {
	const groups = new Map<string, T[]>();
	for (const item of items) {
		const group = groups.get(item.kind);
		if (group) group.push(item);
		else groups.set(item.kind, [item]);
	}
	return [...groups.values()].flat();
}
