<script lang="ts">
	import type { Art } from '#lib/appearance.js';
	import {
		ART_HEIGHT,
		ART_WIDTH,
		clouds,
		contourLines,
		ribbons,
		ridges,
		sparks,
		stars,
		waves
	} from '#lib/backdrop-art.js';

	let { variant, class: klass = '' }: { variant: Art | 'custom'; class?: string } = $props();

	// Several instances are on screen (backdrop + settings previews); gradient ids must not clash.
	const uid = $props.id();
	const id = (name: string) => `${uid}-${name}`;

	const lines = contourLines();
	const ridgePaths = ridges();
	const ribbonPaths = ribbons();
	const starList = stars();
	const wavePaths = waves();
	const cloudList = clouds();
	const sparkList = sparks();

	const RIDGE_FILLS = ['#4a2c52', '#372343', '#271a33', '#1a1324', '#110d17'];
	const RIBBON_FILLS = ['#5eead4', '#b8f03c', '#a78bfa', '#38bdf8'];
	const WAVE_FILLS = ['#123a4a', '#0f3140', '#0c2835', '#0a202b', '#081923', '#06131b'];
	const CLOUD_FILLS = ['#c026d3', '#7c3aed', '#db2777', '#4f46e5', '#a855f7', '#ec4899', '#6366f1'];
</script>

