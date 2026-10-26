import { describe, expect, it } from 'vitest';
import { Resource } from './resource.svelte';

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

describe('Resource', () => {
	it('loads data and tracks loading', async () => {
		const resource = new Resource(() => Promise.resolve([1, 2]));
		const done = resource.load();
		expect(resource.loading).toBe(true);
		await expect(done).resolves.toBe(true);
		expect(resource.data).toEqual([1, 2]);
		expect(resource.loading).toBe(false);
		expect(resource.failed).toBe(false);
	});

	it('keeps the newest answer when an older request finishes last', async () => {
		const requests = [deferred<string>(), deferred<string>()];
		let n = 0;
		const resource = new Resource(() => requests[n++].promise);
		const older = resource.load();
		const newer = resource.load();
		requests[1].resolve('new');
		await expect(newer).resolves.toBe(true);
		requests[0].resolve('old');
		await expect(older).resolves.toBe(false);
		expect(resource.data).toBe('new');
		expect(resource.loading).toBe(false);
	});

	it('stays loading until the latest request finishes', async () => {
		const requests = [deferred<string>(), deferred<string>()];
		let n = 0;
		const resource = new Resource(() => requests[n++].promise);
		void resource.load();
		const newer = resource.load();
		requests[0].resolve('old');
		await Promise.resolve();
		expect(resource.loading).toBe(true);
		requests[1].resolve('new');
		await newer;
		expect(resource.loading).toBe(false);
	});

	it('records errors of the latest request only', async () => {
		const requests = [deferred<string>(), deferred<string>()];
		let n = 0;
		const resource = new Resource(() => requests[n++].promise);
		const older = resource.load();
		const newer = resource.load();
		requests[0].reject(new Error('stale'));
		await older;
		expect(resource.failed).toBe(false);
		requests[1].reject(new Error('down'));
		await expect(newer).resolves.toBe(false);
		expect((resource.error as Error).message).toBe('down');
	});

	it('clears an earlier error on reload', async () => {
		let fail = true;
		const resource = new Resource(() =>
			fail ? Promise.reject(new Error('x')) : Promise.resolve(1)
		);
		await resource.load();
		expect(resource.failed).toBe(true);
		fail = false;
		await resource.load();
		expect(resource.failed).toBe(false);
		expect(resource.data).toBe(1);
	});

	it('ignores requests in flight after a reset', async () => {
		const request = deferred<string>();
		const resource = new Resource(() => request.promise);
		const pending = resource.load();
		resource.reset();
		request.resolve('late');
		await expect(pending).resolves.toBe(false);
		expect(resource.data).toBeNull();
		expect(resource.loading).toBe(false);
	});
});
