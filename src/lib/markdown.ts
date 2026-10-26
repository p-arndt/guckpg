/**
 * A small Markdown parser for assistant replies. It returns a tree that components
 * render as elements, never as an HTML string: model output must not be able to
 * inject markup into a webview that can call Tauri commands.
 */

export type Inline =
	| { type: 'text'; text: string }
	| { type: 'code'; text: string }
	| { type: 'strong' | 'em' | 'del'; children: Inline[] }
	| { type: 'link'; href: string; children: Inline[] }
	| { type: 'break' };

export type Align = 'left' | 'center' | 'right' | null;

export type Block =
	| { type: 'paragraph'; children: Inline[] }
	| { type: 'heading'; level: number; children: Inline[] }
	| { type: 'code'; lang: string; text: string; closed: boolean }
	| { type: 'list'; ordered: boolean; start: number; items: Block[][] }
	| { type: 'quote'; children: Block[] }
	| { type: 'table'; align: Align[]; head: Inline[][]; rows: Inline[][][] }
	| { type: 'rule' };

const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const LIST_ITEM = /^( {0,3})([-*+]|\d{1,9}[.)])([ \t]+(.*))?$/;
const TABLE_DIVIDER = /^ {0,3}\|?[ \t]*:?-+:?[ \t]*(\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/;
const SQL_LANGS = /^(sql|postgresql|postgres)$/i;

/** True for the fences that may open in the editor; matches `extractSqlBlocks`. */
export function isSqlBlock(block: Block): block is Extract<Block, { type: 'code' }> {
	return block.type === 'code' && block.closed && SQL_LANGS.test(block.lang) && !!block.text.trim();
}

export function parseMarkdown(source: string): Block[] {
	return parseBlocks(source.replace(/\r\n?/g, '\n').split('\n'));
}

function isBlank(line: string): boolean {
	return line.trim() === '';
}

/** Lines that end a paragraph because they start another block. */
function startsBlock(line: string, next: string | undefined): boolean {
	return (
		FENCE.test(line) ||
		HEADING.test(line) ||
		RULE.test(line) ||
		QUOTE.test(line) ||
		LIST_ITEM.test(line) ||
		(line.includes('|') && next !== undefined && TABLE_DIVIDER.test(next) && next.includes('-'))
	);
}

function parseBlocks(lines: string[]): Block[] {
	const blocks: Block[] = [];
	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		if (isBlank(line)) {
			i++;
			continue;
		}

		const fence = FENCE.exec(line);
		if (fence) {
			const marker = fence[1];
			const closing = new RegExp(
				`^ {0,3}${marker[0] === '`' ? '`' : '~'}{${marker.length},}[ \\t]*$`
			);
			const body: string[] = [];
			let j = i + 1;
			while (j < lines.length && !closing.test(lines[j])) body.push(lines[j++]);
			blocks.push({
				type: 'code',
				lang: fence[2].trim().split(/\s+/)[0] ?? '',
				text: body.join('\n'),
				closed: j < lines.length
			});
			i = j + 1;
			continue;
		}

		const heading = HEADING.exec(line);
		if (heading) {
			blocks.push({
				type: 'heading',
				level: heading[1].length,
				children: parseInline(heading[2] ?? '')
			});
			i++;
			continue;
		}

		if (RULE.test(line)) {
			blocks.push({ type: 'rule' });
			i++;
			continue;
		}

		if (QUOTE.test(line)) {
			const inner: string[] = [];
			while (i < lines.length && !isBlank(lines[i])) {
				const quoted = QUOTE.exec(lines[i]);
				inner.push(quoted ? quoted[1] : lines[i]);
				i++;
			}
			blocks.push({ type: 'quote', children: parseBlocks(inner) });
			continue;
		}

		const item = LIST_ITEM.exec(line);
		if (item) {
			const [list, next] = parseList(lines, i);
			blocks.push(list);
			i = next;
			continue;
		}

		if (line.includes('|') && i + 1 < lines.length && TABLE_DIVIDER.test(lines[i + 1])) {
			const head = splitRow(line);
			const align = splitRow(lines[i + 1]).map((cell): Align => {
				const left = cell.startsWith(':');
				const right = cell.endsWith(':');
				return left && right ? 'center' : right ? 'right' : left ? 'left' : null;
			});
			const rows: Inline[][][] = [];
			i += 2;
			while (i < lines.length && lines[i].includes('|') && !isBlank(lines[i])) {
				const cells = splitRow(lines[i]);
				rows.push(head.map((_, c) => parseInline(cells[c] ?? '')));
				i++;
			}
			blocks.push({ type: 'table', align, head: head.map(parseInline), rows });
			continue;
		}

		const paragraph: string[] = [line.trim()];
		i++;
		while (i < lines.length && !isBlank(lines[i]) && !startsBlock(lines[i], lines[i + 1])) {
			paragraph.push(lines[i].trim());
			i++;
		}
		blocks.push({ type: 'paragraph', children: parseInline(paragraph.join('\n')) });
	}
	return blocks;
}

