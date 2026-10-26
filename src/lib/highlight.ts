import { PostgreSQL } from '@codemirror/lang-sql';
import { jsonLanguage } from '@codemirror/lang-json';
type Parser = typeof jsonLanguage.parser;
import { classHighlighter, highlightCode } from '@lezer/highlight';

export interface Token {
	text: string;
	/** `tok-*` classes from the Lezer class highlighter, '' for plain text. */
	classes: string;
}

function highlight(code: string, parser: Parser): Token[] {
	const tokens: Token[] = [];
	highlightCode(
		code,
		parser.parse(code),
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

/** Highlighted SQL as plain tokens, so callers render text nodes and never HTML. */
export function highlightSql(code: string): Token[] {
	return highlight(code, PostgreSQL.language.parser);
}

/** Highlighted JSON as plain tokens; cut-off or broken JSON still highlights what it can. */
export function highlightJson(code: string): Token[] {
	return highlight(code, jsonLanguage.parser);
}

/** json and jsonb columns, including arrays of them. */
export function isJsonType(type: string | undefined): boolean {
	return /^jsonb?(\[\])?$/i.test(type?.trim() ?? '');
}
