<script lang="ts">
	import { onMount } from 'svelte';
	import { basicSetup } from 'codemirror';
	import { EditorView, keymap, Decoration, type DecorationSet } from '@codemirror/view';
	import { Compartment, EditorState, Prec, StateEffect, StateField } from '@codemirror/state';
	import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { sql, PostgreSQL, type SQLNamespace } from '@codemirror/lang-sql';
	import { tags } from '@lezer/highlight';
	import type { CompletionTable } from '#lib/api.js';

	interface Props {
		value: string;
		completion?: CompletionTable[];
		/** Run the selection or the statement under the cursor. */
		onRun?: () => void;
		onRunAll?: () => void;
	}

	let { value = $bindable(), completion = [], onRun, onRunAll }: Props = $props();

	let host: HTMLDivElement;
	let view: EditorView | undefined;
	const language = new Compartment();

	function namespace(tables: CompletionTable[]): SQLNamespace {
		const ns: Record<string, Record<string, string[]>> = {};
		for (const t of tables) (ns[t.schema] ??= {})[t.name] = t.columns;
		return ns;
	}

	function languageFor(tables: CompletionTable[]) {
		return sql({
			dialect: PostgreSQL,
			schema: namespace(tables),
			defaultSchema: 'public',
			upperCaseKeywords: true
		});
	}

	// Colors come from CSS variables so light and dark mode both work.
	const highlight = HighlightStyle.define([
		{ tag: tags.keyword, color: 'var(--syntax-keyword)', fontWeight: '500' },
		{ tag: [tags.string, tags.special(tags.string)], color: 'var(--syntax-string)' },
		{ tag: [tags.number, tags.bool, tags.null], color: 'var(--syntax-number)' },
		{
			tag: [tags.lineComment, tags.blockComment],
			color: 'var(--color-muted)',
			fontStyle: 'italic'
		},
		{ tag: [tags.typeName, tags.standard(tags.name)], color: 'var(--syntax-type)' },
		{ tag: [tags.operator, tags.punctuation], color: 'var(--muted-foreground)' }
	]);

	const theme = EditorView.theme({
		'&': { height: '100%', fontSize: '13px', backgroundColor: 'transparent' },
		'.cm-scroller': { fontFamily: 'var(--font-outlier)', lineHeight: '1.55' },
		'.cm-content': { caretColor: 'var(--color-accent-text)' },
		// basicSetup draws its own cursor, black by default: invisible in dark mode.
		'.cm-cursor, .cm-dropCursor': {
			borderLeftColor: 'var(--color-accent-text)',
			borderLeftWidth: '2px'
		},
		'.cm-gutters': {
			backgroundColor: 'transparent',
			color: 'var(--muted-foreground)',
			border: 'none'
		},
		'.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--syntax-active-line)' },
		'&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
			backgroundColor: 'var(--syntax-selection) !important'
		},
		'.cm-tooltip': {
			backgroundColor: 'var(--popover)',
			color: 'var(--popover-foreground)',
			border: '1px solid var(--color-rule-2)',
			borderRadius: '6px',
			fontFamily: 'var(--font-outlier)'
		},
		'.cm-tooltip-autocomplete ul li[aria-selected]': {
			backgroundColor: 'var(--color-accent-soft)',
			color: 'var(--accent-foreground)'
		},
		'.cm-error-mark': {
			textDecoration: 'underline wavy var(--destructive)',
			backgroundColor: 'color-mix(in oklch, var(--destructive) 15%, transparent)'
		}
	});

	const setError = StateEffect.define<{ from: number; to: number } | null>();
	const errorField = StateField.define<DecorationSet>({
		create: () => Decoration.none,
		update(marks, tr) {
			for (const e of tr.effects) {
				if (e.is(setError)) {
					return e.value
						? Decoration.set([
								Decoration.mark({ class: 'cm-error-mark' }).range(e.value.from, e.value.to)
							])
						: Decoration.none;
				}
			}
			return tr.docChanged ? Decoration.none : marks.map(tr.changes);
		},
		provide: (f) => EditorView.decorations.from(f)
	});

	/** Selected text, or undefined when the selection is empty. */
	export function selection(): { from: number; text: string } | undefined {
		if (!view) return undefined;
		const range = view.state.selection.main;
		if (range.empty) return undefined;
		return { from: range.from, text: view.state.sliceDoc(range.from, range.to) };
	}

	export function cursor(): number {
		return view?.state.selection.main.head ?? 0;
	}

	/** Underline the token at `pos` (0-based offset) and move the cursor there. */
	export function markError(pos: number | null) {
		if (!view) return;
		if (pos === null) {
			view.dispatch({ effects: setError.of(null) });
			return;
		}
		const doc = view.state.doc;
		const from = Math.min(pos, doc.length);
		const word = /^[\w."]+/.exec(doc.sliceString(from, from + 64))?.[0].length ?? 1;
		const to = Math.min(from + Math.max(word, 1), doc.length);
		view.dispatch({
			effects: setError.of(from < to ? { from, to } : null),
			selection: { anchor: from },
			scrollIntoView: true
		});
	}

	export function focus() {
		view?.focus();
	}

	onMount(() => {
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					// Before basicSetup so Mod-Enter is not taken by insertBlankLine.
					Prec.highest(
						keymap.of([
							{ key: 'Mod-Enter', run: () => (onRun?.(), true) },
							{ key: 'Shift-Mod-Enter', run: () => (onRunAll?.(), true) }
						])
					),
					basicSetup,
					language.of(languageFor(completion)),
					syntaxHighlighting(highlight),
					theme,
					errorField,
					EditorView.updateListener.of((u) => {
						if (u.docChanged) value = u.state.doc.toString();
					})
				]
			})
		});
		view.focus();
		return () => view?.destroy();
	});

	$effect(() => {
		const tables = completion;
		view?.dispatch({ effects: language.reconfigure(languageFor(tables)) });
	});

	// Outside changes (e.g. "New query" with SQL) replace the document.
	$effect(() => {
		const next = value;
		if (view && next !== view.state.doc.toString()) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
		}
	});
</script>

<div bind:this={host} class="h-full min-h-0 overflow-hidden"></div>
