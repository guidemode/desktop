import {
  type BaseSessionMessage,
  type ProcessedTimeline,
  messageProcessorRegistry,
  sessionRegistry,
} from '@guideai-dev/session-processing/ui'
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

  // Parse the session file
  const parser = sessionRegistry.findParser(content)
  if (!parser) {
    throw new Error('No suitable parser found for session content')
  }

  const parsedMessages = sessionRegistry.parseSession(content, provider)

  // Process messages for timeline display
  const processor = messageProcessorRegistry.getProcessor(provider)
  const processedTimeline = processor.process(parsedMessages)

  return {
    messages: parsedMessages,
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
    queryFn: () => fetchSessionContent(sessionId!, provider!, filePath!),
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
