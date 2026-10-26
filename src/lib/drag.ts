export interface DragHandlers {
	/** Every pointer move until the drag ends. */
	move?: (event: PointerEvent) => void;
	/** Runs once per animation frame while dragging, e.g. for edge auto-scroll. */
	frame?: () => void;
	/** Runs exactly once, whether the pointer was released, cancelled or `stop` was called. */
	end?: () => void;
}

export interface DragOptions {
	/** Where move/up/cancel are listened for; the window by default. */
	target?: EventTarget;
}

/**
 * Follows one pointer from `start` until it is released or cancelled. The pressed element
 * captures the pointer so the drag also ends when the release happens outside the window.
 * Returns `stop` so an unmounting component can end a drag still in progress.
 */
export function dragPointer(
	start: PointerEvent,
	handlers: DragHandlers,
	{ target = window }: DragOptions = {}
): () => void {
	const element = start.currentTarget as Partial<Element> | null;
	if (typeof element?.setPointerCapture === 'function') {
		try {
			element.setPointerCapture(start.pointerId);
		} catch {
			// The pointer may already be gone (e.g. a synthetic event); window listeners still work.
		}
	}
	let frameId = 0;
	let active = true;
	const tick = () => {
		if (!active) return;
		handlers.frame?.();
		frameId = requestAnimationFrame(tick);
	};
	const move = (event: Event) => {
		if ((event as PointerEvent).pointerId === start.pointerId)
			handlers.move?.(event as PointerEvent);
	};
	const finish = (event: Event) => {
		if ((event as PointerEvent).pointerId === start.pointerId) stop();
	};
	function stop() {
		if (!active) return;
		active = false;
		if (frameId) cancelAnimationFrame(frameId);
		target.removeEventListener('pointermove', move);
		target.removeEventListener('pointerup', finish);
		target.removeEventListener('pointercancel', finish);
		handlers.end?.();
	}
	target.addEventListener('pointermove', move);
	target.addEventListener('pointerup', finish);
	target.addEventListener('pointercancel', finish);
	if (handlers.frame) frameId = requestAnimationFrame(tick);
	return stop;
}
