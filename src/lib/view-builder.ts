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
import type { CompletionTable } from './api.js';
import { importViewQuery } from './view-query-import.js';
export { importViewQuery } from './view-query-import.js';

export function importEditedViewSql(
	current: string,
	prefix: string | null,
	tables: CompletionTable[],
	view: { schema: string; name: string; details: ViewDetails } | null
) {
	if (prefix && !current.startsWith(prefix)) return null;
	let query = current.slice(prefix?.length ?? 0);
	if (view) {
		const header = replaceViewSql(view.schema, view.name, view.details, 'SELECT 1').split(
			'SELECT 1'
		)[0];
		if (!query.startsWith(header)) return null;
		query = query.slice(header.length);
	}
	return importViewQuery(query, tables, tables, view?.details.searchPath ?? ['pg_catalog'], true);
}

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
