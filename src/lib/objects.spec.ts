import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { api, type SchemaObject } from './api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
	mockedInvoke.mockReset();
});

describe('database object commands', () => {
	it('loads schema objects with overload identities', async () => {
		const objects: SchemaObject[] = [
			{ oid: 42, name: 'calculate', kind: 'function', identity: 'calculate(integer)' },
			{ oid: 43, name: 'calculate', kind: 'function', identity: 'calculate(text)' }
		];
		mockedInvoke.mockResolvedValue(objects);
		await expect(api.listObjects('connection', 'database', 'schema')).resolves.toEqual(objects);
		expect(mockedInvoke).toHaveBeenCalledWith('list_objects', {
			id: 'connection',
			database: 'database',
			schema: 'schema'
		});
	});

	it('requests definitions by object kind and OID', async () => {
		mockedInvoke.mockResolvedValue('CREATE FUNCTION calculate(integer)');
		await expect(
			api.objectDefinition('connection', 'database', 'schema', 'function', 42)
		).resolves.toBe('CREATE FUNCTION calculate(integer)');
		expect(mockedInvoke).toHaveBeenCalledWith('object_definition', {
			id: 'connection',
			database: 'database',
			schema: 'schema',
			kind: 'function',
			oid: 42
		});
	});

	it('requests relation definitions by qualified name', async () => {
		mockedInvoke.mockResolvedValue('CREATE VIEW "schema"."view" AS SELECT 1;');
		await expect(api.relationDefinition('connection', 'database', 'schema', 'view')).resolves.toBe(
			'CREATE VIEW "schema"."view" AS SELECT 1;'
		);
		expect(mockedInvoke).toHaveBeenCalledWith('relation_definition', {
			id: 'connection',
			database: 'database',
			schema: 'schema',
			name: 'view'
		});
	});

	it('forwards refresh failures to the caller', async () => {
		const error = { kind: 'message', message: 'This connection is read-only' };
		mockedInvoke.mockRejectedValue(error);
		await expect(
			api.refreshMaterializedView('connection', 'database', 'schema', 'cached')
		).rejects.toEqual(error);
		expect(mockedInvoke).toHaveBeenCalledWith('refresh_materialized_view', {
			id: 'connection',
			database: 'database',
			schema: 'schema',
			name: 'cached'
		});
	});
});
