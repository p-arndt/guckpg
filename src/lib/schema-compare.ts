import type { Column, Relation } from './api.js';

export interface SchemaSnapshot {
	relations: { relation: Relation; columns: Column[] }[];
}

export interface SchemaDifference {
	object: string;
	detail: string;
}

export function compareSchemas(left: SchemaSnapshot, right: SchemaSnapshot): SchemaDifference[] {
	const differences: SchemaDifference[] = [];
	const a = new Map(left.relations.map((r) => [r.relation.name, r]));
	const b = new Map(right.relations.map((r) => [r.relation.name, r]));
	for (const name of [...new Set([...a.keys(), ...b.keys()])].sort()) {
		const l = a.get(name);
		const r = b.get(name);
		if (!l || !r) {
			differences.push({ object: name, detail: l ? 'Only in source' : 'Only in target' });
			continue;
		}
		if (l.relation.kind !== r.relation.kind)
			differences.push({ object: name, detail: `Kind: ${l.relation.kind} → ${r.relation.kind}` });
		const lc = new Map(l.columns.map((c) => [c.name, c]));
		const rc = new Map(r.columns.map((c) => [c.name, c]));
		for (const column of [...new Set([...lc.keys(), ...rc.keys()])].sort()) {
			const x = lc.get(column);
			const y = rc.get(column);
			const object = `${name}.${column}`;
			if (!x || !y) {
				differences.push({ object, detail: x ? 'Only in source' : 'Only in target' });
				continue;
			}
			for (const field of ['dataType', 'notNull', 'default', 'primaryKey'] as const) {
				if (x[field] !== y[field])
					differences.push({
						object,
						detail: `${field}: ${String(x[field])} → ${String(y[field])}`
					});
			}
		}
	}
	return differences;
}
