<script lang="ts">
	import { onMount } from 'svelte';
	import { minimalSetup } from 'codemirror';
	import { EditorView } from '@codemirror/view';
	import { EditorState } from '@codemirror/state';
	import { bracketMatching, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { json } from '@codemirror/lang-json';
	import { tags } from '@lezer/highlight';

	interface Props {
		value: string;
		readonly?: boolean;
		id?: string;
		label?: string;
		/** Cap before the field scrolls instead of growing. */
		maxHeight?: string;
		onchange?: (value: string) => void;
		/** Fires with the text when the field loses focus. */
		onblur?: (value: string) => void;
		onkeydown?: (event: KeyboardEvent) => void;
	}

	let {
		value,
		readonly = false,
		id,
		label,
		maxHeight = '320px',
		onchange,
		onblur,
		onkeydown
	}: Props = $props();

	let host: HTMLDivElement;
	let view: EditorView | undefined;

	const highlight = HighlightStyle.define([
		{ tag: tags.propertyName, color: 'var(--syntax-property)' },
		{ tag: tags.string, color: 'var(--syntax-string)' },
		{ tag: [tags.number, tags.bool, tags.null, tags.keyword], color: 'var(--syntax-number)' },
		{
			tag: [tags.punctuation, tags.separator, tags.brace, tags.squareBracket],
			color: 'var(--muted-foreground)'
		}
	]);

	onMount(() => {
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					minimalSetup,
					bracketMatching(),
					json(),
					syntaxHighlighting(highlight),
					EditorView.lineWrapping,
					EditorState.readOnly.of(readonly),
					EditorView.editable.of(!readonly),
					EditorView.contentAttributes.of({
						...(id ? { id } : {}),
						...(label ? { 'aria-label': label } : {}),
						spellcheck: 'false'
					}),
					EditorView.updateListener.of((update) => {
						if (update.docChanged) onchange?.(update.state.doc.toString());
					}),
					EditorView.domEventHandlers({
						keydown: (event) => {
							onkeydown?.(event);
							return false;
						},
						blur: (_event, editor) => {
							onblur?.(editor.state.doc.toString());
							return false;
						}
					}),
					EditorView.theme({
						'&': { fontSize: '12.5px', backgroundColor: 'transparent', maxHeight },
						'&.cm-focused': { outline: 'none' },
						'.cm-scroller': { fontFamily: 'var(--font-outlier)', lineHeight: '1.6' },
						'.cm-content': { padding: '8px 0', caretColor: 'var(--color-accent-text)' },
						'.cm-line': { padding: '0 12px' },
						'.cm-cursor': { borderLeftColor: 'var(--color-accent-text)', borderLeftWidth: '2px' },
						'&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
							backgroundColor: 'var(--syntax-selection) !important'
						},
						// Matching brackets only show while editing, and only as a tint.
						'.cm-matchingBracket, .cm-nonmatchingBracket': { backgroundColor: 'transparent' },
						'&.cm-focused .cm-matchingBracket': { backgroundColor: 'var(--syntax-selection)' }
					})
				]
			})
		});
		return () => view?.destroy();
	});

	// Outside changes (Format JSON, switching rows) replace the text; typing does not loop back.
	$effect(() => {
		const next = value;
		if (view && next !== view.state.doc.toString()) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
		}
	});
</script>

<div
	bind:this={host}
	class="overflow-hidden rounded-lg border bg-background transition-[border-color,box-shadow] duration-(--dur-micro) {readonly
		? 'border-rule'
		: 'border-input focus-within:border-signal focus-within:ring-2 focus-within:ring-signal/25'}"
></div>
