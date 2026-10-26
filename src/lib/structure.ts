import { invoke } from './api.js';
import { quoteIdent, valueLiteral } from './sql-quote.js';

export interface StructureColumn {
	name: string;
	dataType: string;
	nullable: boolean;
	default: string | null;
	identity: '' | 'always' | 'byDefault';
	comment: string | null;
	generated: boolean;
	/** Draft only, never loaded: how existing values convert when dataType changes. */
	using?: string | null;
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
export type ColumnInput = Omit<StructureColumn, 'generated' | 'using'> & {
	/** USING expression for an alterColumn that changes dataType; ignored otherwise. */
	using?: string | null;
};
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
/** An altered column, with its USING expression only when the type actually changes. */
export function alterColumnInput(old: StructureColumn, column: StructureColumn): ColumnInput {
	const input = columnInput(column);
	const using = column.using?.trim();
	return using && column.dataType !== old.dataType ? { ...input, using } : input;
}
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
/** Original column name → current draft name, for every renamed column. */
export function columnRenames(
	draft: TableStructure,
	columnNames: Record<string, string> | (string | null)[] = {}
): Map<string, string> {
	const renames = new Map<string, string>();
	for (const [index, col] of draft.columns.entries()) {
		const old = Array.isArray(columnNames) ? columnNames[index] : columnNames[col.name];
		if (old && col.name !== old) renames.set(old, col.name);
	}
	return renames;
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
		const renames = columnRenames(draft, columnNames);
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
				changes.push({ op: 'alterColumn', name: old.name, column: alterColumnInput(old, col) });
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

export type ItemStatus = 'same' | 'new' | 'changed';
export interface ChangeStatus {
	/** Original names of columns that get altered. */
	alteredColumns: Set<string>;
	droppedColumns: string[];
	addedConstraints: Set<string>;
	droppedConstraints: string[];
	addedIndexes: Set<string>;
	droppedIndexes: string[];
}
/** What the queued request does to each object, so the designer can mark rows like a diff. */
export function changeStatus(request: StructureRequest): ChangeStatus {
	const status: ChangeStatus = {
		alteredColumns: new Set(),
		droppedColumns: [],
		addedConstraints: new Set(),
		droppedConstraints: [],
		addedIndexes: new Set(),
		droppedIndexes: []
	};
	for (const change of request.changes)
		if (change.op === 'alterColumn') status.alteredColumns.add(change.name);
		else if (change.op === 'dropColumn') status.droppedColumns.push(change.name);
		else if (change.op === 'addConstraint') status.addedConstraints.add(change.constraint.name);
		else if (change.op === 'dropConstraint') status.droppedConstraints.push(change.name);
		else if (change.op === 'addIndex') status.addedIndexes.add(change.index.name);
		else if (change.op === 'dropIndex') status.droppedIndexes.push(change.name);
	return status;
}
/** Status of a constraint or index: new when nothing of that name existed, changed when it is replaced. */
export function namedStatus(
	name: string,
	originals: { name: string }[] | undefined,
	added: Set<string>
): ItemStatus {
	if (!originals?.some((o) => o.name === name)) return 'new';
	return added.has(name) ? 'changed' : 'same';
}
/** Constraints and indexes that are gone from the draft but still exist in the database. */
export function removedItems<T extends { name: string }>(
	originals: T[] | undefined,
	drafts: T[],
	dropped: string[]
): T[] {
	return (originals ?? []).filter(
		(o) => dropped.includes(o.name) && !drafts.some((d) => d.name === o.name)
	);
}

const PRIMARY_KEY: Omit<StructureConstraint, 'name' | 'columns'> = {
	kind: 'primaryKey',
	refSchema: null,
	refTable: null,
	refColumns: [],
	onUpdate: null,
	onDelete: null,
	expression: null,
	definition: ''
};
export function primaryKeyColumns(draft: TableStructure): string[] {
	return draft.constraints.find((c) => c.kind === 'primaryKey')?.columns ?? [];
}
/** Adds a column to the primary key (creating it if needed) or takes it out (dropping an empty key). */
export function togglePrimaryKey(draft: TableStructure, column: StructureColumn): void {
	const index = draft.constraints.findIndex((c) => c.kind === 'primaryKey');
	const key = draft.constraints[index];
	if (key?.columns.includes(column.name)) {
		key.columns = key.columns.filter((name) => name !== column.name);
		if (!key.columns.length) draft.constraints.splice(index, 1);
		return;
	}
	column.nullable = false;
	if (key) key.columns = [...key.columns, column.name];
	else
		draft.constraints.unshift({
			...PRIMARY_KEY,
			name: suggestName(draft, 'primaryKey', []),
			columns: [column.name]
		});
}

const SUFFIX = {
	primaryKey: 'pkey',
	unique: 'key',
	foreignKey: 'fkey',
	check: 'check',
	index: 'idx'
};
/** Postgres' own naming scheme (items_pkey, items_email_key, …), made unique within the draft. */
export function suggestName(
	draft: TableStructure,
	kind: keyof typeof SUFFIX,
	columns: string[],
	self?: { name: string }
): string {
	const parts = [draft.name || 'table', ...(kind === 'primaryKey' ? [] : columns), SUFFIX[kind]];
	const base = parts.join('_').slice(0, 63);
	const taken = new Set(
		[...draft.constraints, ...draft.indexes].filter((o) => o !== self).map((o) => o.name)
	);
	let name = base;
	for (let i = 1; taken.has(name); i++) name = `${base}${i}`;
	return name;
}

const INTEGER = /^(smallint|integer|int|int2|int4|int8|bigint)$/i;
export function supportsIdentity(dataType: string): boolean {
	return INTEGER.test(dataType.trim());
}
export interface DefaultPreset {
	label: string;
	value: string | null;
}
/** Defaults that make sense for the column's type; no default is always offered. */
export function defaultPresets(dataType: string): DefaultPreset[] {
	const type = dataType.trim().toLowerCase();
	const presets: DefaultPreset[] = [];
	if (type.endsWith('[]')) presets.push({ label: 'Empty array', value: "'{}'" });
	else if (INTEGER.test(type) || /^(numeric|decimal|real|double precision|float\d?)/.test(type))
		presets.push({ label: 'Zero', value: '0' });
	else if (/^(bool|boolean)$/.test(type))
		presets.push({ label: 'True', value: 'true' }, { label: 'False', value: 'false' });
	else if (/^(timestamp|timestamptz)/.test(type))
		presets.push({ label: 'Now', value: 'CURRENT_TIMESTAMP' });
	else if (type === 'date') presets.push({ label: 'Today', value: 'CURRENT_DATE' });
	else if (type === 'uuid') presets.push({ label: 'Random UUID', value: 'gen_random_uuid()' });
	else if (/^jsonb?$/.test(type))
		presets.push({ label: 'Empty object', value: "'{}'" }, { label: 'Empty array', value: "'[]'" });
	else if (/^(text|varchar|character varying|char|character|citext)/.test(type))
		presets.push({ label: 'Empty text', value: "''" });
	presets.push({ label: 'No default', value: null });
	return presets;
}

/** Puts a dropped column back, keeping its original position where possible. */
export function restoreColumn(
	original: TableStructure,
	draft: TableStructure,
	columnNames: (string | null)[],
	name: string
): void {
	const column = original.columns.find((c) => c.name === name);
	if (!column) return;
	if (draft.columns.some((c) => c.name === name))
		throw new Error(`A column named "${name}" already exists`);
	const at = Math.min(original.columns.indexOf(column), draft.columns.length);
	draft.columns.splice(at, 0, structuredClone(column));
	columnNames.splice(at, 0, name);
}
/** A dropped constraint or index, with column names following any renames made since. */
export function restored<T extends { columns: string[] }>(
	item: T,
	renames: Map<string, string>
): T {
	const copy = structuredClone(item);
	copy.columns = copy.columns.map((name) => renames.get(name) ?? name);
	return copy;
}

/**
 * Adds or removes a local foreign key column while keeping the referenced columns aligned by
 * position. A new pair references the column of the same name, else the next unused key column.
 */
export function toggleForeignKeyColumn(
	constraint: StructureConstraint,
	column: string,
	options: { name: string; primaryKey?: boolean }[]
): void {
	const at = constraint.columns.indexOf(column);
	if (at >= 0) {
		constraint.columns = constraint.columns.filter((_, i) => i !== at);
		constraint.refColumns = constraint.refColumns.filter((_, i) => i !== at);
		return;
	}
	const used = new Set(constraint.refColumns);
	const match =
		options.find((o) => o.name === column && !used.has(o.name)) ??
		options.find((o) => o.primaryKey && !used.has(o.name));
	const refColumns = constraint.refColumns.slice(0, constraint.columns.length);
	while (refColumns.length < constraint.columns.length) refColumns.push('');
	constraint.columns = [...constraint.columns, column];
	constraint.refColumns = [...refColumns, match?.name ?? ''];
}

/** A one-column check like `"price" >= 0`; numbers stay bare, anything else becomes a text literal. */
export function checkExpression(column: string, operator: string, value: string): string | null {
	const trimmed = value.trim();
	if (!column || !trimmed) return null;
	return `${quoteIdent(column)} ${operator} ${valueLiteral(trimmed)}`;
}

/** Pairs each local column with the referenced column of the same name, else an unused key column. */
export function matchReferences(
	columns: string[],
	options: { name: string; primaryKey?: boolean }[]
): string[] {
	const used: string[] = [];
	return columns.map((name) => {
		const match =
			options.find((o) => o.name === name && !used.includes(o.name)) ??
			options.find((o) => o.primaryKey && !used.includes(o.name));
		if (match) used.push(match.name);
		return match?.name ?? '';
	});
}

export const COLUMN_TYPES = [
	'text',
	'varchar(255)',
	'integer',
	'bigint',
	'smallint',
	'numeric',
	'numeric(12,2)',
	'real',
	'double precision',
	'boolean',
	'uuid',
	'date',
	'time',
	'timestamp',
	'timestamp with time zone',
	'interval',
	'jsonb',
	'json',
	'bytea',
	'inet',
	'text[]'
];
export const IDENTITY_LABELS = { '': 'None', always: 'Always', byDefault: 'By default' } as const;
export const DESIGNER_SECTIONS = ['columns', 'constraints', 'indexes'] as const;
export type DesignerSection = (typeof DESIGNER_SECTIONS)[number];
export const FK_ACTION_KEYS = ['onUpdate', 'onDelete'] as const;
export const NEW_CONSTRAINT_KINDS = ['primaryKey', 'unique', 'foreignKey', 'check'] as const;
export type NewConstraintKind = (typeof NEW_CONSTRAINT_KINDS)[number];
export const REFERENTIAL_ACTIONS = ['NO ACTION', 'RESTRICT', 'CASCADE', 'SET NULL', 'SET DEFAULT'];
export const INDEX_METHODS = ['btree', 'hash', 'gin', 'gist', 'brin', 'spgist'];

/** Items whose name still follows their columns until the user types one. */
export type AutoNamed = Pick<WeakSet<object>, 'has'>;
export type ReferenceOption = { name: string; primaryKey?: boolean };

/** Pending change count; an untouched create form has nothing to apply yet. */
export function draftChangeCount(
	draft: TableStructure,
	request: StructureRequest,
	table: string | undefined,
	reloadRequired: boolean
): number {
	if (reloadRequired) return 0;
	const touched =
		table ||
		draft.name ||
		draft.comment ||
		draft.columns.length ||
		draft.constraints.length ||
		draft.indexes.length;
	return touched ? request.changes.length : 0;
}

/** Why the designer cannot be edited right now, or '' when it can. */
export function designerReadOnlyReason(state: {
	reloadRequired: boolean;
	/** The connection's flag; anything but an explicit false counts as read-only. */
	readOnly: boolean | undefined;
	table: string | undefined;
	original: TableStructure | null;
}): string {
	if (state.reloadRequired) return 'Reload to keep editing.';
	if (state.readOnly !== false) return 'Read-only connection.';
	if (state.table && !state.original) return 'The structure could not be loaded.';
	if (state.original && state.original.kind !== 'r') return 'Only plain tables can be edited here.';
	return '';
}

const nameKind = (kind: StructureConstraint['kind']) => (kind === 'other' ? 'check' : kind);

/** Renames the draft table, following self references and auto-named constraints and indexes. */
export function renameDraftTable(
	draft: TableStructure,
	schema: string,
	name: string,
	autoNamed: AutoNamed
): void {
	const old = draft.name;
	draft.name = name;
	for (const c of draft.constraints) {
		if ((c.refSchema ?? schema) === schema && c.refTable === old) c.refTable = name;
		if (autoNamed.has(c)) c.name = suggestName(draft, nameKind(c.kind), c.columns, c);
	}
	for (const idx of draft.indexes)
		if (autoNamed.has(idx)) idx.name = suggestName(draft, 'index', idx.columns, idx);
}

/** Appends an empty nullable text column and returns it. */
export function addDraftColumn(
	draft: TableStructure,
	columnNames: (string | null)[]
): StructureColumn {
	columnNames.push(null);
	draft.columns.push({
		name: '',
		dataType: 'text',
		nullable: true,
		default: null,
		identity: '',
		comment: null,
		generated: false
	});
	return draft.columns.at(-1)!;
}

/**
 * Removes a column and takes it out of every constraint and index; keys and indexes left
 * without columns go too, checks stay because their expression is free text.
 */
export function removeDraftColumn(
	draft: TableStructure,
	columnNames: (string | null)[],
	index: number
): void {
	const name = draft.columns[index].name;
	draft.columns.splice(index, 1);
	columnNames.splice(index, 1);
	for (const c of draft.constraints) {
		const at = c.columns.indexOf(name);
		if (at < 0) continue;
		c.columns = c.columns.filter((_, n) => n !== at);
		if (c.kind === 'foreignKey') c.refColumns = c.refColumns.filter((_, n) => n !== at);
	}
	draft.constraints = draft.constraints.filter(
		(c) => c.kind === 'other' || c.kind === 'check' || c.columns.length
	);
	for (const idx of draft.indexes) idx.columns = idx.columns.filter((n) => n !== name);
	draft.indexes = draft.indexes.filter((idx) => idx.managed || idx.columns.length);
}

/** Appends an empty constraint of `kind` with a suggested name and returns it. */
export function addDraftConstraint(
	draft: TableStructure,
	schema: string,
	kind: NewConstraintKind
): StructureConstraint {
	draft.constraints.push({
		name: suggestName(draft, kind, []),
		kind,
		columns: [],
		refSchema: kind === 'foreignKey' ? schema : null,
		refTable: null,
		refColumns: [],
		onUpdate: null,
		onDelete: null,
		expression: null,
		definition: ''
	});
	return draft.constraints.at(-1)!;
}

/** Appends an empty btree index with a suggested name and returns it. */
export function addDraftIndex(draft: TableStructure): StructureIndex {
	draft.indexes.push({
		name: suggestName(draft, 'index', []),
		columns: [],
		unique: false,
		method: 'btree',
		definition: '',
		managed: false
	});
	return draft.indexes.at(-1)!;
}

const toggled = (columns: string[], column: string) =>
	columns.includes(column) ? columns.filter((n) => n !== column) : [...columns, column];

/** Toggles a column of a constraint; primary key columns become not null. */
export function toggleConstraintColumn(
	draft: TableStructure,
	constraint: StructureConstraint,
	column: string,
	options: ReferenceOption[],
	autoNamed: AutoNamed
): void {
	if (constraint.kind === 'foreignKey') toggleForeignKeyColumn(constraint, column, options);
	else constraint.columns = toggled(constraint.columns, column);
	if (constraint.kind === 'primaryKey')
		for (const col of draft.columns)
			if (constraint.columns.includes(col.name)) col.nullable = false;
	if (autoNamed.has(constraint))
		constraint.name = suggestName(draft, nameKind(constraint.kind), constraint.columns, constraint);
}

export function toggleIndexColumn(
	draft: TableStructure,
	index: StructureIndex,
	column: string,
	autoNamed: AutoNamed
): void {
	index.columns = toggled(index.columns, column);
	if (autoNamed.has(index)) index.name = suggestName(draft, 'index', index.columns, index);
}

/** True when a foreign key points at the table being created, whose columns exist only in the draft. */
export function referencesDraft(
	draft: TableStructure,
	original: TableStructure | null,
	schema: string,
	constraint: StructureConstraint
): boolean {
	return (
		(constraint.refSchema || schema) === schema && constraint.refTable === draft.name && !original
	);
}

/** Cache key of a referenced table's columns. */
export const referenceKey = (schema: string, constraint: StructureConstraint) =>
	`${constraint.refSchema || schema}.${constraint.refTable ?? ''}`;

/** Columns a foreign key may reference: the draft's own for a new self reference, else the loaded ones. */
export function referenceOptions(
	draft: TableStructure,
	original: TableStructure | null,
	schema: string,
	refColumns: Record<string, ReferenceOption[]>,
	constraint: StructureConstraint
): ReferenceOption[] {
	if (referencesDraft(draft, original, schema, constraint)) {
		const keys = primaryKeyColumns(draft);
		return draft.columns.map((col) => ({ name: col.name, primaryKey: keys.includes(col.name) }));
	}
	return refColumns[referenceKey(schema, constraint)] ?? [];
}

/**
 * What a freshly loaded draft needs fetched for its foreign keys: relation lists of the other
 * schemas it references, and one foreign key per referenced table for its column list.
 */
export function referenceLoads(
	draft: TableStructure,
	schema: string
): { schemas: string[]; foreignKeys: StructureConstraint[] } {
	const schemas = new Set<string>();
	const foreignKeys = new Map<string, StructureConstraint>();
	for (const c of draft.constraints) {
		if (c.refSchema && c.refSchema !== schema) schemas.add(c.refSchema);
		if (c.kind === 'foreignKey' && c.refTable && !foreignKeys.has(referenceKey(schema, c)))
			foreignKeys.set(referenceKey(schema, c), c);
	}
	return { schemas: [...schemas], foreignKeys: [...foreignKeys.values()] };
}

/** Tables a foreign key can point at; the draft table itself is offered within its own schema. */
export function referenceTables(
	draft: TableStructure,
	schema: string,
	relations: Record<string, { name: string }[]>,
	constraint: StructureConstraint
): string[] {
	const names = (relations[constraint.refSchema || schema] ?? []).map((r) => r.name);
	if ((constraint.refSchema || schema) === schema && draft.name && !names.includes(draft.name))
		names.unshift(draft.name);
	return names;
}
