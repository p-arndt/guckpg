import { describe, expect, it } from 'vitest';
import { splitStatements, statementAt } from './sql-text';

const texts = (sql: string) => splitStatements(sql).map((s) => s.text);

describe('splitStatements', () => {
	it('splits at semicolons and trims', () => {
		expect(texts('select 1;\n  select 2 ;')).toEqual(['select 1', 'select 2']);
	});

	it('keeps the last statement without semicolon', () => {
		expect(texts('select 1; select 2')).toEqual(['select 1', 'select 2']);
	});

	it('ignores semicolons in strings and identifiers', () => {
		expect(texts(`select 'a;b', "x;y" from t; select 'it''s;'`)).toEqual([
			`select 'a;b', "x;y" from t`,
			`select 'it''s;'`
		]);
	});

	it('handles escape strings', () => {
		expect(texts(`select E'a\\';b'; select 2`)).toEqual([`select E'a\\';b'`, 'select 2']);
	});

	it('ignores semicolons in comments', () => {
		expect(texts('-- a; b\nselect 1; /* x; /* nested; */ y; */ select 2')).toEqual([
			'-- a; b\nselect 1',
			'/* x; /* nested; */ y; */ select 2'
		]);
	});

	it('keeps dollar-quoted bodies together', () => {
		const fn = `create function f() returns int as $body$ begin return 1; end; $body$ language plpgsql`;
		expect(texts(`${fn}; do $$ begin perform 1; end $$;`)).toEqual([
			fn,
			'do $$ begin perform 1; end $$'
		]);
	});

	it('does not treat positional parameters as dollar quotes', () => {
		expect(texts('select $1; select 2')).toEqual(['select $1', 'select 2']);
	});

	it('drops empty statements', () => {
		expect(texts(' ; ;\n')).toEqual([]);
	});

	it('reports offsets into the script', () => {
		const sql = 'select 1;\n\nselect 2;';
		const [, second] = splitStatements(sql);
		expect(sql.slice(second.from, second.to)).toBe('select 2');
	});
});

describe('statementAt', () => {
	const sql = 'select 1;\n\nselect 2;\nselect 3';

	it('finds the statement under the cursor', () => {
		expect(statementAt(sql, 2)?.text).toBe('select 1');
		expect(statementAt(sql, sql.indexOf('2'))?.text).toBe('select 2');
		expect(statementAt(sql, sql.length)?.text).toBe('select 3');
	});

	it('uses the previous statement in trailing whitespace', () => {
		expect(statementAt(sql, sql.indexOf('\n'))?.text).toBe('select 1');
	});

	it('falls back to the first statement before any text', () => {
		expect(statementAt('  select 1', 0)?.text).toBe('select 1');
	});

	it('returns undefined for an empty script', () => {
		expect(statementAt('  ', 1)).toBeUndefined();
	});
});
