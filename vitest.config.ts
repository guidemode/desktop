import path from 'node:path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
	test: {
		globals: true,
		environment: 'jsdom',
		include: ['tests/**/*.{test,spec}.{ts,tsx}'],
		exclude: ['node_modules', 'dist', 'src-tauri'],
		setupFiles: ['./tests/setup.ts'],
		coverage: {
			provider: 'v8',
			reporter: ['text', 'html', 'json-summary'],
			include: ['src/**/*.{ts,tsx}'],
			exclude: [
				'src/**/*.d.ts',
				'src/**/*.test.{ts,tsx}',
				'src/vite-env.d.ts',
				'src/main.tsx', // App entry point
			],
		},
	},
	resolve: {
		alias: {
			'@': path.resolve(__dirname, './src'),
		},
	},
})
