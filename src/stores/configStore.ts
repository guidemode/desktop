import { invoke } from '@tauri-apps/api/core'
import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ProviderConfig } from '../types/providers'

interface AiApiKeys {
  claude?: string
  gemini?: string
  openai?: string
}

interface AiModelConfig {
  gemini?: string
  openai?: string
}

interface SystemConfig {
  aiProcessingDelayMinutes: number
  coreMetricsDebounceSeconds: number
  preferredAiProvider?: 'claude' | 'gemini' | 'openai' | 'auto'
  aiModels?: AiModelConfig
}

interface ConfigState {
  providerConfigs: Record<string, ProviderConfig>
  aiApiKeys: AiApiKeys
  systemConfig: SystemConfig
  isLoading: boolean
  error: string | null

  // Provider config actions
  loadProviderConfig: (providerId: string) => Promise<void>
  saveProviderConfig: (providerId: string, config: ProviderConfig) => Promise<void>
  deleteProviderConfig: (providerId: string) => Promise<void>

  // AI API key actions
  setAiApiKey: (provider: 'claude' | 'gemini' | 'openai', apiKey: string) => void
  deleteAiApiKey: (provider: 'claude' | 'gemini' | 'openai') => void
  getAiApiKey: (provider: 'claude' | 'gemini' | 'openai') => string | undefined

  // System config actions
  updateSystemConfig: (config: Partial<SystemConfig>) => void
  getSystemConfig: () => SystemConfig
  setPreferredAiProvider: (provider: 'claude' | 'gemini' | 'openai' | 'auto') => void
  setAiModel: (provider: 'gemini' | 'openai', model: string) => void
  getAiModel: (provider: 'gemini' | 'openai') => string | undefined

  clearError: () => void
}

export const useConfigStore = create<ConfigState>()(
  persist(
    (set, get) => ({
      providerConfigs: {},
      aiApiKeys: {},
      systemConfig: {
        aiProcessingDelayMinutes: 10,
        coreMetricsDebounceSeconds: 10,
      },
      isLoading: false,
      error: null,

      loadProviderConfig: async (providerId: string) => {
        try {
          set({ isLoading: true, error: null })
          const config = await invoke<ProviderConfig>('load_provider_config_command', {
            providerId,
          })
          set(state => ({
            providerConfigs: { ...state.providerConfigs, [providerId]: config },
            isLoading: false,
          }))
        } catch (error) {
          set({ error: error as string, isLoading: false })
        }
      },

      saveProviderConfig: async (providerId: string, config: ProviderConfig) => {
        try {
          set({ isLoading: true, error: null })
          await invoke('save_provider_config_command', { providerId, config })
          set(state => ({
            providerConfigs: { ...state.providerConfigs, [providerId]: config },
            isLoading: false,
          }))
        } catch (error) {
          set({ error: error as string, isLoading: false })
        }
      },

      deleteProviderConfig: async (providerId: string) => {
        try {
          set({ isLoading: true, error: null })
          await invoke('delete_provider_config_command', { providerId })
          set(state => {
            const newConfigs = { ...state.providerConfigs }
            delete newConfigs[providerId]
            return { providerConfigs: newConfigs, isLoading: false }
          })
        } catch (error) {
          set({ error: error as string, isLoading: false })
        }
      },

      setAiApiKey: (provider: 'claude' | 'gemini' | 'openai', apiKey: string) => {
        set(state => ({
          aiApiKeys: { ...state.aiApiKeys, [provider]: apiKey },
        }))
      },

      deleteAiApiKey: (provider: 'claude' | 'gemini' | 'openai') => {
        set(state => {
          const newKeys = { ...state.aiApiKeys }
          delete newKeys[provider]
          return { aiApiKeys: newKeys }
        })
      },

      getAiApiKey: (provider: 'claude' | 'gemini' | 'openai') => {
        return get().aiApiKeys[provider]
      },

      updateSystemConfig: (config: Partial<SystemConfig>) => {
        set(state => ({
          systemConfig: { ...state.systemConfig, ...config },
        }))
      },

      getSystemConfig: () => {
        return get().systemConfig
      },

      setPreferredAiProvider: (provider: 'claude' | 'gemini' | 'openai' | 'auto') => {
        set(state => ({
          systemConfig: { ...state.systemConfig, preferredAiProvider: provider },
        }))
      },

      setAiModel: (provider: 'gemini' | 'openai', model: string) => {
        set(state => ({
          systemConfig: {
            ...state.systemConfig,
            aiModels: { ...state.systemConfig.aiModels, [provider]: model },
          },
        }))
      },

      getAiModel: (provider: 'gemini' | 'openai') => {
        return get().systemConfig.aiModels?.[provider]
      },

      clearError: () => set({ error: null }),
    }),
    {
      name: 'guidemode-config-storage',
      // Persist AI API keys and system config, not provider configs (those are in Tauri backend)
      partialize: state => ({
        aiApiKeys: state.aiApiKeys,
        systemConfig: state.systemConfig,
      }),
    }
  )
)
