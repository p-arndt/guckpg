import type { CompletionTable, ViewDetails } from './api.js';
import {
	addTable,
	emptyModel,
	newId,
	AGGREGATES,
	type Aggregate,
	type BuilderModel,
	type ColumnRef,
	type JoinType,
	type Operator
} from './query-builder.js';

interface Ref {
	qualifier: string | null;
	name: string;
}
interface Selection {
	ref: Ref;
	alias: string;
	aggregate: Aggregate | null;
	aggregateDistinct: boolean;
}
interface Predicate {
	ref: Ref;
	operator: Operator;
	value: string;
	literalCast?: 'text';
	connector?: 'AND' | 'OR';
	openGroups?: number;
	closeGroups?: number;
	aggregate?: Aggregate | null;
	aggregateDistinct?: boolean;
}

/** Accept only syntax that can be reconstructed without changing its meaning. */
export function importViewQuery(
	query: string,
	tables: CompletionTable[],
	sources: ViewDetails['sources'],
	searchPath: string[] = []
): BuilderModel | null {
	try {
		return parse(query, tables, sources, searchPath);
	} catch {
		return null;
	}
}

function parse(
	query: string,
	tables: CompletionTable[],
	sources: ViewDetails['sources'],
	searchPath: string[]
): BuilderModel {
	const tokens: string[] = [];
	const lexer =
		/\s+|"(?:[^"]|"")*"|'(?:[^']|'')*'|-?\d+(?:\.\d+)?|[a-zA-Z_][a-zA-Z_0-9$]*|::|<>|<=|>=|[(),.;*=<>]/gy;
	let offset = 0;
	while (offset < query.length) {
		lexer.lastIndex = offset;
		const match = lexer.exec(query);
		if (!match) throw new Error('Unsupported SQL');
		offset = lexer.lastIndex;
		if (!/^\s+$/.test(match[0])) tokens.push(match[0]);
	}
	let at = 0;
	const take = (value: string) => (tokens[at]?.toUpperCase() === value ? (++at, true) : false);
	const expect = (value: string) => {
		if (!take(value)) throw new Error(`Expected ${value}`);
	};
	const identifier = () => {
		const value = tokens[at++];
		if (!value || !/^(?:"(?:[^"]|"")*"|[a-zA-Z_][a-zA-Z_0-9$]*)$/.test(value))
			throw new Error('Expected identifier');
		const name = value.startsWith('"')
			? value.slice(1, -1).replaceAll('""', '"')
			: value.toLowerCase();
		if (name === '*') throw new Error('Unsupported identifier');
		return name;
	};
	const ref = (): Ref => {
		const first = identifier();
		return take('.') ? { qualifier: first, name: identifier() } : { qualifier: null, name: first };
	};
	const expression = (): { ref: Ref; aggregate: Aggregate | null; aggregateDistinct: boolean } => {
		const start = at;
		const first = identifier();
		let fn = first;
		let catalogQualified = false;
		if (first === 'pg_catalog' && take('.')) {
			fn = identifier();
			catalogQualified = true;
		}
		if (take('(')) {
			const aggregate = fn.toUpperCase() as Aggregate;
			if (!AGGREGATES.includes(aggregate) || (!catalogQualified && searchPath[0] !== 'pg_catalog'))
				throw new Error('Unsupported aggregate');
			const aggregateDistinct = take('DISTINCT');
			const star = take('*');
			if (star && (aggregate !== 'COUNT' || aggregateDistinct))
				throw new Error('Unsupported aggregate');
			const column = star ? { qualifier: null, name: '*' } : ref();
			expect(')');
			return { ref: column, aggregate, aggregateDistinct };
		}
		at = start;
		return { ref: ref(), aggregate: null, aggregateDistinct: false };
	};
	const model = emptyModel();
	const selected: Selection[] = [];
	expect('SELECT');
	model.distinct = take('DISTINCT');
	if (!take('*')) {
		do {
			const column = expression();
			selected.push({ ...column, alias: take('AS') ? identifier() : '' });
		} while (take(','));
	}
	expect('FROM');
	const usedSources = new Set<string>();
	const relation = () => {
		const first = identifier();
		const qualified = take('.');
		const name = qualified ? identifier() : first;
		const schema = qualified ? first : null;
		const candidates = sources.filter((s) => s.name === name && (!schema || s.schema === schema));
		if (candidates.length !== 1) throw new Error('Ambiguous source');
		const source = tables.find((t) => t.schema === candidates[0].schema && t.name === name);
		if (!source) throw new Error('Unknown source');
		const table = addTable(model, source.schema, source.name);
		if (take('AS')) table.alias = identifier();
		else if (
			tokens[at] &&
			/^(?:"(?:[^"]|"")*"|[a-zA-Z_][a-zA-Z_0-9$]*)$/.test(tokens[at]) &&
			![
				'ON',
				'JOIN',
				'INNER',
				'LEFT',
				'RIGHT',
				'FULL',
				'CROSS',
				'WHERE',
				'HAVING',
				'ORDER',
				'LIMIT',
				'OFFSET',
				'GROUP'
			].includes(tokens[at].toUpperCase())
		)
			table.alias = identifier();
		if (model.tables.some((t) => t !== table && t.alias === table.alias))
			throw new Error('Duplicate alias');
		usedSources.add(`${source.schema}\0${source.name}`);
		return table;
	};
	const resolve = (column: Ref): ColumnRef => {
		if (column.name === '*' && !column.qualifier && model.tables.length)
			return { tableId: model.tables[0].id, column: '*' };
		const candidates = model.tables.filter(
			(t) =>
				(!column.qualifier || t.alias === column.qualifier) &&
				tables.some(
					(s) => s.schema === t.schema && s.name === t.name && s.columns.includes(column.name)
				)
		);
		if (candidates.length !== 1) throw new Error('Ambiguous column');
		return { tableId: candidates[0].id, column: column.name };
	};
	const pairs = (): [Ref, Ref][] => {
		if (take('(')) {
			const list = pairs();
			while (take('AND')) list.push(...pairs());
			expect(')');
			return list;
		}
		const left = ref();
		expect('=');
		return [[left, ref()]];
	};
	const from = (): void => {
		if (take('(')) {
			from();
			expect(')');
		} else relation();
		while (true) {
			let type: JoinType = 'INNER';
			if (take('LEFT')) type = 'LEFT';
			else if (take('RIGHT')) type = 'RIGHT';
			else if (take('FULL')) type = 'FULL';
			else take('INNER');
			if (!take('JOIN')) {
				if (type !== 'INNER') throw new Error('Expected join');
				break;
			}
			const added = relation();
			expect('ON');
			const conditions = pairs();
			while (take('AND')) conditions.push(...pairs());
			for (const [left, right] of conditions) {
				const a = resolve(left),
					b = resolve(right);
				if ((a.tableId === added.id) === (b.tableId === added.id))
					throw new Error('Unsupported join');
				const joinType =
					a.tableId === added.id
						? ({ LEFT: 'RIGHT', RIGHT: 'LEFT', INNER: 'INNER', FULL: 'FULL' } as const)[type]
						: type;
				model.joins.push({ id: newId('j'), type: joinType, from: a, to: b });
			}
		}
	};
	from();
	if (usedSources.size !== sources.length) throw new Error('Unsupported sources');
	const columns = new Set<string>();
	for (const column of selected) {
		if (column.alias !== column.alias.trim()) throw new Error('Unsupported alias');
		const resolved = resolve(column.ref);
		const key = `${resolved.tableId}\0${resolved.column}`;
		if (columns.has(key)) throw new Error('Duplicate selection');
		columns.add(key);
		model.columns.push({
			...resolved,
			aggregate: column.aggregate,
			aggregateDistinct: column.aggregateDistinct,
			alias: column.alias,
			builtinAggregate: !!column.aggregate,
			countAll: column.aggregate === 'COUNT' && resolved.column === '*'
		});
	}
	let parsingHaving = false;
	const condition = (): Predicate[] => {
		if (take('(')) {
			const items = conditions();
			expect(')');
			if (items.length > 1) {
				items[0].openGroups = (items[0].openGroups ?? 0) + 1;
				items[items.length - 1].closeGroups = (items[items.length - 1].closeGroups ?? 0) + 1;
			}
			return items;
		}
		const negated = !parsingHaving && take('NOT');
		const parsed = parsingHaving
			? expression()
			: { ref: ref(), aggregate: null, aggregateDistinct: false };
		if (parsingHaving && !parsed.aggregate) throw new Error('Unsupported HAVING expression');
		const column = parsed.ref;
		let operator: Operator;
		if (take('IS')) {
			if (negated) throw new Error('Unsupported negation');
			const not = take('NOT');
			const target = take('NULL') ? 'NULL' : take('TRUE') ? 'TRUE' : take('FALSE') ? 'FALSE' : null;
			if (!target) throw new Error('Unsupported IS expression');
			operator = `IS ${not ? 'NOT ' : ''}${target}` as Operator;
			return [
				{
					ref: column,
					operator,
					value: '',
					aggregate: parsed.aggregate,
					aggregateDistinct: parsed.aggregateDistinct
				}
			];
		}
		if (
			!parsingHaving &&
			(!tokens[at] ||
				[')', ';', 'AND', 'OR', 'GROUP', 'ORDER', 'LIMIT', 'OFFSET'].includes(
					tokens[at].toUpperCase()
				))
		) {
			return [{ ref: column, operator: negated ? 'IS FALSE' : 'IS TRUE', value: '' }];
		}
		if (negated) throw new Error('Unsupported negation');

		const next = tokens[at++];
		if (!['=', '<>', '<', '<=', '>', '>=', 'LIKE', 'ILIKE'].includes(next?.toUpperCase()))
			throw new Error('Unsupported operator');
		operator = next.toUpperCase() as Operator;
		const literal = tokens[at++];
		if (!literal) throw new Error('Expected literal');
		let value: string;
		if (/^-?\d+(?:\.\d+)?$/.test(literal)) value = literal;
		else if (/^'(?:[^']|'')*'$/.test(literal)) {
			value = literal.slice(1, -1).replaceAll("''", "'");
			if (value.includes('\\')) throw new Error('Unsupported literal');
		} else throw new Error('Unsupported literal');
		let literalCast: 'text' | undefined;
		if (take('::')) {
			if (!literal.startsWith("'") || searchPath[0] !== 'pg_catalog' || !take('TEXT'))
				throw new Error('Unsupported cast');
			literalCast = 'text';
		}
		if (
			!literalCast &&
			(value !== value.trim() || (literal.startsWith("'") && /^-?\d+(?:\.\d+)?$/.test(value)))
		)
			throw new Error('Unsupported literal');
		return [
			{
				ref: column,
				operator,
				value,
				literalCast,
				aggregate: parsed.aggregate,
				aggregateDistinct: parsed.aggregateDistinct
			}
		];
	};
	const conditions = (): Predicate[] => {
		const result = condition();
		while (tokens[at]?.toUpperCase() === 'AND' || tokens[at]?.toUpperCase() === 'OR') {
			const connector = tokens[at++].toUpperCase() as 'AND' | 'OR';
			const next = condition();
			next[0].connector = connector;
			result.push(...next);
		}
		return result;
	};
	if (take('WHERE')) {
		for (const item of conditions())
			model.filters.push({
				id: newId('f'),
				connector: item.connector ?? 'AND',
				column: resolve(item.ref),
				operator: item.operator,
				value: item.value,
				literalCast: item.literalCast,
				openGroups: item.openGroups,
				closeGroups: item.closeGroups
			});
	}
	const grouped: ColumnRef[] = [];
	if (take('GROUP')) {
		expect('BY');
		do {
			grouped.push(resolve(ref()));
		} while (take(','));
	}
	if (take('HAVING')) {
		parsingHaving = true;
		for (const item of conditions())
			model.having!.push({
				id: newId('h'),
				connector: item.connector ?? 'AND',
				column: resolve(item.ref),
				operator: item.operator,
				value: item.value,
				literalCast: item.literalCast,
				openGroups: item.openGroups,
				closeGroups: item.closeGroups,
				havingAggregate: item.aggregate!,
				havingDistinct: item.aggregateDistinct,
				havingBuiltin: true,
				havingCountAll: item.ref.name === '*'
			});
		parsingHaving = false;
	}
	const expectedGroups = model.columns.filter((c) => !c.aggregate);
	const aggregating = model.columns.some((c) => c.aggregate) || (model.having?.length ?? 0) > 0;
	const keyOf = (c: ColumnRef) => `${c.tableId}\0${c.column}`;
	if (grouped.length || (aggregating && expectedGroups.length)) {
		const actual = grouped.map(keyOf);
		const expected = expectedGroups.map(keyOf);
		if (
			!aggregating ||
			actual.length !== expected.length ||
			new Set(actual).size !== actual.length ||
			expected.some((key) => !actual.includes(key))
		)
			throw new Error('Unsupported grouping');
	}
	if (take('ORDER')) {
		expect('BY');
		do {
			const sort = expression();
			const column = sort.ref;
			const output =
				!sort.aggregate && !column.qualifier
					? selected.filter((c) => c.alias && c.alias === column.name)
					: [];
			if (output.length > 1) throw new Error('Ambiguous sort');
			const resolved = resolve(output.length ? output[0].ref : column);
			const selectedColumn = model.columns.find(
				(c) => c.tableId === resolved.tableId && c.column === resolved.column
			);
			const effectiveAggregate = output.length ? output[0].aggregate : sort.aggregate;
			const effectiveDistinct = output.length
				? output[0].aggregateDistinct
				: sort.aggregateDistinct;
			if (
				(selectedColumn?.aggregate ?? null) !== effectiveAggregate ||
				!!selectedColumn?.aggregateDistinct !== effectiveDistinct
			)
				throw new Error('Unsupported sort');
			const direction = take('DESC') ? 'DESC' : (take('ASC'), 'ASC');
			model.sorts.push({ id: newId('s'), column: resolved, direction });
		} while (take(','));
	}
	if (take('LIMIT')) {
		const value = tokens[at++];
		if (!value || !/^\d+$/.test(value)) throw new Error('Unsupported limit');
		model.limit = value;
	}
	if (take('OFFSET')) {
		const value = tokens[at++];
		if (!value || !/^\d+$/.test(value)) throw new Error('Unsupported offset');
		model.offset = value;
	}
	take(';');
	if (at !== tokens.length) throw new Error('Unsupported clause');
	return model;
}
