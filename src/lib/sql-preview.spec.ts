import { describe, expect, it } from 'vitest';
import { sqlPreview } from './sql-preview.js';

describe('approval SQL preview', () => {
	it('leaves ordinary statements alone', () => {
		const sql = 'UPDATE t\n   SET a = 1\n\nWHERE id = 2;';
		expect(sqlPreview(sql)).toEqual({ text: sql, collapsed: false, lines: 4, chars: sql.length });
	});

	it('pulls a statement hidden below blank lines into view', () => {
		const sql = 'SELECT 1;' + '\n'.repeat(200) + 'DROP TABLE users;';
		const preview = sqlPreview(sql);
		expect(preview.text).toBe('SELECT 1;\n\nDROP TABLE users;');
		expect(preview.collapsed).toBe(true);
		expect(preview.lines).toBe(201);
	});

	it('treats lines of spaces and invisible characters as blank', () => {
		const sql = 'SELECT 1;\n  \t\n\u200b\u00a0\n\r\n\u2028DELETE FROM t;';
		const preview = sqlPreview(sql);
		expect(preview.text).toBe('SELECT 1;\n\nDELETE FROM t;');
		expect(preview.collapsed).toBe(true);
	});

	it('shortens long runs of spaces inside a line', () => {
		const preview = sqlPreview('SELECT 1;' + ' '.repeat(500) + 'DROP TABLE users;');
		expect(preview.text).toBe('SELECT 1;' + ' '.repeat(8) + 'DROP TABLE users;');
		expect(preview.collapsed).toBe(true);
		expect(preview.chars).toBe(526);
	});

	it('drops leading and trailing blank lines without flagging them', () => {
		expect(sqlPreview('\n\nSELECT 1;\n\n\n')).toMatchObject({
			text: 'SELECT 1;',
			collapsed: false,
			lines: 6
		});
	});

	it('counts characters, not UTF-16 units', () => {
		expect(sqlPreview("SELECT '🐘';").chars).toBe(11);
	});
});
