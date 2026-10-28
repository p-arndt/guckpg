import { describe, expect, it } from 'vitest';
import { createViewSql } from './view-builder.js';

describe('createViewSql', () => {
	it('quotes both identifiers and preserves the SELECT', () => {
		expect(createViewSql('public', ' Sales "report" ', 'SELECT 1;')).toBe(
			'CREATE VIEW "public"."Sales ""report""" AS\nSELECT 1;'
		);
	});
	it('rejects missing, invalid and overlong names', () => {
		expect(() => createViewSql('', 'v', 'SELECT 1')).toThrow();
		expect(() => createViewSql('public', '  ', 'SELECT 1')).toThrow();
		expect(() => createViewSql('public', 'a\0b', 'SELECT 1')).toThrow();
		expect(() => createViewSql('public', 'ä'.repeat(32), 'SELECT 1')).toThrow();
		expect(() => createViewSql('public', 'v', '')).toThrow();
	});
});

import { buildSql } from './query-builder.js';
import { importViewQuery, replaceViewSql, viewEditorContent } from './view-builder.js';

const tables = [
	{ schema: 'public', name: 'customers', columns: ['id', 'name'] },
	{ schema: 'other', name: 'customers', columns: ['id', 'name'] }
];
const sources = [{ schema: 'public', name: 'customers' }];

describe('importViewQuery', () => {
	it('reconstructs a simple PostgreSQL view using its actual dependency', () => {
		const model = importViewQuery(
			' SELECT customers.id, customers.name AS label FROM customers;',
			tables,
			sources
		);
		expect(model).not.toBeNull();
		expect(buildSql(model!).sql).toBe(
			'SELECT customers.id,\n       customers.name AS label\nFROM public.customers;'
		);
	});
	it('supports quoted identifiers, aliases and distinct', () => {
		const model = importViewQuery(
			'SELECT DISTINCT c.id, c.name AS "Name" FROM public.customers AS c;',
			tables,
			sources
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT DISTINCT c.id,\n       c.name AS "Name"\nFROM public.customers AS c;'
		);
	});
	it('rejects expressions, ambiguous dependencies and unknown columns', () => {
		for (const sql of [
			'SELECT count(*) FROM customers;',
			'SELECT missing FROM customers;',
			'SELECT wrong.id FROM customers;',
			'SELECT customers.name AS " padded " FROM customers;'
		])
			expect(importViewQuery(sql, tables, sources)).toBeNull();
		expect(importViewQuery('SELECT id FROM customers;', tables, [])).toBeNull();
		expect(
			importViewQuery('SELECT id FROM customers;', tables, [
				...sources,
				{ schema: 'other', name: 'customers' }
			])
		).toBeNull();
	});
});

describe('replaceViewSql', () => {
	it('preserves existing view options and quoted names exactly', () => {
		expect(
			replaceViewSql(
				'public',
				' sales ',
				{
					query: '',
					sources,
					searchPath: ['pg_catalog', 'public'],
					options: "security_barrier = 'true'"
				},
				'SELECT 1;'
			)
		).toBe(
			'CREATE OR REPLACE VIEW "public"." sales " WITH (security_barrier = \'true\') AS\nSELECT 1;'
		);
	});
});

const joinTables = [
	{ schema: 'public', name: 'orders', columns: ['id', 'customer_id', 'total', 'paid_at'] },
	{ schema: 'public', name: 'customers', columns: ['id', 'name'] }
];
const joinSources = joinTables.map(({ schema, name }) => ({ schema, name }));

describe('import visual joins and filters', () => {
	it('imports PostgreSQL parenthesized left joins and conjunctions', () => {
		const model = importViewQuery(
			`SELECT orders.id, customers.name FROM (orders LEFT JOIN customers ON ((orders.customer_id = customers.id))) WHERE ((orders.total > 10) AND (orders.paid_at IS NOT NULL));`,
			joinTables,
			joinSources
		);
		expect(model).not.toBeNull();
		expect(buildSql(model!).sql).toBe(
			'SELECT orders.id,\n       customers.name\nFROM public.orders\nLEFT JOIN public.customers ON orders.customer_id = customers.id\nWHERE (orders.total > 10\n  AND orders.paid_at IS NOT NULL);'
		);
	});
	it('preserves outer-join direction when the ON operands are reversed', () => {
		const model = importViewQuery(
			'SELECT orders.id FROM orders LEFT JOIN customers ON customers.id = orders.customer_id;',
			joinTables,
			joinSources
		);
		expect(model && buildSql(model).sql).toContain(
			'LEFT JOIN public.customers ON customers.id = orders.customer_id'
		);
	});
	it('imports OR filters and plain nonnumeric strings', () => {
		const model = importViewQuery(
			"SELECT customers.name FROM customers WHERE ((customers.name = 'Alice') OR (customers.name IS NULL));",
			tables,
			sources
		);
		expect(model && buildSql(model).sql).toContain(
			"WHERE (customers.name = 'Alice'\n  OR customers.name IS NULL)"
		);
	});
	it('rejects constructs that cannot be represented safely', () => {
		for (const sql of [
			'SELECT orders.id FROM orders LEFT JOIN customers ON orders.id = customers.id OR orders.customer_id = customers.id;',
			"SELECT customers.name FROM customers WHERE customers.name = '42';",
			"SELECT customers.name FROM customers WHERE customers.name = 'Alice'::text;"
		])
			expect(
				importViewQuery(
					sql,
					[...tables, ...joinTables],
					sql.includes('orders') && sql.includes('JOIN')
						? joinSources
						: sql.includes('orders')
							? [joinSources[0]]
							: sources
				)
			).toBeNull();
	});
});

