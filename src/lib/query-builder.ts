import type { ForeignKey } from './api.js';

export const JOIN_TYPES = ['INNER', 'LEFT', 'RIGHT', 'FULL'] as const;
export type JoinType = (typeof JOIN_TYPES)[number];

export const AGGREGATES = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX'] as const;
export type Aggregate = (typeof AGGREGATES)[number];

export const OPERATORS = [
	'=',
	'<>',
	'<',
	'<=',
	'>',
	'>=',
	'LIKE',
	'NOT LIKE',
	'ILIKE',
	'IN',
	'NOT IN',
	'IS NULL',
	'IS NOT NULL'
] as const;
export type Operator = (typeof OPERATORS)[number];

export function takesValue(operator: Operator): boolean {
	return operator !== 'IS NULL' && operator !== 'IS NOT NULL';
}

export interface ColumnRef {
	tableId: string;
	column: string;
}

export interface BuilderTable {
	id: string;
	schema: string;
	name: string;
	/** Unique within the query; the table name unless the table is used twice. */
	alias: string;
	x: number;
	y: number;
}

export interface SelectedColumn extends ColumnRef {
	aggregate: Aggregate | null;
	alias: string;
}

/** `from` keeps its rows for LEFT, `to` for RIGHT. */
export interface BuilderJoin {
	id: string;
	type: JoinType;
	from: ColumnRef;
	to: ColumnRef;
}

export interface BuilderFilter {
	id: string;
	connector: 'AND' | 'OR';
	column: ColumnRef | null;
	operator: Operator;
	value: string;
}

export interface BuilderSort {
	id: string;
	column: ColumnRef | null;
	direction: 'ASC' | 'DESC';
}

export interface BuilderModel {
	tables: BuilderTable[];
	columns: SelectedColumn[];
	joins: BuilderJoin[];
	filters: BuilderFilter[];
	sorts: BuilderSort[];
	distinct: boolean;
	limit: string;
	offset: string;
}

export function emptyModel(): BuilderModel {
	return {
		tables: [],
		columns: [],
		joins: [],
		filters: [],
		sorts: [],
		distinct: false,
		limit: '',
		offset: ''
	};
}

let counter = 0;
export function newId(prefix: string): string {
	counter += 1;
	return `${prefix}${counter}`;
}

const RESERVED = new Set(
	`all analyse analyze and any array as asc asymmetric authorization binary both case cast check
	collate collation column concurrently constraint create cross current_catalog current_date
	current_role current_schema current_time current_timestamp current_user default deferrable desc
	distinct do else end except false fetch for foreign freeze from full grant group having ilike in
	initially inner intersect into is isnull join lateral leading left like limit localtime
	localtimestamp natural not notnull null offset on only or order outer overlaps placing primary
	references returning right select session_user similar some symmetric system_user table
	tablesample then to trailing true union unique user using variadic verbose when where window with`.split(
		/\s+/
	)
);

/** An identifier as written by hand: bare when Postgres would read it back the same. */
export function ident(name: string): string {
	if (/^[a-z_][a-z0-9_$]*$/.test(name) && !RESERVED.has(name)) return name;
	return `"${name.replaceAll('"', '""')}"`;
}

/** Numbers stay bare; everything else is a string literal Postgres casts to the column type. */
export function literal(value: string): string {
	if (/^-?\d+(\.\d+)?$/.test(value.trim())) return value.trim();
	return `'${value.replaceAll("'", "''")}'`;
}

function tableSql(table: BuilderTable): string {
	const name =
		table.schema === 'public' ? ident(table.name) : `${ident(table.schema)}.${ident(table.name)}`;
	return table.alias === table.name ? name : `${name} AS ${ident(table.alias)}`;
}

export function uniqueAlias(model: BuilderModel, name: string): string {
	const used = new Set(model.tables.map((table) => table.alias));
	if (!used.has(name)) return name;
	let n = 2;
	while (used.has(`${name}_${n}`)) n += 1;
	return `${name}_${n}`;
}

export function addTable(
	model: BuilderModel,
	schema: string,
	name: string,
	position?: { x: number; y: number }
): BuilderTable {
	const n = model.tables.length;
	const table: BuilderTable = {
		id: newId('t'),
		schema,
		name,
		alias: uniqueAlias(model, name),
		x: position?.x ?? 40 + (n % 3) * 290,
		y: position?.y ?? 40 + Math.floor(n / 3) * 280 + (n % 2) * 40
	};
	model.tables.push(table);
	return table;
}

export function removeTable(model: BuilderModel, tableId: string) {
	const keep = (ref: ColumnRef | null) => ref?.tableId !== tableId;
	model.tables = model.tables.filter((table) => table.id !== tableId);
	model.columns = model.columns.filter(keep);
	model.joins = model.joins.filter((join) => keep(join.from) && keep(join.to));
	model.filters = model.filters.filter((filter) => keep(filter.column));
	model.sorts = model.sorts.filter((sort) => keep(sort.column));
}

export function isSelected(model: BuilderModel, ref: ColumnRef): boolean {
	return model.columns.some((c) => c.tableId === ref.tableId && c.column === ref.column);
}

export function toggleColumn(model: BuilderModel, ref: ColumnRef) {
	if (isSelected(model, ref)) {
		model.columns = model.columns.filter(
			(c) => !(c.tableId === ref.tableId && c.column === ref.column)
		);
	} else {
		model.columns.push({ ...ref, aggregate: null, alias: '' });
	}
}

/**
 * Joins the new table to each table already in the query that it shares a
 * foreign key with. One key per pair: a second key between the same tables
 * usually means something else (billing vs. shipping address).
 */
