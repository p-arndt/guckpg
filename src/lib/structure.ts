import { invoke } from './api.js';

export interface StructureColumn {
	name: string;
	dataType: string;
	nullable: boolean;
	default: string | null;
	identity: '' | 'always' | 'byDefault';
	comment: string | null;
	generated: boolean;
}
export interface StructureConstraint {
	name: string;
	kind: 'primaryKey' | 'unique' | 'foreignKey' | 'check' | 'other';
	columns: string[];
	refSchema: string | null;
	refTable: string | null;
	refColumns: string[];
	onUpdate: string | null;
	onDelete: string | null;
	expression: string | null;
	definition: string;
}
export interface StructureIndex {
	name: string;
	columns: string[];
	unique: boolean;
	method: string;
	definition: string;
	managed: boolean;
}
export interface TableStructure {
	schema: string;
	name: string;
	comment: string | null;
	kind: string;
	columns: StructureColumn[];
	constraints: StructureConstraint[];
	indexes: StructureIndex[];
}
export type ColumnInput = Omit<StructureColumn, 'generated'>;
export type ConstraintInput = Omit<StructureConstraint, 'definition' | 'kind'> & {
	kind: Exclude<StructureConstraint['kind'], 'other'>;
};
export type IndexInput = Pick<StructureIndex, 'name' | 'columns' | 'unique' | 'method'>;
export type StructureChange =
	| { op: 'createTable'; name: string; columns: ColumnInput[]; comment: string | null }
	| { op: 'renameTable'; name: string }
	| { op: 'setTableComment'; comment: string | null }
	| { op: 'addColumn'; column: ColumnInput }
	| { op: 'alterColumn'; name: string; column: ColumnInput }
	| { op: 'dropColumn'; name: string }
	| { op: 'addConstraint'; constraint: ConstraintInput }
	| { op: 'dropConstraint'; name: string }
	| { op: 'addIndex'; index: IndexInput }
	| { op: 'dropIndex'; name: string };
export interface StructureRequest {
	schema: string;
	table: string | null;
	expected: TableStructure | null;
	changes: StructureChange[];
}
export const structureApi = {
	load: (id: string, database: string, schema: string, table: string) =>
		invoke<TableStructure>('table_structure', { id, database, schema, table }),
	preview: (request: StructureRequest) => invoke<string[]>('preview_structure', { request }),
	apply: (id: string, database: string, request: StructureRequest) =>
		invoke<string>('apply_structure', { id, database, request })
};
export const columnInput = (column: StructureColumn): ColumnInput => ({
	name: column.name,
	dataType: column.dataType,
	nullable: column.nullable,
	default: column.default,
	identity: column.identity,
	comment: column.comment
});
export const constraintInput = (constraint: StructureConstraint): ConstraintInput => ({
	name: constraint.name,
	kind: constraint.kind as ConstraintInput['kind'],
	columns: constraint.columns,
	refSchema: constraint.refSchema,
	refTable: constraint.refTable,
	refColumns: constraint.refColumns,
	onUpdate: constraint.onUpdate,
	onDelete: constraint.onDelete,
	expression: constraint.expression
});
export const indexInput = ({ name, columns, unique, method }: StructureIndex): IndexInput => ({
	name,
	columns,
	unique,
	method
});
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
export function renameDraftColumn(
	draft: TableStructure,
	column: StructureColumn,
	name: string
): void {
	if (!name.trim()) throw new Error('Column name is required');
	if (draft.columns.some((other) => other !== column && other.name === name))
		throw new Error(`A column named "${name}" already exists`);
	const old = column.name;
	column.name = name;
	renameReferences(draft, old, name);
}
export function renameReferences(draft: TableStructure, oldName: string, newName: string): void {
	const rename = (names: string[]) => names.map((name) => (name === oldName ? newName : name));
	for (const constraint of draft.constraints) {
		constraint.columns = rename(constraint.columns);
		if (
			(constraint.refSchema ?? draft.schema) === draft.schema &&
			constraint.refTable === draft.name
		)
			constraint.refColumns = rename(constraint.refColumns);
	}
	for (const index of draft.indexes) index.columns = rename(index.columns);
}
export function buildRequest(
	schema: string,
	original: TableStructure | null,
	draft: TableStructure,
	columnNames: Record<string, string> | (string | null)[] = {}
): StructureRequest {
	const changes: StructureChange[] = [];
	const normalized = original ? (JSON.parse(JSON.stringify(original)) as TableStructure) : null;
	if (normalized) {
		const renames = new Map<string, string>();
		for (const [index, col] of draft.columns.entries()) {
			const old = Array.isArray(columnNames) ? columnNames[index] : columnNames[col.name];
			if (old && col.name !== old) renames.set(old, col.name);
		}
		const map = (names: string[]) => names.map((name) => renames.get(name) ?? name);
		for (const constraint of normalized.constraints) {
			constraint.columns = map(constraint.columns);
			if ((constraint.refSchema ?? schema) === schema && constraint.refTable === original?.name)
				constraint.refColumns = map(constraint.refColumns);
			if (
				(constraint.refSchema ?? schema) === schema &&
				constraint.refTable === original?.name &&
				draft.name !== original?.name
			)
				constraint.refTable = draft.name;
		}
		for (const index of normalized.indexes) index.columns = map(index.columns);
	}

	if (!original) {
		changes.push({
			op: 'createTable',
			name: draft.name,
			columns: draft.columns.map(columnInput),
			comment: draft.comment
		});
	} else {
		if (draft.name !== original.name) changes.push({ op: 'renameTable', name: draft.name });
		if (draft.comment !== original.comment)
			changes.push({ op: 'setTableComment', comment: draft.comment });
		for (const old of normalized!.constraints)
			if (!draft.constraints.some((c) => c.name === old.name && equal(c, old)))
				changes.push({ op: 'dropConstraint', name: old.name });
		for (const old of normalized!.indexes)
			if (!old.managed && !draft.indexes.some((i) => i.name === old.name && equal(i, old)))
				changes.push({ op: 'dropIndex', name: old.name });
		for (const old of original.columns)
			if (
				!draft.columns.some(
					(c, i) =>
						(Array.isArray(columnNames) ? columnNames[i] : (columnNames[c.name] ?? c.name)) ===
						old.name
				)
			)
				changes.push({ op: 'dropColumn', name: old.name });
		for (const [index, col] of draft.columns.entries()) {
			const origin = Array.isArray(columnNames)
				? columnNames[index]
				: (columnNames[col.name] ?? col.name);
			const old = original.columns.find((c) => c.name === origin);
			if (!old) changes.push({ op: 'addColumn', column: columnInput(col) });
			else if (!equal(columnInput(old), columnInput(col)))
				changes.push({ op: 'alterColumn', name: old.name, column: columnInput(col) });
		}
	}
	for (const c of draft.constraints)
		if (c.kind !== 'other' && !normalized?.constraints.some((old) => equal(c, old)))
			changes.push({ op: 'addConstraint', constraint: constraintInput(c) });
	for (const i of draft.indexes)
		if (!i.managed && !normalized?.indexes.some((old) => equal(i, old)))
			changes.push({ op: 'addIndex', index: indexInput(i) });
	return { schema, table: original?.name ?? null, expected: original, changes };
}
export function destructive(changes: StructureChange[]): boolean {
	return changes.some(
		(c) =>
			c.op === 'dropColumn' ||
			c.op === 'dropConstraint' ||
			c.op === 'dropIndex' ||
			c.op === 'alterColumn'
	);
}
