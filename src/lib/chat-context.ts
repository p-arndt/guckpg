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

/**
 * The context the chat follows by default: the active tab's connection when it is connected,
 * otherwise the first connected one, and the tab's database when the tab belongs to it.
 */
export function activeChatContext(
	tab: { connectionId: string; database: string } | undefined,
	connections: ConnectionConfig[],
	status: Record<string, string>,
	info: Record<string, { database: string } | undefined>
): ChatContext | null {
	const connected = (id: string) => status[id] === 'connected';
	const connection =
		(tab && connected(tab.connectionId)
			? connections.find((c) => c.id === tab.connectionId)
			: undefined) ?? connections.find((c) => connected(c.id));
	if (!connection) return null;
	return {
		connectionId: connection.id,
		database:
			(tab?.connectionId === connection.id ? tab.database : undefined) ??
			info[connection.id]?.database ??
			connection.database,
		name: connection.name,
		user: connection.user,
		readOnly: connection.readOnly
	};
}

/** Identity of a context for "did the chat move elsewhere" checks; display fields don't count. */
export function contextKey(context: { connectionId: string; database: string } | null): string {
	return context ? JSON.stringify([context.connectionId, context.database]) : '';
}
