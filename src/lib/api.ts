import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { hostKeyPrompt } from './state/host-key.svelte.js';

export type SslMode = 'disable' | 'prefer' | 'require' | 'verify-full';

export type SshAuth =
	{ method: 'password' } | { method: 'key'; keyPath: string } | { method: 'agent' };

export interface SshConfig {
	host: string;
	port: number;
	user: string;
	auth: SshAuth;
}

export interface ConnectionConfig {
	id: string;
	name: string;
	host: string;
	port: number;
	user: string;
	database: string;
	sslMode: SslMode;
	readOnly: boolean;
	color: string | null;
	ssh: SshConfig | null;
	/** Sidebar folder; null shows the connection at the top level. */
	folder: string | null;
}

export interface Placement {
	id: string;
	folder: string | null;
}

/** `undefined` keeps the stored secret, `''` deletes it. */
export interface Secrets {
	password?: string;
	sshPassword?: string;
	sshPassphrase?: string;
}

export interface SecretStatus {
	password: boolean;
	sshPassword: boolean;
	sshPassphrase: boolean;
}

export interface ServerInfo {
	version: string;
	database: string;
}

export type RelationKind = 'table' | 'partitioned' | 'view' | 'materialized' | 'foreign' | 'other';

export interface Relation {
	name: string;
	kind: RelationKind;
	estimatedRows: number | null;
}

export interface Column {
	name: string;
	dataType: string;
	notNull: boolean;
	default: string | null;
	primaryKey: boolean;
}

export interface CompletionTable {
	schema: string;
	name: string;
	columns: string[];
}

export interface DiagramTable {
	name: string;
	kind: RelationKind;
	columns: Column[];
}

export interface ForeignKey {
	name: string;
	fromTable: string;
	fromColumns: string[];
	toTable: string;
	toColumns: string[];
}

export interface Diagram {
	tables: DiagramTable[];
	foreignKeys: ForeignKey[];
}

/** Where editable rows come from; see editing.rs. */
export interface SourceInfo {
	schema: string;
	table: string;
	/** Table column per result column; null for expressions. */
	columns: (string | null)[];
	/** Result column indexes of the primary key. */
	key: number[];
}

export interface ColumnValue {
	column: string;
	value: string | null;
}

export type Change =
	| { op: 'update'; key: ColumnValue[]; values: ColumnValue[] }
	| { op: 'delete'; key: ColumnValue[] }
	| { op: 'insert'; values: ColumnValue[] };

export interface ResultSet {
	columns: string[];
	rows: (string | null)[][];
	rowCount: number;
	truncated: boolean;
	source: SourceInfo | null;
}

export interface QueryOutput {
	results: ResultSet[];
	elapsedMs: number;
}

export interface BrowseOutput extends QueryOutput {
	sql: string;
}

export interface BrowseRequest {
	schema: string;
	table: string;
	sort: { column: string; dir: 'asc' | 'desc' } | null;
	limit: number;
	offset: number;
}

export type AppError =
	| { kind: 'message'; message: string }
	| {
			kind: 'postgres';
			message: string;
			code: string | null;
			detail: string | null;
			hint: string | null;
			position: number | null;
	  }
	| { kind: 'unknownHostKey'; host: string; port: number; fingerprint: string; key: string }
	| { kind: 'hostKeyChanged'; host: string; port: number; fingerprint: string };

const ERROR_KINDS: readonly string[] = ['message', 'postgres', 'unknownHostKey', 'hostKeyChanged'];

export function isAppError(e: unknown): e is AppError {
	return (
		typeof e === 'object' &&
		e !== null &&
		ERROR_KINDS.includes((e as { kind?: unknown }).kind as string)
	);
}

export function errorMessage(e: unknown): string {
	if (isAppError(e)) {
		switch (e.kind) {
			case 'unknownHostKey':
				return `Unknown SSH host key for ${e.host}:${e.port} (${e.fingerprint})`;
			case 'hostKeyChanged':
				return `SSH host key for ${e.host}:${e.port} has CHANGED (${e.fingerprint}). Someone may be intercepting the connection. If the server was reinstalled, remove its old key from known_hosts.`;
			default:
				return e.message;
		}
	}
	if (e instanceof Error) return e.message;
	return String(e);
}

/**
 * Every command may open an SSH tunnel. When the server's key is unknown, ask
 * the user, store the key if they trust it, and retry the command once.
 */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	try {
		return await tauriInvoke<T>(cmd, args);
	} catch (e) {
		if (!isAppError(e) || e.kind !== 'unknownHostKey') throw e;
		if (!(await hostKeyPrompt.ask(e))) throw e;
		await tauriInvoke('trust_host_key', { host: e.host, port: e.port, key: e.key });
		return await tauriInvoke<T>(cmd, args);
	}
}

export const api = {
	listConnections: () => invoke<ConnectionConfig[]>('list_connections'),
	secretStatus: (id: string) => invoke<SecretStatus>('secret_status', { id }),
	saveConnection: (config: ConnectionConfig, secrets: Secrets) =>
		invoke<ConnectionConfig>('save_connection', { config, secrets }),
	deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),
	duplicateConnection: (id: string) => invoke<ConnectionConfig>('duplicate_connection', { id }),
	arrangeConnections: (placements: Placement[]) =>
		invoke<ConnectionConfig[]>('arrange_connections', { placements }),
	testConnection: (config: ConnectionConfig, secrets: Secrets) =>
		invoke<string>('test_connection', { config, secrets }),
	trustHostKey: (host: string, port: number, key: string) =>
		invoke<void>('trust_host_key', { host, port, key }),
	connect: (id: string) => invoke<ServerInfo>('connect', { id }),
	disconnect: (id: string) => invoke<void>('disconnect', { id }),
	listDatabases: (id: string) => invoke<string[]>('list_databases', { id }),
	listSchemas: (id: string, database: string, includeSystem = false) =>
		invoke<string[]>('list_schemas', { id, database, includeSystem }),
	listRelations: (id: string, database: string, schema: string) =>
		invoke<Relation[]>('list_relations', { id, database, schema }),
	listColumns: (id: string, database: string, schema: string, table: string) =>
		invoke<Column[]>('list_columns', { id, database, schema, table }),
	schemaDiagram: (id: string, database: string, schema: string) =>
		invoke<Diagram>('schema_diagram', { id, database, schema }),
	completionTables: (id: string, database: string) =>
		invoke<CompletionTable[]>('completion_tables', { id, database }),
	runQuery: (tabId: string, id: string, database: string, sql: string, maxRows: number) =>
		invoke<QueryOutput>('run_query', { tabId, id, database, sql, maxRows }),
	browseTable: (tabId: string, id: string, database: string, request: BrowseRequest) =>
		invoke<BrowseOutput>('browse_table', { tabId, id, database, request }),
	applyChanges: (id: string, database: string, schema: string, table: string, changes: Change[]) =>
		invoke<number>('apply_changes', { id, database, schema, table, changes }),
	previewChanges: (schema: string, table: string, changes: Change[]) =>
		invoke<string[]>('preview_changes', { schema, table, changes }),
	cancelQuery: (tabId: string) => invoke<void>('cancel_query', { tabId }),
	closeTab: (tabId: string) => invoke<void>('close_tab', { tabId })
};
