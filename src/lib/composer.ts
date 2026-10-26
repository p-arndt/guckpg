interface ComposerKey {
	key: string;
	shiftKey: boolean;
	isComposing: boolean;
}

/**
 * Enter sends, Shift+Enter breaks the line, as in chat apps. Enter that confirms an IME
 * composition (Japanese, Chinese input) must not send half-typed text.
 */
export function isSendKey(e: ComposerKey): boolean {
	return e.key === 'Enter' && !e.shiftKey && !e.isComposing;
}
