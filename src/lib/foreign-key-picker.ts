import type { ForeignKey } from './api.js';

export function singleColumnReferences(
	keys: ForeignKey[],
	table: string
): Record<string, ForeignKey> {
	const references: Record<string, ForeignKey> = Object.create(null);
	const ambiguous = new Set<string>();
	for (const key of keys) {
		if (key.fromTable !== table || key.fromColumns.length !== 1 || key.toColumns.length !== 1)
			continue;
		const column = key.fromColumns[0];
		if (references[column]) {
			delete references[column];
			ambiguous.add(column);
		} else if (!ambiguous.has(column)) references[column] = key;
	}
	return references;
}

export function referenceQuery(schema: string, key: ForeignKey): string {
	const quote = (name: string) => '"' + name.replaceAll('"', '""') + '"';
	return `SELECT ${quote(key.toColumns[0])} FROM ${quote(schema)}.${quote(key.toTable)} ORDER BY 1 LIMIT 101`;
}
