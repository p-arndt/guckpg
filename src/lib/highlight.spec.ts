import { describe, expect, it } from 'vitest';
import { highlightSql } from './highlight';

describe('highlightSql', () => {
	it('keeps the text intact and tags keywords, strings and numbers', () => {
		const sql = "SELECT count(*) AS total\nFROM games WHERE kind = 'solo' AND n > 3;";
		const tokens = highlightSql(sql);
		expect(tokens.map((t) => t.text).join('')).toBe(sql);
		const classOf = (text: string) => tokens.find((t) => t.text === text)?.classes;
		expect(classOf('SELECT')).toContain('tok-keyword');
		expect(classOf("'solo'")).toContain('tok-string');
		expect(classOf('3')).toContain('tok-number');
	});
});
