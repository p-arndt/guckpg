import { describe, expect, it } from 'vitest';
import { highlightJson, highlightSql, isJsonType } from './highlight';

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

describe('highlightJson', () => {
	it('keeps the text intact and tells keys from string values', () => {
		const json = '{"width": 12, "name": "box", "ok": true, "gone": null}';
		const tokens = highlightJson(json);
		expect(tokens.map((t) => t.text).join('')).toBe(json);
		const classOf = (text: string) => tokens.find((t) => t.text === text)?.classes;
		expect(classOf('"width"')).toContain('tok-propertyName');
		expect(classOf('"box"')).toContain('tok-string');
		expect(classOf('12')).toContain('tok-number');
		expect(classOf('true')).toContain('tok-bool');
		expect(classOf('null')).toContain('tok-keyword');
	});

	it('still highlights JSON that was cut off', () => {
		const json = '{"width": 12, "na';
		expect(
			highlightJson(json)
				.map((t) => t.text)
				.join('')
		).toBe(json);
	});
});

describe('isJsonType', () => {
	it('matches json and jsonb only', () => {
		expect(isJsonType('jsonb')).toBe(true);
		expect(isJsonType('JSON')).toBe(true);
		expect(isJsonType('jsonb[]')).toBe(true);
		expect(isJsonType('jsonpath')).toBe(false);
		expect(isJsonType('text')).toBe(false);
		expect(isJsonType(undefined)).toBe(false);
	});
});