describe('import sorting and limits', () => {
	it('imports output aliases, ordering and numeric limits', () => {
		const model = importViewQuery(
			'SELECT customers.name AS label FROM customers ORDER BY label DESC LIMIT 10 OFFSET 2;',
			tables,
			sources
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT customers.name AS label\nFROM public.customers\nORDER BY customers.name DESC\nLIMIT 10\nOFFSET 2;'
		);
	});
	it('rejects ordering that the builder cannot preserve', () => {
		expect(
			importViewQuery(
				'SELECT customers.name FROM customers ORDER BY customers.name DESC NULLS LAST;',
				tables,
				sources
			)
		).toBeNull();
	});
});

describe('typed text filters', () => {
	it('preserves explicit PostgreSQL text casts and the exact string value', () => {
		const model = importViewQuery(
			"SELECT customers.name FROM customers WHERE customers.name = ' 001 '::text;",
			tables,
			sources,
			['pg_catalog', 'public']
		);
		expect(model && buildSql(model).sql).toContain(
			"WHERE customers.name = ' 001 '::pg_catalog.text"
		);
	});
	it('rejects a text cast when it could resolve outside pg_catalog', () => {
		expect(
			importViewQuery(
				"SELECT customers.name FROM customers WHERE customers.name = 'x'::text;",
				tables,
				sources,
				['public', 'pg_catalog']
			)
		).toBeNull();
	});
});

describe('import repeated and multiple joins', () => {
	it('keeps aliases for self joins', () => {
		const model = importViewQuery(
			'SELECT a.id, b.name FROM (customers a JOIN customers b ON (a.id = b.id));',
			tables,
			sources
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT a.id,\n       b.name\nFROM public.customers AS a\nJOIN public.customers AS b ON a.id = b.id;'
		);
	});
	it('keeps PostgreSQL left-associated join groups', () => {
		const related = [
			...joinTables,
			{ schema: 'public', name: 'payments', columns: ['id', 'order_id'] }
		];
		const dependencies = related.map(({ schema, name }) => ({ schema, name }));
		const model = importViewQuery(
			'SELECT orders.id, payments.id AS payment FROM ((orders JOIN customers ON (orders.customer_id = customers.id)) LEFT JOIN payments ON (orders.id = payments.order_id));',
			related,
			dependencies
		);
		expect(model && buildSql(model).sql).toContain(
			'JOIN public.customers ON orders.customer_id = customers.id\nLEFT JOIN public.payments ON orders.id = payments.order_id'
		);
	});
});

describe('import aggregates and grouped conditions', () => {
	it('imports aggregates with matching GROUP BY and sorts by an aggregate alias', () => {
		const model = importViewQuery(
			'SELECT orders.customer_id, sum(orders.total) AS revenue FROM orders GROUP BY orders.customer_id ORDER BY revenue DESC;',
			joinTables,
			[joinSources[0]],
			['pg_catalog', 'public']
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT orders.customer_id,\n       pg_catalog.SUM(orders.total) AS revenue\nFROM public.orders\nGROUP BY orders.customer_id\nORDER BY revenue DESC;'
		);
	});
	it('rejects aggregates resolved outside pg_catalog and grouping the builder cannot express', () => {
		const sql =
			'SELECT orders.customer_id, sum(orders.total) AS revenue FROM orders GROUP BY orders.customer_id;';
		expect(importViewQuery(sql, joinTables, [joinSources[0]], ['public', 'pg_catalog'])).toBeNull();
		expect(
			importViewQuery(
				'SELECT sum(orders.total) FROM orders GROUP BY orders.customer_id;',
				joinTables,
				[joinSources[0]],
				['pg_catalog']
			)
		).toBeNull();
	});
	it('preserves nested AND and OR groups', () => {
		const model = importViewQuery(
			'SELECT orders.id FROM orders WHERE ((orders.id = 1 OR orders.id = 2) AND (orders.total > 10 OR orders.paid_at IS NULL));',
			joinTables,
			[joinSources[0]]
		);
		expect(model && buildSql(model).sql).toContain(
			'WHERE ((orders.id = 1\n  OR orders.id = 2)\n  AND (orders.total > 10\n  OR orders.paid_at IS NULL))'
		);
	});
});