export function foreignKeyJoins(
	model: BuilderModel,
	added: BuilderTable,
	foreignKeys: ForeignKey[]
): BuilderJoin[] {
	const joins: BuilderJoin[] = [];
	for (const other of model.tables) {
		if (other.id === added.id || other.schema !== added.schema) continue;
		const fk = foreignKeys.find(
			(key) =>
				(key.fromTable === added.name && key.toTable === other.name) ||
				(key.fromTable === other.name && key.toTable === added.name)
		);
		if (!fk) continue;
		const [from, to] = fk.fromTable === added.name ? [added, other] : [other, added];
		fk.fromColumns.forEach((column, i) =>
			joins.push({
				id: newId('j'),
				type: 'INNER',
				from: { tableId: from.id, column },
				to: { tableId: to.id, column: fk.toColumns[i] }
			})
		);
	}
	return joins;
}

const flipped: Record<JoinType, JoinType> = {
	INNER: 'INNER',
	LEFT: 'RIGHT',
	RIGHT: 'LEFT',
	FULL: 'FULL'
};

export interface BuiltQuery {
	sql: string;
	problems: string[];
}

export function buildSql(model: BuilderModel): BuiltQuery {
	const problems: string[] = [];
	if (model.tables.length === 0) return { sql: '', problems: ['Add a table to start.'] };

	const byId = new Map(model.tables.map((table) => [table.id, table]));
	const col = (ref: ColumnRef) => `${ident(byId.get(ref.tableId)!.alias)}.${ident(ref.column)}`;

	const select = model.columns.map((c) => {
		const expression = c.aggregate ? `${c.aggregate}(${col(c)})` : col(c);
		return c.alias.trim() ? `${expression} AS ${ident(c.alias.trim())}` : expression;
	});
	const lines = [
		`SELECT ${model.distinct ? 'DISTINCT ' : ''}${select.length ? select.join(',\n       ') : '*'}`
	];

	// FROM the first table, then each table that joins onto what is already there.
	const placed = new Set([model.tables[0].id]);
	lines.push(`FROM ${tableSql(model.tables[0])}`);
	const pending = model.tables.slice(1);
	while (pending.length) {
		const index = pending.findIndex((table) =>
			model.joins.some(
				(join) =>
					(join.from.tableId === table.id && placed.has(join.to.tableId)) ||
					(join.to.tableId === table.id && placed.has(join.from.tableId))
			)
		);
		if (index === -1) {
			const table = pending.shift()!;
			problems.push(`${table.alias} is not joined, so every row pairs with every other row.`);
			lines.push(`CROSS JOIN ${tableSql(table)}`);
			placed.add(table.id);
			continue;
		}
		const [table] = pending.splice(index, 1);
		const conditions = model.joins.filter(
			(join) =>
				(join.from.tableId === table.id && placed.has(join.to.tableId)) ||
				(join.to.tableId === table.id && placed.has(join.from.tableId))
		);
		const first = conditions[0];
		const type = first.to.tableId === table.id ? first.type : flipped[first.type];
		const on = conditions.map((join) => `${col(join.from)} = ${col(join.to)}`).join(' AND ');
		lines.push(`${type === 'INNER' ? 'JOIN' : `${type} JOIN`} ${tableSql(table)} ON ${on}`);
		placed.add(table.id);
	}

	const filters = model.filters.filter((filter) => filter.column);
	if (filters.length) {
		const parts = filters.map((filter, i) => {
			let expression = `${col(filter.column!)} ${filter.operator}`;
			if (filter.operator === 'IN' || filter.operator === 'NOT IN') {
				const values = filter.value
					.split(',')
					.map((v) => v.trim())
					.filter(Boolean);
				expression += ` (${values.map(literal).join(', ')})`;
				if (!values.length)
					problems.push(`${col(filter.column!)} ${filter.operator} needs values.`);
			} else if (takesValue(filter.operator)) {
				expression += ` ${literal(filter.value)}`;
			}
			return i === 0 ? expression : `${filter.connector} ${expression}`;
		});
		lines.push(`WHERE ${parts.join('\n  ')}`);
	}

	const aggregating = model.columns.some((c) => c.aggregate);
	const grouped = aggregating ? model.columns.filter((c) => !c.aggregate).map(col) : [];
	if (grouped.length) lines.push(`GROUP BY ${grouped.join(', ')}`);

	const sorts = model.sorts.filter((sort) => sort.column);
	if (sorts.length) {
		const terms = sorts.map((sort) => {
			const ref = sort.column!;
			// With an aggregate in the list, sort by what the query returns, not the raw column.
			const aggregated = model.columns.find(
				(c) => c.aggregate && c.tableId === ref.tableId && c.column === ref.column
			);
			let term = col(ref);
			if (aggregated) {
				term = aggregated.alias.trim()
					? ident(aggregated.alias.trim())
					: `${aggregated.aggregate}(${col(ref)})`;
			} else if (grouped.length && !grouped.includes(col(ref))) {
				problems.push(`Sorting by ${col(ref)} needs it as a column or an aggregate.`);
			}
			return sort.direction === 'DESC' ? `${term} DESC` : term;
		});
		lines.push(`ORDER BY ${terms.join(', ')}`);
	}

	for (const [key, clause] of [
		['limit', 'LIMIT'],
		['offset', 'OFFSET']
	] as const) {
		const value = model[key].trim();
		if (!value) continue;
		if (/^\d+$/.test(value)) lines.push(`${clause} ${value}`);
		else problems.push(`${clause} must be a whole number.`);
	}

	return { sql: lines.join('\n') + ';', problems };
}
