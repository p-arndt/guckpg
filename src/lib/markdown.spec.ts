import { describe, expect, it, vi } from 'vitest';
import { extractSqlBlocks } from './ai';
import { isSqlBlock, parseInline, parseMarkdown, type Block } from './markdown';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('parseInline', () => {
	it('parses code, strong, emphasis and strikethrough', () => {
		expect(parseInline('ca. **437 Zeilen** in `public.game_sessions` *etwa* ~~nie~~')).toEqual([
			{ type: 'text', text: 'ca. ' },
			{ type: 'strong', children: [{ type: 'text', text: '437 Zeilen' }] },
			{ type: 'text', text: ' in ' },
			{ type: 'code', text: 'public.game_sessions' },
			{ type: 'text', text: ' ' },
			{ type: 'em', children: [{ type: 'text', text: 'etwa' }] },
			{ type: 'text', text: ' ' },
			{ type: 'del', children: [{ type: 'text', text: 'nie' }] }
		]);
	});

	it('keeps snake_case identifiers and lone asterisks as text', () => {
		expect(parseInline('count(*) from game_sessions_v2 where a * b')).toEqual([
			{ type: 'text', text: 'count(*) from game_sessions_v2 where a * b' }
		]);
	});

	it('leaves markup inside code untouched and honours escapes', () => {
		expect(parseInline('`casino_*` and \\*literal\\*')).toEqual([
			{ type: 'code', text: 'casino_*' },
			{ type: 'text', text: ' and *literal*' }
		]);
	});

	it('parses links and line breaks', () => {
		expect(parseInline('see [the docs](https://example.com/a b)\nnext')).toEqual([
			{ type: 'text', text: 'see ' },
			{
				type: 'link',
				href: 'https://example.com/a',
				children: [{ type: 'text', text: 'the docs' }]
			},
			{ type: 'break' },
			{ type: 'text', text: 'next' }
		]);
	});

	it('keeps raw HTML as text', () => {
		expect(parseInline('<img src=x onerror=alert(1)>')).toEqual([
			{ type: 'text', text: '<img src=x onerror=alert(1)>' }
		]);
	});
});

describe('parseMarkdown', () => {
	it('splits paragraphs, headings and fenced code', () => {
		const blocks = parseMarkdown('## Ergebnis\n\nAlle Sessions:\n```sql\nSELECT 1;\n```\nDanach.');
		expect(blocks.map((b) => b.type)).toEqual(['heading', 'paragraph', 'code', 'paragraph']);
		expect(blocks[0]).toMatchObject({ level: 2 });
		expect(blocks[2]).toEqual({ type: 'code', lang: 'sql', text: 'SELECT 1;', closed: true });
	});

	it('marks an unfinished fence as open code', () => {
		expect(parseMarkdown('```sql\nSELECT 1;')).toEqual([
			{ type: 'code', lang: 'sql', text: 'SELECT 1;', closed: false }
		]);
	});

	it('parses ordered and nested lists', () => {
		const [list] = parseMarkdown('3. eins\n4. zwei\n   - innen\n5. drei');
		expect(list).toMatchObject({ type: 'list', ordered: true, start: 3 });
		const items = (list as Extract<Block, { type: 'list' }>).items;
		expect(items).toHaveLength(3);
		expect(items[1].map((b) => b.type)).toEqual(['paragraph', 'list']);
	});

	it('keeps a list going across blank lines between items', () => {
		const blocks = parseMarkdown('- a\n\n- b\n\nText');
		expect(blocks.map((b) => b.type)).toEqual(['list', 'paragraph']);
		expect((blocks[0] as Extract<Block, { type: 'list' }>).items).toHaveLength(2);
	});

	it('parses tables with alignment and escaped pipes', () => {
		const [table] = parseMarkdown('| type | total |\n|:--|--:|\n| solo | 12 |\n| a\\|b |');
		expect(table).toMatchObject({ type: 'table', align: ['left', 'right'] });
		const rows = (table as Extract<Block, { type: 'table' }>).rows;
		expect(rows).toHaveLength(2);
		expect(rows[1]).toEqual([[{ type: 'text', text: 'a|b' }], []]);
	});

	it('parses quotes and rules', () => {
		expect(parseMarkdown('> Hinweis\n\n---').map((b) => b.type)).toEqual(['quote', 'rule']);
	});

	it('offers the same SQL blocks as extractSqlBlocks', () => {
		const text =
			'```sql\nSELECT 1;\n```\n```js\nalert(1)\n```\n~~~PostgreSQL\nSELECT 2;\n~~~\n```sql\n\n```';
		const sql = parseMarkdown(text)
			.filter(isSqlBlock)
			.map((b) => b.text.trim());
		expect(sql).toEqual(extractSqlBlocks(text));
	});
});
