/** One condition of the table data filter. `connector` joins it to the conditions before it. */
export interface FilterCondition {
	column: string;
	op: FilterOperator;
	value?: string;
	connector: 'AND' | 'OR';
}

export type FilterOperator =
	| 'contains'
	| 'startsWith'
	| 'endsWith'
	| 'eq'
	| 'ne'
	| 'lt'
	| 'lte'
	| 'gt'
	| 'gte'
	| 'isNull'
	| 'isNotNull';

export const FILTER_OPERATORS: { value: FilterOperator; label: string }[] = [
	{ value: 'contains', label: 'contains' },
	{ value: 'startsWith', label: 'starts with' },
	{ value: 'endsWith', label: 'ends with' },
	{ value: 'eq', label: '=' },
	{ value: 'ne', label: '<>' },
	{ value: 'lt', label: '<' },
	{ value: 'lte', label: '<=' },
	{ value: 'gt', label: '>' },
	{ value: 'gte', label: '>=' },
	{ value: 'isNull', label: 'is null' },
	{ value: 'isNotNull', label: 'is not null' }
];

export function takesValue(op: FilterOperator): boolean {
	return op !== 'isNull' && op !== 'isNotNull';
}

/** Usable as a query condition: a column, plus a value unless the operator needs none. */
export function isComplete(condition: FilterCondition): boolean {
	return !!condition.column && (!takesValue(condition.op) || !!condition.value);
}
