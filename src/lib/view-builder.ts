import { quoteIdent } from './sql-quote.js';

export function createViewSql(schema: string, name: string, query: string): string {
	const trimmedName = name.trim();
	if (!schema || schema.includes('\0')) throw new Error('Choose a schema.');
	if (!trimmedName || trimmedName.includes('\0')) throw new Error('Enter a view name.');
	if (new TextEncoder().encode(trimmedName).length > 63)
		throw new Error('View names must be at most 63 bytes.');
	const select = query.trim().replace(/;\s*$/, '');
	if (!select) throw new Error('Build a query first.');
	return `CREATE VIEW ${quoteIdent(schema)}.${quoteIdent(trimmedName)} AS\n${select};`;
}

import type { ViewDetails } from './api.js';
export { importViewQuery } from './view-query-import.js';

export function replaceViewSql(
	schema: string,
	name: string,
	details: ViewDetails,
	query: string
): string {
	if (!schema || !name || schema.includes('\0') || name.includes('\0'))
		throw new Error('Invalid view name.');
	const select = query.trim().replace(/;\s*$/, '');
	if (!select) throw new Error('Build a query first.');
	const options = details.options ? ` WITH (${details.options})` : '';
	return `CREATE OR REPLACE VIEW ${quoteIdent(schema)}.${quoteIdent(name)}${options} AS\n${select};`;
}

export function viewEditorContent(
	current: string,
	snapshot: string | null,
	previousPrefix: string | null,
	statement: string
) {
	const prefix =
		previousPrefix !== null && current === snapshot
			? previousPrefix
			: current.trim()
				? `${current.trimEnd()}\n\n`
				: '';
	return { prefix, sql: `${prefix}${statement}\n` };
}