function parseList(lines: string[], from: number): [Block, number] {
	const first = LIST_ITEM.exec(lines[from])!;
	const ordered = /\d/.test(first[2]);
	const indent = first[1].length;
	const items: Block[][] = [];
	let i = from;
	while (i < lines.length) {
		const item = LIST_ITEM.exec(lines[i]);
		if (!item || item[1].length !== indent || /\d/.test(item[2]) !== ordered) break;
		const contentIndent = indent + item[2].length + 1;
		const content: string[] = [item[4] ?? ''];
		i++;
		while (i < lines.length) {
			const line = lines[i];
			if (isBlank(line)) {
				// A blank line continues the item only when indented content follows.
				const next = lines[i + 1];
				if (next !== undefined && !isBlank(next) && leadingSpaces(next) >= contentIndent) {
					content.push('');
					i++;
					continue;
				}
				break;
			}
			const sibling = LIST_ITEM.exec(line);
			if (sibling && sibling[1].length <= indent) break;
			if (leadingSpaces(line) < contentIndent && startsBlock(line, lines[i + 1]) && !sibling) break;
			content.push(line.slice(Math.min(leadingSpaces(line), contentIndent)));
			i++;
		}
		items.push(parseBlocks(content));
		// Blank lines between items keep the list going.
		let j = i;
		while (j < lines.length && isBlank(lines[j])) j++;
		const next = j < lines.length ? LIST_ITEM.exec(lines[j]) : null;
		if (j > i && next && next[1].length === indent && /\d/.test(next[2]) === ordered) i = j;
	}
	return [{ type: 'list', ordered, start: ordered ? Number.parseInt(first[2], 10) : 1, items }, i];
}

function leadingSpaces(line: string): number {
	return line.length - line.trimStart().length;
}

function splitRow(line: string): string[] {
	let row = line.trim();
	if (row.startsWith('|')) row = row.slice(1);
	if (row.endsWith('|') && !row.endsWith('\\|')) row = row.slice(0, -1);
	const cells: string[] = [];
	let cell = '';
	for (let k = 0; k < row.length; k++) {
		if (row[k] === '\\' && row[k + 1] === '|') {
			cell += '|';
			k++;
		} else if (row[k] === '|') {
			cells.push(cell.trim());
			cell = '';
		} else cell += row[k];
	}
	cells.push(cell.trim());
	return cells;
}

const ESCAPABLE = '\\`*_{}[]()#+-.!|~>';

export function parseInline(text: string): Inline[] {
	const out: Inline[] = [];
	let buffer = '';
	const flush = () => {
		if (buffer) out.push({ type: 'text', text: buffer });
		buffer = '';
	};

	let i = 0;
	while (i < text.length) {
		const ch = text[i];

		if (ch === '\\' && i + 1 < text.length && ESCAPABLE.includes(text[i + 1])) {
			buffer += text[i + 1];
			i += 2;
			continue;
		}

		if (ch === '\n') {
			flush();
			out.push({ type: 'break' });
			i++;
			continue;
		}

		if (ch === '`') {
			let run = 1;
			while (text[i + run] === '`') run++;
			const ticks = '`'.repeat(run);
			const end = text.indexOf(ticks, i + run);
			if (end !== -1 && text[end + run] !== '`') {
				flush();
				let code = text.slice(i + run, end).replace(/\n/g, ' ');
				if (/^ .* $/.test(code) && code.trim()) code = code.slice(1, -1);
				out.push({ type: 'code', text: code });
				i = end + run;
				continue;
			}
			buffer += ticks;
			i += run;
			continue;
		}

		if (ch === '[') {
			const link = matchLink(text, i);
			if (link) {
				flush();
				out.push({ type: 'link', href: link.href, children: parseInline(link.label) });
				i = link.end;
				continue;
			}
		}

		const delimited = matchDelimited(text, i);
		if (delimited) {
			flush();
			out.push({ type: delimited.type, children: parseInline(delimited.inner) });
			i = delimited.end;
			continue;
		}

		buffer += ch;
		i++;
	}
	flush();
	return out;
}

function matchLink(text: string, at: number): { label: string; href: string; end: number } | null {
	let depth = 0;
	let k = at;
	for (; k < text.length; k++) {
		if (text[k] === '\\') k++;
		else if (text[k] === '[') depth++;
		else if (text[k] === ']' && --depth === 0) break;
	}
	if (k >= text.length || text[k + 1] !== '(') return null;
	const close = text.indexOf(')', k + 2);
	if (close === -1) return null;
	const href =
		text
			.slice(k + 2, close)
			.trim()
			.split(/\s+/)[0] ?? '';
	if (!href) return null;
	return { label: text.slice(at + 1, k), href, end: close + 1 };
}

const DELIMITERS: { open: string; type: 'strong' | 'em' | 'del' }[] = [
	{ open: '**', type: 'strong' },
	{ open: '__', type: 'strong' },
	{ open: '~~', type: 'del' },
	{ open: '*', type: 'em' },
	{ open: '_', type: 'em' }
];

function matchDelimited(
	text: string,
	at: number
): { type: 'strong' | 'em' | 'del'; inner: string; end: number } | null {
	for (const { open, type } of DELIMITERS) {
		if (!text.startsWith(open, at)) continue;
		const after = text[at + open.length];
		if (after === undefined || /\s/.test(after)) continue;
		// Underscores inside words (snake_case identifiers) are text, not emphasis.
		if (open[0] === '_' && at > 0 && /\w/.test(text[at - 1])) continue;
		let search = at + open.length + 1;
		while (search <= text.length) {
			const end = text.indexOf(open, search);
			if (end === -1) break;
			const before = text[end - 1];
			const following = text[end + open.length];
			const skipDouble = open.length === 1 && following === open;
			const wordAfter = open[0] === '_' && following !== undefined && /\w/.test(following);
			if (
				!/\s/.test(before) &&
				!skipDouble &&
				!wordAfter &&
				!text.slice(at, end).includes('\n\n')
			) {
				return { type, inner: text.slice(at + open.length, end), end: end + open.length };
			}
			search = end + (skipDouble ? 2 : 1);
		}
	}
	return null;
}
