import { describe, expect, it } from 'vitest';
import { ident, literal, qualified, quoteIdent, valueLiteral } from './sql-quote';

describe('ident', () => {
	it('leaves plain lowercase names bare', () => {
		expect(ident('orders')).toBe('orders');
		expect(ident('a_1$')).toBe('a_1$');
	});

	it('quotes reserved words, mixed case and specials', () => {
		expect(ident('order')).toBe('"order"');
		expect(ident('user')).toBe('"user"');
		expect(ident('User')).toBe('"User"');
		expect(ident('1st')).toBe('"1st"');
		expect(ident('we"ird')).toBe('"we""ird"');
	});

	it('always quotes with quoteIdent', () => {
		expect(quoteIdent('price')).toBe('"price"');
	});
});

describe('qualified', () => {
	it('drops the public schema', () => {
		expect(qualified('public', 'order')).toBe('"order"');
		expect(qualified('audit', 'log')).toBe('audit.log');
		expect(qualified('My Schema', 'select')).toBe('"My Schema"."select"');
	});
});

describe('literal', () => {
	it('doubles single quotes', () => {
		expect(literal("O'Brien")).toBe("'O''Brien'");
		expect(literal('')).toBe("''");
	});

	it('uses an E literal with doubled backslashes like the backend', () => {
		expect(literal('C:\\temp')).toBe("E'C:\\\\temp'");
		expect(literal("a\\'b")).toBe("E'a\\\\''b'");
	});

	it('keeps numbers bare in valueLiteral', () => {
		expect(valueLiteral(' 42 ')).toBe('42');
		expect(valueLiteral('-1.5')).toBe('-1.5');
		expect(valueLiteral('12abc')).toBe("'12abc'");
		expect(valueLiteral('a\\b')).toBe("E'a\\\\b'");
	});
});
