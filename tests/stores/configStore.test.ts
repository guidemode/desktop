import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { useConfigStore } from '../../src/stores/configStore'
import type { ProviderConfig } from '../../src/types/providers'

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(),
}))

const mockProviderConfig: ProviderConfig = {
	enabled: true,
	path: '~/.claude/projects',
}

describe('useConfigStore', () => {
	beforeEach(() => {
		// Reset store to initial state
		useConfigStore.setState({
			providerConfigs: {},
			aiApiKeys: {},
			systemConfig: {
				aiProcessingDelayMinutes: 10,
				coreMetricsDebounceSeconds: 10,
			},
			isLoading: false,
			error: null,
		})
		vi.clearAllMocks()
	})

	describe('loadProviderConfig', () => {
		it('loads config from Tauri command', async () => {
			vi.mocked(invoke).mockResolvedValue(mockProviderConfig)

			const { loadProviderConfig } = useConfigStore.getState()
			await loadProviderConfig('claude-code')

			expect(invoke).toHaveBeenCalledWith('load_provider_config_command', {
				providerId: 'claude-code',
			})
			expect(useConfigStore.getState().providerConfigs['claude-code']).toEqual(
				mockProviderConfig
			)
			expect(useConfigStore.getState().isLoading).toBe(false)
		})

		it('sets loading state during load', async () => {
			let resolveLoad: (value: ProviderConfig) => void
			const loadPromise = new Promise<ProviderConfig>(resolve => {
				resolveLoad = resolve
			})
			vi.mocked(invoke).mockReturnValue(loadPromise)

			const { loadProviderConfig } = useConfigStore.getState()
			const loadTask = loadProviderConfig('claude-code')

			// Should be loading
			expect(useConfigStore.getState().isLoading).toBe(true)

			resolveLoad!(mockProviderConfig)
			await loadTask

			expect(useConfigStore.getState().isLoading).toBe(false)
		})

		it('handles load errors', async () => {
			const error = 'Failed to load config'
			vi.mocked(invoke).mockRejectedValue(error)

			const { loadProviderConfig } = useConfigStore.getState()
			await loadProviderConfig('claude-code')

			expect(useConfigStore.getState().error).toBe(error)
			expect(useConfigStore.getState().isLoading).toBe(false)
		})

		it('clears previous errors on new load', async () => {
			// Set an error first
			useConfigStore.setState({ error: 'Previous error' })

			vi.mocked(invoke).mockResolvedValue(mockProviderConfig)

			const { loadProviderConfig } = useConfigStore.getState()
			await loadProviderConfig('claude-code')

			expect(useConfigStore.getState().error).toBeNull()
		})
	})

	describe('saveProviderConfig', () => {
		it('saves config to Tauri backend', async () => {
			vi.mocked(invoke).mockResolvedValue(undefined)

			const { saveProviderConfig } = useConfigStore.getState()
			await saveProviderConfig('claude-code', mockProviderConfig)

			expect(invoke).toHaveBeenCalledWith('save_provider_config_command', {
				providerId: 'claude-code',
				config: mockProviderConfig,
			})
			expect(useConfigStore.getState().providerConfigs['claude-code']).toEqual(
				mockProviderConfig
			)
		})

		it('updates local state after successful save', async () => {
			vi.mocked(invoke).mockResolvedValue(undefined)

			const { saveProviderConfig } = useConfigStore.getState()
			await saveProviderConfig('claude-code', mockProviderConfig)

			expect(useConfigStore.getState().providerConfigs['claude-code']).toEqual(
				mockProviderConfig
			)
		})

		it('handles save errors', async () => {
			const error = 'Failed to save config'
			vi.mocked(invoke).mockRejectedValue(error)

			const { saveProviderConfig } = useConfigStore.getState()
			await saveProviderConfig('claude-code', mockProviderConfig)

			expect(useConfigStore.getState().error).toBe(error)
			expect(useConfigStore.getState().providerConfigs['claude-code']).toBeUndefined()
		})
	})

	describe('deleteProviderConfig', () => {
		it('deletes config from backend and state', async () => {
			// Set up initial config
			useConfigStore.setState({
				providerConfigs: { 'claude-code': mockProviderConfig },
			})

			vi.mocked(invoke).mockResolvedValue(undefined)

			const { deleteProviderConfig } = useConfigStore.getState()
			await deleteProviderConfig('claude-code')

			expect(invoke).toHaveBeenCalledWith('delete_provider_config_command', {
				providerId: 'claude-code',
			})
			expect(useConfigStore.getState().providerConfigs['claude-code']).toBeUndefined()
		})

		it('handles delete errors', async () => {
			useConfigStore.setState({
				providerConfigs: { 'claude-code': mockProviderConfig },
			})

			const error = 'Failed to delete config'
			vi.mocked(invoke).mockRejectedValue(error)

			const { deleteProviderConfig } = useConfigStore.getState()
			await deleteProviderConfig('claude-code')

			expect(useConfigStore.getState().error).toBe(error)
			// Config should still exist after failed delete
			expect(useConfigStore.getState().providerConfigs['claude-code']).toEqual(
				mockProviderConfig
			)
		})
	})

	describe('AI API Keys', () => {
		it('sets AI API key', () => {
			const { setAiApiKey } = useConfigStore.getState()

			setAiApiKey('claude', 'sk-ant-test-key')

			expect(useConfigStore.getState().aiApiKeys.claude).toBe('sk-ant-test-key')
		})

		it('gets AI API key', () => {
			useConfigStore.setState({
				aiApiKeys: { claude: 'sk-ant-test-key' },
			})

			const { getAiApiKey } = useConfigStore.getState()

			expect(getAiApiKey('claude')).toBe('sk-ant-test-key')
		})

		it('returns undefined for missing key', () => {
			const { getAiApiKey } = useConfigStore.getState()

			expect(getAiApiKey('gemini')).toBeUndefined()
		})

		it('deletes AI API key', () => {
			useConfigStore.setState({
				aiApiKeys: { claude: 'sk-ant-test-key', gemini: 'ai-test-key' },
			})

			const { deleteAiApiKey } = useConfigStore.getState()
			deleteAiApiKey('claude')

			expect(useConfigStore.getState().aiApiKeys.claude).toBeUndefined()
			expect(useConfigStore.getState().aiApiKeys.gemini).toBe('ai-test-key')
		})

		it('supports multiple API keys', () => {
			const { setAiApiKey } = useConfigStore.getState()

			setAiApiKey('claude', 'sk-ant-test-key')
			setAiApiKey('gemini', 'ai-test-key')

			const { aiApiKeys } = useConfigStore.getState()
			expect(aiApiKeys.claude).toBe('sk-ant-test-key')
			expect(aiApiKeys.gemini).toBe('ai-test-key')
		})
	})

	describe('System Config', () => {
		it('updates system config', () => {
			const { updateSystemConfig } = useConfigStore.getState()

			updateSystemConfig({ aiProcessingDelayMinutes: 15 })

			expect(useConfigStore.getState().systemConfig.aiProcessingDelayMinutes).toBe(15)
			// Other config values should remain unchanged
			expect(useConfigStore.getState().systemConfig.coreMetricsDebounceSeconds).toBe(10)
		})

		it('updates multiple system config values', () => {
			const { updateSystemConfig } = useConfigStore.getState()

			updateSystemConfig({
				aiProcessingDelayMinutes: 20,
				coreMetricsDebounceSeconds: 5,
			})

			expect(useConfigStore.getState().systemConfig.aiProcessingDelayMinutes).toBe(20)
			expect(useConfigStore.getState().systemConfig.coreMetricsDebounceSeconds).toBe(5)
		})

		it('gets system config', () => {
			const { getSystemConfig } = useConfigStore.getState()

			const config = getSystemConfig()

			expect(config).toEqual({
				aiProcessingDelayMinutes: 10,
				coreMetricsDebounceSeconds: 10,
			})
		})
	})

	describe('Error handling', () => {
		it('clears error', () => {
			useConfigStore.setState({ error: 'Test error' })

			const { clearError } = useConfigStore.getState()
			clearError()

			expect(useConfigStore.getState().error).toBeNull()
		})
	})

	describe('Default values', () => {
		it('has correct default values', () => {
			const state = useConfigStore.getState()

			expect(state.providerConfigs).toEqual({})
			expect(state.aiApiKeys).toEqual({})
			expect(state.systemConfig.aiProcessingDelayMinutes).toBe(10)
			expect(state.systemConfig.coreMetricsDebounceSeconds).toBe(10)
			expect(state.isLoading).toBe(false)
			expect(state.error).toBeNull()
		})
	})

	describe('Type validation', () => {
		it('maintains type safety for provider configs', async () => {
			vi.mocked(invoke).mockResolvedValue(mockProviderConfig)

			const { loadProviderConfig } = useConfigStore.getState()
			await loadProviderConfig('claude-code')

			const config = useConfigStore.getState().providerConfigs['claude-code']

			// TypeScript should enforce these properties exist
			expect(typeof config?.enabled).toBe('boolean')
			expect(typeof config?.path).toBe('string')
		})
	})
})
