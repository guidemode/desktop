/**
 * Custom render utilities for React Testing Library
 */

import type { RenderOptions } from '@testing-library/react'
import { render } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import type { ReactElement, ReactNode } from 'react'

function createTestQueryClient() {
	return new QueryClient({
		defaultOptions: {
			queries: {
				retry: false,
				gcTime: 0,
			},
			mutations: {
				retry: false,
			},
		},
	})
}

interface CustomRenderOptions extends Omit<RenderOptions, 'wrapper'> {
	queryClient?: QueryClient
	withRouter?: boolean
}

function AllTheProviders({
	children,
	queryClient,
	withRouter = false,
}: {
	children: ReactNode
	queryClient?: QueryClient
	withRouter?: boolean
}) {
	const client = queryClient || createTestQueryClient()

	let content = <QueryClientProvider client={client}>{children}</QueryClientProvider>

	if (withRouter) {
		content = <BrowserRouter>{content}</BrowserRouter>
	}

	return content
}

export function renderWithProviders(
	ui: ReactElement,
	{ queryClient, withRouter = false, ...options }: CustomRenderOptions = {}
) {
	const client = queryClient || createTestQueryClient()

	const Wrapper = ({ children }: { children: ReactNode }) => (
		<AllTheProviders queryClient={client} withRouter={withRouter}>
			{children}
		</AllTheProviders>
	)

	return {
		...render(ui, { wrapper: Wrapper, ...options }),
		queryClient: client,
	}
}

export { createTestQueryClient }
