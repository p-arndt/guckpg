import { describe, expect, it } from 'vitest';
import type { ForeignKey } from './api';
import {
	addTable,
	buildSql,
	emptyModel,
	foreignKeyJoins,
	ident,
	literal,
	removeTable,
	toggleColumn,
	type BuilderModel
} from './query-builder';

const orderFk: ForeignKey = {
	name: 'orders_customer_id_fkey',
	fromTable: 'orders',
	fromColumns: ['customer_id'],
	toTable: 'customers',
	toColumns: ['id']
};

function ordersAndCustomers(): BuilderModel {
	const model = emptyModel();
	addTable(model, 'public', 'orders');
	const customers = addTable(model, 'public', 'customers');
	model.joins.push(...foreignKeyJoins(model, customers, [orderFk]));
	return model;
}

describe('ident and literal', () => {
	it('quotes identifiers only when needed', () => {
		expect(ident('orders')).toBe('orders');
		expect(ident('order')).toBe('"order"');
		expect(ident('User')).toBe('"User"');
		expect(ident('we"ird')).toBe('"we""ird"');
	});

	it('keeps numbers bare and escapes strings', () => {
		expect(literal('42')).toBe('42');
		expect(literal('-1.5')).toBe('-1.5');
		expect(literal("O'Brien")).toBe("'O''Brien'");
		expect(literal('12abc')).toBe("'12abc'");
	});
});

describe('buildSql', () => {
	it('asks for a table when the canvas is empty', () => {
		expect(buildSql(emptyModel())).toEqual({ sql: '', problems: ['Add a table to start.'] });
	});

	it('selects everything from a single table', () => {
		const model = emptyModel();
		addTable(model, 'public', 'customers');
		expect(buildSql(model).sql).toBe('SELECT *\nFROM public.customers;');
	});

	it('qualifies tables outside public', () => {
		const model = emptyModel();
		addTable(model, 'sales', 'Orders');
		expect(buildSql(model).sql).toBe('SELECT *\nFROM sales."Orders";');
	});

	it('joins along foreign keys and writes readable SQL', () => {
		const model = ordersAndCustomers();
		const [orders, customers] = model.tables;
		toggleColumn(model, { tableId: orders.id, column: 'id' });
		toggleColumn(model, { tableId: customers.id, column: 'name' });
		model.filters.push({
			id: 'f1',
			connector: 'AND',
			column: { tableId: customers.id, column: 'name' },
			operator: 'ILIKE',
			value: "%o'b%"
		});
		model.sorts.push({ id: 's1', column: { tableId: orders.id, column: 'id' }, direction: 'DESC' });
		model.limit = '50';
		expect(buildSql(model)).toEqual({
			sql: [
				'SELECT orders.id,',
				'       customers.name',
				'FROM public.orders',
				'JOIN public.customers ON orders.customer_id = customers.id',
				"WHERE customers.name ILIKE '%o''b%'",
				'ORDER BY orders.id DESC',
				'LIMIT 50;'
			].join('\n'),
			problems: []
		});
	});

	it('flips LEFT to RIGHT when the kept side is joined second', () => {
		const model = emptyModel();
		const customers = addTable(model, 'public', 'customers');
		const orders = addTable(model, 'public', 'orders');
		// customers come first in FROM; the join keeps every order.
		model.joins.push({
			id: 'j',
			type: 'LEFT',
			from: { tableId: orders.id, column: 'customer_id' },
			to: { tableId: customers.id, column: 'id' }
		});
		expect(buildSql(model).sql).toContain('RIGHT JOIN public.orders ON');
	});

	it('cross joins and warns about tables without a join', () => {
		const model = emptyModel();
		addTable(model, 'public', 'a');
		addTable(model, 'public', 'b');
		const built = buildSql(model);
		expect(built.sql).toBe('SELECT *\nFROM public.a\nCROSS JOIN public.b;');
		expect(built.problems).toHaveLength(1);
	});

	it('aliases a table used twice', () => {
		const model = emptyModel();
		addTable(model, 'public', 'employees');
		const manager = addTable(model, 'public', 'employees');
		expect(manager.alias).toBe('employees_2');
		expect(buildSql(model).sql).toContain('CROSS JOIN public.employees AS employees_2');
	});

	it('groups by the plain columns when one is aggregated', () => {
		const model = ordersAndCustomers();
		const [orders, customers] = model.tables;
		toggleColumn(model, { tableId: customers.id, column: 'name' });
		toggleColumn(model, { tableId: orders.id, column: 'id' });
		model.columns[1].aggregate = 'COUNT';
		model.columns[1].alias = 'order_count';
		const { sql } = buildSql(model);
		expect(sql).toContain('COUNT(orders.id) AS order_count');
		expect(sql).toContain('GROUP BY customers.name');
	});

	it('sorts by the aggregate, not the raw column, when grouping', () => {
		const model = ordersAndCustomers();
		const [orders, customers] = model.tables;
		toggleColumn(model, { tableId: customers.id, column: 'name' });
		toggleColumn(model, { tableId: orders.id, column: 'total' });
		model.columns[1].aggregate = 'SUM';
		const total = { tableId: orders.id, column: 'total' };
		model.sorts.push({ id: 's', column: total, direction: 'DESC' });
		expect(buildSql(model).sql).toContain('ORDER BY SUM(orders.total) DESC');
		model.columns[1].alias = 'revenue';
		expect(buildSql(model).sql).toContain('ORDER BY revenue DESC');

		model.sorts.push({
			id: 't',
			column: { tableId: orders.id, column: 'status' },
			direction: 'ASC'
		});
		expect(buildSql(model).problems).toEqual([
			'Sorting by orders.status needs it as a column or an aggregate.'
		]);
	});

	it('combines filters with their connectors, IN lists and null checks', () => {
		const model = emptyModel();
		const t = addTable(model, 'public', 'orders');
		const column = (name: string) => ({ tableId: t.id, column: name });
		model.filters.push(
			{ id: '1', connector: 'AND', column: column('status'), operator: 'IN', value: 'new, paid' },
			{ id: '2', connector: 'OR', column: column('paid_at'), operator: 'IS NULL', value: 'x' },
			{ id: '3', connector: 'AND', column: null, operator: '=', value: '' }
		);
		expect(buildSql(model).sql).toContain(
			"WHERE orders.status IN ('new', 'paid')\n  OR orders.paid_at IS NULL"
		);
	});

	it('reports a limit that is not a number and leaves it out', () => {
		const model = emptyModel();
		addTable(model, 'public', 'orders');
		model.limit = 'ten';
		model.offset = '20';
		const built = buildSql(model);
		expect(built.sql).toBe('SELECT *\nFROM public.orders\nOFFSET 20;');
		expect(built.problems).toEqual(['LIMIT must be a whole number.']);
	});

	it('adds DISTINCT', () => {
		const model = emptyModel();
		addTable(model, 'public', 'orders');
		model.distinct = true;
		expect(buildSql(model).sql).toBe('SELECT DISTINCT *\nFROM public.orders;');
	});
});

