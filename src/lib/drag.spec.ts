import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dragPointer } from './drag.js';

function pointer(type: string, pointerId = 1, clientY = 0): PointerEvent {
	return Object.assign(new Event(type), { pointerId, clientY }) as unknown as PointerEvent;
}

describe('dragPointer', () => {
	let frames: (() => void)[];
	beforeEach(() => {
		frames = [];
		vi.stubGlobal('requestAnimationFrame', (callback: () => void) => frames.push(callback));
		vi.stubGlobal('cancelAnimationFrame', (id: number) => {
			frames[id - 1] = () => {};
		});
	});
	afterEach(() => vi.unstubAllGlobals());

	it('follows moves of the same pointer and ends on release', () => {
		const target = new EventTarget();
		const moves: number[] = [];
		const end = vi.fn();
		dragPointer(pointer('pointerdown'), { move: (e) => moves.push(e.clientY), end }, { target });
		target.dispatchEvent(pointer('pointermove', 1, 10));
		target.dispatchEvent(pointer('pointermove', 2, 99));
		target.dispatchEvent(pointer('pointerup'));
		target.dispatchEvent(pointer('pointermove', 1, 20));
		expect(moves).toEqual([10]);
		expect(end).toHaveBeenCalledTimes(1);
	});

	it('ends on pointercancel', () => {
		const target = new EventTarget();
		const end = vi.fn();
		dragPointer(pointer('pointerdown'), { end }, { target });
		target.dispatchEvent(pointer('pointercancel'));
		target.dispatchEvent(pointer('pointerup'));
		expect(end).toHaveBeenCalledTimes(1);
	});

	it('stops the frame loop when stopped from outside', () => {
		const target = new EventTarget();
		const frame = vi.fn();
		const end = vi.fn();
		const stop = dragPointer(pointer('pointerdown'), { frame, end }, { target });
		frames.at(-1)!();
		frames.at(-1)!();
		expect(frame).toHaveBeenCalledTimes(2);
		stop();
		stop();
		frames.at(-1)!();
		expect(frame).toHaveBeenCalledTimes(2);
		expect(end).toHaveBeenCalledTimes(1);
	});

	it('captures the pointer on the pressed element', () => {
		const setPointerCapture = vi.fn();
		const start = pointer('pointerdown', 7);
		// Event.currentTarget is a prototype getter, so shadow it on the instance.
		Object.defineProperty(start, 'currentTarget', { value: { setPointerCapture } });
		dragPointer(start, {}, { target: new EventTarget() });
		expect(setPointerCapture).toHaveBeenCalledWith(7);
	});
});
