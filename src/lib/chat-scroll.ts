type ScrollMetrics = Pick<HTMLElement, 'scrollHeight' | 'scrollTop' | 'clientHeight'>;

export function isNearChatEnd(element: ScrollMetrics): boolean {
	return element.scrollHeight - element.scrollTop - element.clientHeight < 40;
}

export function followChatEnd(
	element: Pick<HTMLElement, 'scrollHeight' | 'scrollTo'>,
	following: boolean
): void {
	if (following) element.scrollTo({ top: element.scrollHeight, behavior: 'auto' });
}

/**
 * Decides on each scroll event whether the log keeps following new content.
 * Proximity alone is not enough: trackpads scroll a few pixels per event, so a reader
 * moving up stays inside the "near end" band and the next streamed token would yank
 * them back. The direction of travel is what signals intent.
 */
export function nextFollowing(
	previous: { scrollTop: number; following: boolean },
	element: ScrollMetrics
): boolean {
	if (element.scrollTop < previous.scrollTop) {
		// Shrinking content makes the browser clamp scrollTop; that is not the reader leaving.
		const pinnedToEnd = element.scrollHeight - element.scrollTop - element.clientHeight < 1;
		return pinnedToEnd && previous.following;
	}
	if (isNearChatEnd(element)) return true;
	return previous.following;
}

const upwardKeys = new Set(['ArrowUp', 'PageUp', 'Home']);

/**
 * Upward intent that may not produce a scroll event, e.g. wheel up while the log
 * does not overflow yet, or a key press that lands before streamed content moves it.
 */
export function stopsFollowing(
	event: Pick<WheelEvent, 'type' | 'deltaY'> | Pick<KeyboardEvent, 'type' | 'key'>
): boolean {
	if ('deltaY' in event) return event.deltaY < 0;
	return upwardKeys.has(event.key);
}