describe('model edits', () => {
	it('finds foreign keys in either direction and ignores other schemas', () => {
		const model = emptyModel();
		addTable(model, 'public', 'customers');
		addTable(model, 'other', 'customers');
		const orders = addTable(model, 'public', 'orders');
		const joins = foreignKeyJoins(model, orders, [orderFk]);
		expect(joins).toHaveLength(1);
		expect(joins[0].from).toEqual({ tableId: orders.id, column: 'customer_id' });
		expect(joins[0].to.tableId).toBe(model.tables[0].id);
	});

	it('removing a table drops everything that points at it', () => {
		const model = ordersAndCustomers();
		const [orders, customers] = model.tables;
		toggleColumn(model, { tableId: customers.id, column: 'name' });
		toggleColumn(model, { tableId: orders.id, column: 'id' });
		model.sorts.push({
			id: 's',
			column: { tableId: customers.id, column: 'name' },
			direction: 'ASC'
		});
		removeTable(model, customers.id);
		expect(model.tables.map((t) => t.name)).toEqual(['orders']);
		expect(model.joins).toEqual([]);
		expect(model.sorts).toEqual([]);
		expect(model.columns.map((c) => c.column)).toEqual(['id']);
	});

	it('toggles a column on and off', () => {
		const model = emptyModel();
		const t = addTable(model, 'public', 'orders');
		toggleColumn(model, { tableId: t.id, column: 'id' });
		expect(model.columns).toHaveLength(1);
		toggleColumn(model, { tableId: t.id, column: 'id' });
		expect(model.columns).toHaveLength(0);
	});
});

describe('condition groups', () => {
	it('blocks unmatched parentheses instead of running a different query', () => {
		const model = emptyModel();
		const table = addTable(model, 'public', 'orders');
		model.filters.push({
			id: 'f',
			connector: 'AND',
			column: { tableId: table.id, column: 'id' },
			operator: '=',
			value: '1',
			openGroups: 1
		});
		expect(buildSql(model)).toEqual({ sql: '', problems: ['Check the condition parentheses.'] });
	});
});
