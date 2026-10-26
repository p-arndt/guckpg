import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { sql, PostgreSQL } from '@codemirror/lang-sql';
import { catalogHover, catalogMatches } from './sql-hover.js';

const tables = [
	{ schema: 'public', name: 'users', columns: ['id', 'Name'] },
	{ schema: 'audit', name: 'events', columns: ['id'] }
];

describe('catalogMatches', () => {
	it('lists relations and all matching columns without resolving aliases', () => {
		expect(catalogMatches('USERS', tables)).toEqual(['Relation: public.users']);
		expect(catalogMatches('id', tables)).toEqual([
			'Column: public.users.id',
			'Column: audit.events.id'
		]);
	});

	it('respects PostgreSQL quoted identifier case', () => {
		expect(catalogMatches('Name', tables)).toEqual([]);
		expect(catalogMatches('"Name"', tables)).toEqual(['Column: public.users.Name']);
	});
});

it('shows matches only on SQL identifiers, including quoted identifiers', () => {
	const state = EditorState.create({
		doc: `SELECT "Name", 'users' -- id`,
		extensions: [sql({ dialect: PostgreSQL })]
	});
	expect(catalogHover(state, 10, tables)?.matches).toEqual(['Column: public.users.Name']);
	expect(catalogHover(state, 18, tables)).toBeNull();
	expect(catalogHover(state, 27, tables)).toBeNull();
});
