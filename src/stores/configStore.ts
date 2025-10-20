import { invoke } from '@tauri-apps/api/core'
import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ProviderConfig } from '../types/providers'

interface AiApiKeys {
  claude?: string
  gemini?: string
}

interface SystemConfig {
  aiProcessingDelayMinutes: number
  coreMetricsDebounceSeconds: number
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
  setAiApiKey: (provider: 'claude' | 'gemini', apiKey: string) => void
  deleteAiApiKey: (provider: 'claude' | 'gemini') => void
  getAiApiKey: (provider: 'claude' | 'gemini') => string | undefined

  // System config actions
  updateSystemConfig: (config: Partial<SystemConfig>) => void
  getSystemConfig: () => SystemConfig

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

      setAiApiKey: (provider: 'claude' | 'gemini', apiKey: string) => {
        set(state => ({
          aiApiKeys: { ...state.aiApiKeys, [provider]: apiKey },
        }))
      },

      deleteAiApiKey: (provider: 'claude' | 'gemini') => {
        set(state => {
          const newKeys = { ...state.aiApiKeys }
          delete newKeys[provider]
          return { aiApiKeys: newKeys }
        })
      },

      getAiApiKey: (provider: 'claude' | 'gemini') => {
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

      clearError: () => set({ error: null }),
    }),
    {
      name: 'guideai-config-storage',
      // Persist AI API keys and system config, not provider configs (those are in Tauri backend)
      partialize: state => ({
        aiApiKeys: state.aiApiKeys,
        systemConfig: state.systemConfig,
      }),
    }
  )
)
