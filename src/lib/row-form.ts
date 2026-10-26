import type { NewCell } from './state/edits.svelte.js';

export interface RowField {
	name: string;
	type: string;
	value: NewCell;
	editable: boolean;
	primaryKey: boolean;
}

export function rowChanges(fields: RowField[], values: NewCell[], isNew: boolean) {
	return fields.flatMap((field, index) => {
		const value = values[index];
		return field.editable &&
			index < values.length &&
			value !== field.value &&
			(isNew || value !== undefined)
			? [{ column: index, value }]
			: [];
	});
}
export function formattedJson(value: string): string | null {
	try {
		JSON.parse(value);
		// Preserve number tokens: JSON.stringify would round PostgreSQL's large numeric values.
		const tokens = value.match(/"(?:\\.|[^"\\])*"|[{}[\],:]|[^\s{}[\],:]+/g) ?? [];
		let result = '';
		let depth = 0;
		const newline = () => '\n' + '  '.repeat(depth);
		for (const [index, token] of tokens.entries()) {
			if (token === '{' || token === '[') {
				result += token;
				depth++;
				if (tokens[index + 1] !== (token === '{' ? '}' : ']')) result += newline();
			} else if (token === '}' || token === ']') {
				depth--;
				if (tokens[index - 1] !== (token === '}' ? '{' : '[')) result += newline();
				result += token;
			} else if (token === ',') result += ',' + newline();
			else if (token === ':') result += ': ';
			else result += token;
		}
		return result;
	} catch {
		return null;
	}
}
