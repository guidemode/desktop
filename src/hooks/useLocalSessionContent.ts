import {
  type BaseSessionMessage,
  type ProcessedTimeline,
  messageProcessorRegistry,
  parserRegistry,
} from '@guidemode/session-processing/ui'
import { useQuery } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

interface UseLocalSessionContentResult {
  messages: BaseSessionMessage[]
  timeline: ProcessedTimeline | null
  fileContent: string | null
  loading: boolean
  error: string | null
}

interface SessionContentData {
  messages: BaseSessionMessage[]
  timeline: ProcessedTimeline | null
  fileContent: string
}

async function fetchSessionContent(
  sessionId: string,
  provider: string,
  filePath: string
): Promise<SessionContentData> {
  // Fetch content from Tauri
  const content = await invoke<string>('get_session_content', {
    provider,
    filePath,
    sessionId,
  })

  // Parse the session file using the provider-specific parser
  const parser = parserRegistry.getParser(provider)
  if (!parser) {
    throw new Error(`No parser found for provider: ${provider}`)
  }

  const parsedSession = parser.parseSession(content)

  // Convert ParsedMessage[] to BaseSessionMessage[] by serializing Date timestamps
  const baseMessages = parsedSession.messages.map(msg => ({
    ...msg,
    timestamp: msg.timestamp instanceof Date ? msg.timestamp.toISOString() : msg.timestamp,
  }))

  // Process messages for timeline display
  const processor = messageProcessorRegistry.getProcessor(provider)
  const processedTimeline = processor.process(baseMessages)

  return {
    messages: baseMessages,
    timeline: processedTimeline,
    fileContent: content,
  }
}

export function useLocalSessionContent(
  sessionId: string | undefined,
  provider: string | undefined,
  filePath: string | undefined
): UseLocalSessionContentResult {
  const {
    data,
    isLoading: loading,
    error,
  } = useQuery({
    queryKey: ['session-content', sessionId, provider, filePath],
    queryFn: () => {
      // Type guard: enabled ensures these are defined
      if (!sessionId || !provider || !filePath) {
        throw new Error('Missing required parameters')
      }
      return fetchSessionContent(sessionId, provider, filePath)
    },
    enabled: !!(sessionId && provider && filePath),
  })

  return {
    messages: data?.messages || [],
    timeline: data?.timeline || null,
    fileContent: data?.fileContent || null,
    loading,
    error: error ? (error as Error).message : null,
  }
}
