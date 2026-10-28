<script lang="ts" module>
	import { type VariantProps, tv } from 'tailwind-variants';
	import { cn, type WithElementRef } from '#lib/utils.js';
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';

	export const buttonVariants = tv({
		base: "aria-invalid:border-destructive rounded-lg border border-transparent bg-clip-padding text-sm font-medium aria-invalid:ring-3 active:not-aria-[haspopup]:translate-y-px disabled:cursor-not-allowed [&_svg:not([class*='size-'])]:size-4 group/button inline-flex shrink-0 items-center justify-center whitespace-nowrap transition-[color,background-color,border-color,opacity] outline-none select-none disabled:opacity-55 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		variants: {
			variant: {
				default:
					'bg-primary text-primary-foreground border-primary hover:bg-[color-mix(in_oklab,var(--color-action),var(--color-paper-3)_14%)] active:bg-[color-mix(in_oklab,var(--color-action),var(--color-paper-3)_22%)]',
				outline:
					'border-rule-2 bg-raised text-foreground hover:bg-hover active:bg-[color-mix(in_oklab,var(--color-hover),var(--color-ink)_6%)] aria-expanded:bg-hover',
				secondary:
					'bg-hover text-foreground hover:bg-[color-mix(in_oklab,var(--color-hover),var(--color-ink)_6%)] aria-expanded:bg-hover',
				ghost:
					'text-muted-foreground hover:bg-hover hover:text-foreground active:bg-[color-mix(in_oklab,var(--color-hover),var(--color-ink)_6%)] aria-expanded:bg-hover aria-expanded:text-foreground aria-pressed:bg-hover aria-pressed:text-foreground',
				destructive:
					'border-bad-fill bg-bad-fill text-white hover:bg-[color-mix(in_oklab,var(--color-bad-fill),black_10%)] active:bg-[color-mix(in_oklab,var(--color-bad-fill),black_18%)]',
				link: 'text-foreground underline decoration-rule-2 underline-offset-4 hover:decoration-signal-text'
			},
			size: {
				default:
					'h-8 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2',
				xs: "h-6 gap-1 rounded-[min(var(--radius-md),10px)] px-2 text-xs in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
				sm: "h-7 gap-1 rounded-[min(var(--radius-md),12px)] px-2.5 text-[0.8rem] in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-4",
				lg: 'h-9 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2',
				icon: 'size-8',
				'icon-xs':
					"size-6 rounded-[min(var(--radius-md),10px)] in-data-[slot=button-group]:rounded-lg [&_svg:not([class*='size-'])]:size-4",
				'icon-sm':
					'size-7 rounded-[min(var(--radius-md),12px)] in-data-[slot=button-group]:rounded-lg',
				'icon-lg': 'size-9'
			}
		},
		defaultVariants: {
			variant: 'default',
			size: 'default'
		}
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
	export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

	export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
		WithElementRef<HTMLAnchorAttributes> & {
			variant?: ButtonVariant;
			size?: ButtonSize;
		};
</script>

<script lang="ts">
	let {
		class: className,
		variant = 'default',
		size = 'default',
		ref = $bindable(null),
		href = undefined,
		type = 'button',
		disabled,
		children,
		...restProps
	}: ButtonProps = $props();
</script>

{#if href}
	<a
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		href={disabled ? undefined : href}
		aria-disabled={disabled}
		role={disabled ? 'link' : undefined}
		tabindex={disabled ? -1 : undefined}
		{...restProps}
	>
		{@render children?.()}
	</a>
{:else}
	<button
		bind:this={ref}
		data-slot="button"
		class={cn(buttonVariants({ variant, size }), className)}
		{type}
		{disabled}
		{...restProps}
	>
		{@render children?.()}
	</button>
{/if}