describe('COUNT(*)', () => {
	it('preserves COUNT(*) in the projection and order', () => {
		const model = importViewQuery(
			'SELECT orders.customer_id, count(*) AS order_count FROM orders GROUP BY orders.customer_id ORDER BY order_count DESC;',
			joinTables,
			[joinSources[0]],
			['pg_catalog', 'public']
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT orders.customer_id,\n       pg_catalog.COUNT(*) AS order_count\nFROM public.orders\nGROUP BY orders.customer_id\nORDER BY order_count DESC;'
		);
	});
	it('rejects star arguments for other aggregates', () => {
		expect(
			importViewQuery('SELECT sum(*) FROM orders;', joinTables, [joinSources[0]], ['pg_catalog'])
		).toBeNull();
	});
});

describe('distinct aggregates and HAVING', () => {
	it('imports COUNT(DISTINCT ...) and HAVING with matching GROUP BY', () => {
		const model = importViewQuery(
			'SELECT orders.customer_id, count(DISTINCT orders.id) AS n FROM orders GROUP BY orders.customer_id HAVING (count(DISTINCT orders.id) > 2) ORDER BY n DESC;',
			joinTables,
			[joinSources[0]],
			['pg_catalog', 'public']
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT orders.customer_id,\n       pg_catalog.COUNT(DISTINCT orders.id) AS n\nFROM public.orders\nGROUP BY orders.customer_id\nHAVING pg_catalog.COUNT(DISTINCT orders.id) > 2\nORDER BY n DESC;'
		);
	});
	it('supports HAVING aggregates absent from the projection', () => {
		const model = importViewQuery(
			'SELECT orders.customer_id FROM orders GROUP BY orders.customer_id HAVING count(*) > 1;',
			joinTables,
			[joinSources[0]],
			['pg_catalog']
		);
		expect(model && buildSql(model).sql).toBe(
			'SELECT orders.customer_id\nFROM public.orders\nGROUP BY orders.customer_id\nHAVING pg_catalog.COUNT(*) > 1;'
		);
	});
	it('keeps grouped HAVING conditions and rejects invalid distinct star', () => {
		const model = importViewQuery(
			'SELECT count(*) AS n FROM orders HAVING (count(*) > 1 OR sum(orders.total) > 10);',
			joinTables,
			[joinSources[0]],
			['pg_catalog']
		);
		expect(model && buildSql(model).sql).toContain(
			'HAVING (pg_catalog.COUNT(*) > 1\n  OR pg_catalog.SUM(orders.total) > 10)'
		);
		expect(
			importViewQuery(
				'SELECT count(DISTINCT *) FROM orders;',
				joinTables,
				[joinSources[0]],
				['pg_catalog']
			)
		).toBeNull();
	});
});

describe('PostgreSQL 18 view definitions', () => {
	it('imports a deparsed grouped view with DISTINCT and HAVING', () => {
		const sql = ` SELECT customer_id,
    count(DISTINCT id) AS total
   FROM visual_fixture.orders
  WHERE ((amount > 10) OR (id = 3))
  GROUP BY customer_id
 HAVING (count(DISTINCT id) > 1);`;
		const model = importViewQuery(
			sql,
			[{ schema: 'visual_fixture', name: 'orders', columns: ['customer_id', 'id', 'amount'] }],
			[{ schema: 'visual_fixture', name: 'orders' }],
			['pg_catalog', 'public']
		);
		expect(model).not.toBeNull();
		expect(buildSql(model!).sql).toContain('HAVING pg_catalog.COUNT(DISTINCT orders.id) > 1');
	});
});

describe('boolean view filters', () => {
	const boolTables = [
		{ schema: 'public', name: 'customers', columns: ['id', 'active', 'verified'] }
	];
	const boolSources = [{ schema: 'public', name: 'customers' }];
	it('imports a bare boolean filter from PostgreSQL', () => {
		const model = importViewQuery(
			'SELECT customers.id FROM customers WHERE customers.active;',
			boolTables,
			boolSources
		);
		expect(model && buildSql(model).sql).toContain('WHERE customers.active IS TRUE');
	});
	it('preserves boolean tests in grouped expressions', () => {
		const model = importViewQuery(
			'SELECT customers.id FROM customers WHERE (NOT customers.active OR customers.verified IS NOT TRUE);',
			boolTables,
			boolSources
		);
		expect(model && buildSql(model).sql).toContain(
			'WHERE (customers.active IS FALSE\n  OR customers.verified IS NOT TRUE)'
		);
	});
});

describe('view SQL editor handoff', () => {
	it('preserves existing editor text and replaces the generated view statement on later handoffs', () => {
		const first = viewEditorContent(
			'SELECT 1;',
			null,
			null,
			'CREATE OR REPLACE VIEW v AS SELECT 1;'
		);
		expect(first.sql).toBe('SELECT 1;\n\nCREATE OR REPLACE VIEW v AS SELECT 1;\n');
		const next = viewEditorContent(
			first.sql,
			first.sql,
			first.prefix,
			'CREATE OR REPLACE VIEW v AS SELECT 2;'
		);
		expect(next.sql).toBe('SELECT 1;\n\nCREATE OR REPLACE VIEW v AS SELECT 2;\n');
	});
});
