import type { ConnectionConfig } from './api.js';

export type ChatContext = {
	connectionId: string;
	database: string;
	name: string;
	user: string;
	readOnly: boolean;
};

export function chatContext(
	base: ChatContext | null,
	selected: { connectionId: string; database: string } | null,
	connections: ConnectionConfig[],
	status: Record<string, string>
): ChatContext | null {
	if (!base || !selected || status[selected.connectionId] !== 'connected') return base;
	const connection = connections.find((c) => c.id === selected.connectionId);
	if (!connection) return base;
	return {
		connectionId: connection.id,
		database: selected.database,
		name: connection.name,
		user: connection.user,
		readOnly: connection.readOnly
	};
}
