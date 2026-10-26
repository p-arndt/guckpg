/** Longest whitespace run kept inside a line; enough for real indentation. */
const MAX_RUN = 8;
/** Horizontal whitespace, including invisible and zero-width characters. */
const SPACE = '[\\t \\u00a0\\u1680\\u2000-\\u200d\\u202f\\u205f\\u2060\\u3000\\ufeff]';
const LONG_RUN = new RegExp(`${SPACE}{${MAX_RUN + 1},}`, 'g');
const BLANK = new RegExp(`^${SPACE}*$`);
const LINE_BREAK = /\r\n|[\n\r\v\f\u0085\u2028\u2029]/;

export interface SqlPreview {
	/** The statement with whitespace runs shortened, for display only. */
	text: string;
	/** Whether blank lines or space runs between visible parts were shortened. */
	collapsed: boolean;
	/** Line and character counts of the original statement, so nothing can hide off-screen. */
	lines: number;
	chars: number;
}

/**
 * Prepares a statement for an approval prompt. Long runs of blank lines or spaces could push a
 * dangerous part out of the visible box, so they are shortened; execution still uses the
 * original statement.
 */
export function sqlPreview(sql: string): SqlPreview {
	const source = sql.split(LINE_BREAK);
	const lines: string[] = [];
	let collapsed = false;
	// Blank lines dropped since the last text; they only count once text follows them.
	let skipped = false;
	for (const line of source) {
		if (BLANK.test(line)) {
			if (lines.length === 0) continue;
			if (lines.at(-1) === '') skipped = true;
			else lines.push('');
			continue;
		}
		const short = line.replace(LONG_RUN, ' '.repeat(MAX_RUN));
		if (short !== line || skipped) collapsed = true;
		skipped = false;
		lines.push(short);
	}
	if (lines.at(-1) === '') lines.pop();
	return { text: lines.join('\n'), collapsed, lines: source.length, chars: [...sql].length };
}