<div class="overflow-hidden {klass}" aria-hidden="true">
	{#if variant === 'custom'}
		<div class="custom-art h-full w-full"></div>
	{:else}
		<svg
			class="h-full w-full"
			viewBox="0 0 {ART_WIDTH} {ART_HEIGHT}"
			preserveAspectRatio="xMinYMin slice"
			fill="none"
		>
			{#if variant === 'contours'}
				<!-- Lime contour lines over a deep moss-to-teal dusk. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="1" y2="1">
						<stop offset="0" stop-color="#16301f" />
						<stop offset="0.55" stop-color="#112622" />
						<stop offset="1" stop-color="#0c1714" />
					</linearGradient>
					<radialGradient id={id('sun')} cx="0.78" cy="0.05" r="0.55">
						<stop offset="0" stop-color="#b8f03c" stop-opacity="0.42" />
						<stop offset="0.45" stop-color="#7fcf4a" stop-opacity="0.14" />
						<stop offset="1" stop-color="#7fcf4a" stop-opacity="0" />
					</radialGradient>
					<radialGradient id={id('tide')} cx="0.12" cy="0.9" r="0.5">
						<stop offset="0" stop-color="#3fb8a4" stop-opacity="0.22" />
						<stop offset="1" stop-color="#3fb8a4" stop-opacity="0" />
					</radialGradient>
					<linearGradient id={id('line')} x1="0" y1="0" x2="1" y2="0">
						<stop offset="0" stop-color="#8fe0c8" />
						<stop offset="0.7" stop-color="#d4f78a" />
						<stop offset="1" stop-color="#eaffc0" />
					</linearGradient>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<rect width="100%" height="100%" fill="url(#{id('tide')})" />
				<rect width="100%" height="100%" fill="url(#{id('sun')})" />
				<g stroke="url(#{id('line')})" stroke-width="0.7" stroke-linecap="round">
					{#each lines as line, i (i)}
						<path d={line.d} stroke-opacity={line.opacity} />
					{/each}
				</g>
			{:else if variant === 'aurora'}
				<!-- Blurred bands of teal, lime and violet over a night sky with a few stars. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="0" y2="1">
						<stop offset="0" stop-color="#0b1628" />
						<stop offset="0.5" stop-color="#0c1320" />
						<stop offset="1" stop-color="#0d1117" />
					</linearGradient>
					<radialGradient id={id('haze')} cx="0.3" cy="0.45" r="0.6">
						<stop offset="0" stop-color="#2dd4bf" stop-opacity="0.16" />
						<stop offset="1" stop-color="#2dd4bf" stop-opacity="0" />
					</radialGradient>
					<filter id={id('blur')} x="-10%" y="-50%" width="120%" height="200%">
						<feGaussianBlur stdDeviation="11" />
					</filter>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<rect width="100%" height="100%" fill="url(#{id('haze')})" />
				{#each starList as s, i (i)}
					<circle cx={s.x} cy={s.y} r={s.r} fill="#e8f0ff" fill-opacity={s.o} />
				{/each}
				<g filter="url(#{id('blur')})" style="mix-blend-mode: screen">
					{#each ribbonPaths as d, i (i)}
						<path {d} fill={RIBBON_FILLS[i % RIBBON_FILLS.length]} fill-opacity={0.4 - i * 0.06} />
					{/each}
				</g>
			{:else if variant === 'dusk'}
				<!-- Layered ridges under an amber-to-violet evening sky. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="0" y2="1">
						<stop offset="0" stop-color="#2a1a3a" />
						<stop offset="0.14" stop-color="#6b3a4a" />
						<stop offset="0.28" stop-color="#c9774a" />
						<stop offset="0.5" stop-color="#3a2238" />
						<stop offset="1" stop-color="#140f1a" />
					</linearGradient>
					<radialGradient id={id('sun')} cx="0.72" cy="0.24" r="0.32">
						<stop offset="0" stop-color="#ffd08a" stop-opacity="0.9" />
						<stop offset="0.35" stop-color="#ff9e5e" stop-opacity="0.35" />
						<stop offset="1" stop-color="#ff9e5e" stop-opacity="0" />
					</radialGradient>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<rect width="100%" height="100%" fill="url(#{id('sun')})" />
				{#each starList.slice(0, 12) as s, i (i)}
					<circle cx={s.x} cy={s.y * 0.6} r={s.r} fill="#fff4e6" fill-opacity={s.o * 0.7} />
				{/each}
				{#each ridgePaths as d, i (i)}
					<path {d} fill={RIDGE_FILLS[i]} />
				{/each}
			{:else if variant === 'tide'}
				<!-- Swells rolling in under a cold dawn, a line of light on every crest. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="0" y2="1">
						<stop offset="0" stop-color="#0d2233" />
						<stop offset="0.3" stop-color="#14465a" />
						<stop offset="1" stop-color="#07141c" />
					</linearGradient>
					<radialGradient id={id('sun')} cx="0.3" cy="0.18" r="0.45">
						<stop offset="0" stop-color="#7dd3fc" stop-opacity="0.5" />
						<stop offset="0.5" stop-color="#2dd4bf" stop-opacity="0.12" />
						<stop offset="1" stop-color="#2dd4bf" stop-opacity="0" />
					</radialGradient>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<rect width="100%" height="100%" fill="url(#{id('sun')})" />
				{#each wavePaths as wave, i (i)}
					<path d={wave.d} fill={WAVE_FILLS[i]} />
					<path
						d={wave.crest}
						stroke="#a5f3fc"
						stroke-width="0.8"
						stroke-opacity={0.35 - i * 0.04}
					/>
				{/each}
			{:else if variant === 'nebula'}
				<!-- Clouds of magenta and indigo gas along one arm, stars shining through. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="1" y2="1">
						<stop offset="0" stop-color="#1a1033" />
						<stop offset="0.6" stop-color="#110b22" />
						<stop offset="1" stop-color="#0b0816" />
					</linearGradient>
					<filter id={id('blur')} x="-50%" y="-50%" width="200%" height="200%">
						<feGaussianBlur stdDeviation="18" />
					</filter>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<g filter="url(#{id('blur')})" style="mix-blend-mode: screen">
					{#each cloudList as c, i (i)}
						<ellipse
							cx={c.cx}
							cy={c.cy}
							rx={c.rx}
							ry={c.ry}
							transform="rotate({c.angle} {c.cx} {c.cy})"
							fill={CLOUD_FILLS[i % CLOUD_FILLS.length]}
							fill-opacity="0.45"
						/>
					{/each}
				</g>
				{#each starList as s, i (i)}
					<circle cx={s.x} cy={s.y} r={s.r} fill="#f5e8ff" fill-opacity={s.o} />
				{/each}
			{:else}
				<!-- Embers rising out of a low red glow. -->
				<defs>
					<linearGradient id={id('sky')} x1="0" y1="0" x2="0" y2="1">
						<stop offset="0" stop-color="#1c0c0a" />
						<stop offset="0.55" stop-color="#2a110c" />
						<stop offset="1" stop-color="#120807" />
					</linearGradient>
					<radialGradient id={id('fire')} cx="0.55" cy="0.75" r="0.6">
						<stop offset="0" stop-color="#f97316" stop-opacity="0.55" />
						<stop offset="0.4" stop-color="#dc2626" stop-opacity="0.22" />
						<stop offset="1" stop-color="#dc2626" stop-opacity="0" />
					</radialGradient>
					<radialGradient id={id('spark')}>
						<stop offset="0" stop-color="#fff1c2" />
						<stop offset="0.4" stop-color="#fbbf24" />
						<stop offset="1" stop-color="#f97316" stop-opacity="0" />
					</radialGradient>
				</defs>
				<rect width="100%" height="100%" fill="url(#{id('sky')})" />
				<rect width="100%" height="100%" fill="url(#{id('fire')})" />
				{#each sparkList as s, i (i)}
					<circle cx={s.x} cy={s.y} r={s.r * 2.2} fill="url(#{id('spark')})" fill-opacity={s.o} />
				{/each}
			{/if}
		</svg>
	{/if}
</div>
