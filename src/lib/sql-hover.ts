import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';
import type { CompletionTable } from './api.js';

/** Catalog matches are suggestions, not SQL name resolution. */
export function catalogMatches(identifier: string, tables: CompletionTable[]): string[] {
	const quoted = identifier.startsWith('"') && identifier.endsWith('"');
	const name = quoted ? identifier.slice(1, -1).replaceAll('""', '"') : identifier.toLowerCase();
	const matches: string[] = [];
	for (const table of tables) {
		const relation = `${table.schema}.${table.name}`;
		if (table.name === name) matches.push(`Relation: ${relation}`);
		if (table.columns.includes(name)) matches.push(`Column: ${relation}.${name}`);
	}
	return matches;
}

export function catalogHover(state: EditorState, pos: number, tables: CompletionTable[]) {
	const node = syntaxTree(state).resolveInner(pos, -1);
	if (node.name !== 'Identifier' && node.name !== 'QuotedIdentifier') return null;
	const matches = catalogMatches(state.sliceDoc(node.from, node.to), tables);
	return matches.length ? { from: node.from, to: node.to, matches } : null;
}
