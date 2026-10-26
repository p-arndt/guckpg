import { PostgreSQL } from '@codemirror/lang-sql';
import { classHighlighter, highlightCode } from '@lezer/highlight';

export interface Token {
	text: string;
	/** `tok-*` classes from the Lezer class highlighter, '' for plain text. */
	classes: string;
}

/** Highlighted SQL as plain tokens, so callers render text nodes and never HTML. */
export function highlightSql(code: string): Token[] {
	const tokens: Token[] = [];
	highlightCode(
		code,
		PostgreSQL.language.parser.parse(code),
		classHighlighter,
		(text, classes) => {
			const last = tokens.at(-1);
			if (last && last.classes === classes) last.text += text;
			else tokens.push({ text, classes });
		},
		() => {
			const last = tokens.at(-1);
			if (last && last.classes === '') last.text += '\n';
			else tokens.push({ text: '\n', classes: '' });
		}
	);
	return tokens;
}
