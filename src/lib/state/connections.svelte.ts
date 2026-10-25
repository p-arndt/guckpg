import {
	api,
	type ConnectionConfig,
	type Placement,
	type Secrets,
	type ServerInfo
} from '#lib/api.js';

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'error';

class Connections {
	list = $state<ConnectionConfig[]>([]);
	status = $state<Record<string, ConnectionStatus>>({});
	info = $state<Record<string, ServerInfo>>({});
	/** Bumped to make open tree nodes reload their children. */
	revision = $state<Record<string, number>>({});

	get(id: string): ConnectionConfig | undefined {
		return this.list.find((c) => c.id === id);
	}

	async load() {
		this.list = await api.listConnections();
	}

	async connect(id: string): Promise<ServerInfo> {
		this.status[id] = 'connecting';
		try {
			const info = await api.connect(id);
			this.info[id] = info;
			this.status[id] = 'connected';
			return info;
		} catch (e) {
			this.status[id] = 'error';
			throw e;
		}
	}

	async disconnect(id: string) {
		await api.disconnect(id);
		this.status[id] = 'idle';
		delete this.info[id];
	}

	async save(config: ConnectionConfig, secrets: Secrets): Promise<ConnectionConfig> {
		const saved = await api.saveConnection(config, secrets);
		// Saving drops live sessions in the backend.
		this.status[saved.id] = 'idle';
		await this.load();
		return saved;
	}

	async remove(id: string) {
		await api.deleteConnection(id);
		delete this.status[id];
		delete this.info[id];
		await this.load();
	}

	async duplicate(id: string): Promise<ConnectionConfig> {
		const copy = await api.duplicateConnection(id);
		await this.load();
		return copy;
	}

	/** Apply a new sidebar order and folders; shows it before the backend answers. */
	async arrange(placements: Placement[]) {
		const byId = new Map(this.list.map((c) => [c.id, c]));
		this.list = placements.flatMap((p) => {
			const c = byId.get(p.id);
			return c ? [{ ...c, folder: p.folder }] : [];
		});
		try {
			this.list = await api.arrangeConnections(placements);
		} catch (e) {
			await this.load();
			throw e;
		}
	}

	refresh(id: string) {
		this.revision[id] = (this.revision[id] ?? 0) + 1;
	}
}

export const connections = new Connections();

export function newConnection(): ConnectionConfig {
	return {
		id: '',
		name: '',
		host: 'localhost',
		port: 5432,
		user: 'postgres',
		database: 'postgres',
		sslMode: 'prefer',
		readOnly: false,
		color: null,
		ssh: null,
		folder: null
	};
}
