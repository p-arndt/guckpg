import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { hostKeyPrompt } from './state/host-key.svelte.js';

export type SslMode = 'disable' | 'prefer' | 'require' | 'verify-ca' | 'verify-full';

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
	/** CA file for verify-ca/verify-full; unset uses the system roots. */
	sslRootCert?: string | null;
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

export interface DatabaseRole {
	name: string;
	canLogin: boolean;
	superuser: boolean;
	createDb: boolean;
	createRole: boolean;
}

export interface RoleMembership {
	role: string;
	admin: boolean;
}

export type PrivilegeKind =
	'database' | 'schema' | 'table' | 'partitioned' | 'view' | 'materialized' | 'foreign' | 'other';

export interface PrivilegeGrant {
	privilege: string;
	grantor: string;
}

export interface ObjectPrivileges {
	schema: string | null;
	name: string;
	kind: PrivilegeKind;
	owner: boolean;
	/** Granted to the role directly. */
	granted: string[];
	/** Who granted each entry of `granted`; a REVOKE leaves other grantors' grants in place. */
	grants: PrivilegeGrant[];
	/** What the role can do, including ownership, PUBLIC and memberships. */
	effective: string[];
}

export interface RoleDetails {
	name: string;
	canLogin: boolean;
	superuser: boolean;
	createDb: boolean;
	createRole: boolean;
	inherit: boolean;
	replication: boolean;
	bypassRls: boolean;
	/** -1 means unlimited. */
	connectionLimit: number;
	/** null means the password never expires. */
	validUntil: string | null;
	comment: string | null;
	memberOf: RoleMembership[];
	members: RoleMembership[];
	databases: ObjectPrivileges[];
	otherRoles: string[];
}

export interface RolePrivileges {
	schemas: ObjectPrivileges[];
	relations: ObjectPrivileges[];
}

export type PrivilegeTarget =
	| { kind: 'database'; name: string }
	| { kind: 'schema'; name: string }
	| { kind: 'table'; schema: string; name: string };

export interface RoleAttributeChanges {
	canLogin?: boolean;
	superuser?: boolean;
	createDb?: boolean;
	createRole?: boolean;
	inherit?: boolean;
	replication?: boolean;
	bypassRls?: boolean;
	connectionLimit?: number;
	/** null removes the expiry. */
	validUntil?: string | null;
}

export interface RoleChanges {
	name: string;
	rename?: string;
	attributes?: RoleAttributeChanges;
	password?: string;
	removePassword?: boolean;
	/** null removes the comment. */
	comment?: string | null;
	grantRoles?: string[];
	revokeRoles?: string[];
	privileges?: { target: PrivilegeTarget; privilege: string; grant: boolean }[];
}

export type RelationKind = 'table' | 'partitioned' | 'view' | 'materialized' | 'foreign' | 'other';

export interface Relation {
	name: string;
	kind: RelationKind;
	estimatedRows: number | null;
}

export interface SchemaObject {
	oid: number;
	name: string;
	kind: 'function' | 'procedure' | 'trigger' | 'sequence';
	identity: string;
}

export interface Column {
	name: string;
	dataType: string;
	notNull: boolean;
	default: string | null;
	primaryKey: boolean;
}

export interface ViewDetails {
	query: string;
	options: string;
	sources: { schema: string; name: string }[];
	searchPath: string[];
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
	filter?: { column: string; value: string } | null;
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

/** Any thrown value as an AppError, so views can show it the same way. */
export function toAppError(e: unknown): AppError {
	return isAppError(e) ? e : { kind: 'message', message: errorMessage(e) };
}

/**
 * Every command may open an SSH tunnel. When the server's key is unknown, ask
 * the user, store the key if they trust it, and retry the command once.
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
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
	listRoles: (id: string) => invoke<DatabaseRole[]>('list_roles', { id }),
	createRole: (
		id: string,
		name: string,
		password: string | null,
		login: boolean,
		createDb: boolean,
		createRole: boolean
	) => invoke<void>('create_role', { id, name, password, login, createDb, createRole }),
	roleDetails: (id: string, name: string) => invoke<RoleDetails>('role_details', { id, name }),
	rolePrivileges: (id: string, database: string, name: string) =>
		invoke<RolePrivileges>('role_privileges', { id, database, name }),
	previewRoleChanges: (changes: RoleChanges) =>
		invoke<string[]>('preview_role_changes', { changes }),
	applyRoleChanges: (id: string, database: string, changes: RoleChanges) =>
		invoke<void>('apply_role_changes', { id, database, changes }),
	dropRole: (id: string, name: string) => invoke<void>('drop_role', { id, name }),
	listSchemas: (id: string, database: string, includeSystem = false) =>
		invoke<string[]>('list_schemas', { id, database, includeSystem }),
	listRelations: (id: string, database: string, schema: string) =>
		invoke<Relation[]>('list_relations', { id, database, schema }),
	listObjects: (id: string, database: string, schema: string) =>
		invoke<SchemaObject[]>('list_objects', { id, database, schema }),
	objectDefinition: (
		id: string,
		database: string,
		schema: string,
		kind: SchemaObject['kind'],
		oid: number
	) => invoke<string>('object_definition', { id, database, schema, kind, oid }),
	relationDefinition: (id: string, database: string, schema: string, name: string) =>
		invoke<string>('relation_definition', { id, database, schema, name }),
	viewDetails: (id: string, database: string, schema: string, name: string) =>
		invoke<ViewDetails>('view_details', { id, database, schema, name }),
	refreshMaterializedView: (id: string, database: string, schema: string, name: string) =>
		invoke<void>('refresh_materialized_view', { id, database, schema, name }),
	listColumns: (id: string, database: string, schema: string, table: string) =>
		invoke<Column[]>('list_columns', { id, database, schema, table }),
	schemaDiagram: (id: string, database: string, schema: string) =>
		invoke<Diagram>('schema_diagram', { id, database, schema }),
	completionTables: (id: string, database: string) =>
		invoke<CompletionTable[]>('completion_tables', { id, database }),
	explainQuery: (tabId: string, id: string, database: string, sql: string) =>
		invoke<QueryOutput>('explain_query', { tabId, id, database, sql }),
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
