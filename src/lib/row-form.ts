import { isJsonType } from './highlight.js';
import type { NewCell } from './state/edits.svelte.js';

export interface RowField {
	name: string;
	type: string;
	value: NewCell;
	editable: boolean;
	primaryKey: boolean;
	/** Differs from the loaded value (an unsaved edit in the grid). */
	changed?: boolean;
}

/** Whether writing `next` into a field is an edit worth recording. */
export function isFieldEdit(field: RowField, next: NewCell, isNew: boolean): boolean {
	return (
		field.editable &&
		next !== field.value &&
		!(isJsonType(field.type) && sameJson(next, field.value)) &&
		(isNew || next !== undefined)
	);
}

export function rowChanges(fields: RowField[], values: NewCell[], isNew: boolean) {
	return fields.flatMap((field, index) =>
		index < values.length && isFieldEdit(field, values[index], isNew)
			? [{ column: index, value: values[index] }]
			: []
	);
}

export const ROW_PANEL_MIN_PX = 300;
export const ROW_PANEL_DEFAULT_PX = 400;

/** Panel width kept usable: never below the minimum, never more than 70% of the space. */
export function clampPanelWidth(width: number, available: number): number {
	const max = Math.max(ROW_PANEL_MIN_PX, Math.floor(available * 0.7));
	if (!Number.isFinite(width)) return Math.min(ROW_PANEL_DEFAULT_PX, max);
	return Math.round(Math.min(max, Math.max(ROW_PANEL_MIN_PX, width)));
}

/** Equal JSON text apart from whitespace, so pretty-printing a value for display is no edit. */
export function sameJson(a: NewCell, b: NewCell): boolean {
	if (typeof a !== 'string' || typeof b !== 'string') return a === b;
	const formatted = formattedJson(a);
	return formatted !== null && formatted === formattedJson(b);
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

export type FieldKind = 'json' | 'bool' | 'multiline' | 'line';

/** Which input a form field gets: short values stay on one line, long or multi-line text wraps. */
export function fieldKind(type: string, value: string): FieldKind {
	if (isJsonType(type)) return 'json';
	if (/^bool(ean)?$/i.test(type.trim())) return 'bool';
	return value.includes('\n') || value.length > 80 ? 'multiline' : 'line';
}
