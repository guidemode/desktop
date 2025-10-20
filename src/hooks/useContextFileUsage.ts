import { useMemo } from 'react'

interface ContextFile {
  fileName: string
  filePath: string
  relativePath: string
  content: string
  size: number
}

export interface FileUsageStats {
  toolCalls: number
  messages: number
}

/**
 * Hook to track usage of context files within a session transcript.
 *
 * Scans the raw transcript file content for mentions of each context file's
 * relative path. Separates counts into tool calls vs regular messages.
 *
 * @param fileContent - Raw session transcript (JSONL format)
 * @param contextFiles - List of context files found in the project
 * @returns Map of relativePath -> FileUsageStats
 */
export function useContextFileUsage(
  fileContent: string | null,
  contextFiles: ContextFile[]
): Map<string, FileUsageStats> {
  return useMemo(() => {
    const usageCounts = new Map<string, FileUsageStats>()

    // Return empty map if no content to scan
    if (!fileContent || contextFiles.length === 0) {
      return usageCounts
    }

    // Parse JSONL content into individual messages
    const lines = fileContent.split('\n').filter(line => line.trim())
    const messages: any[] = []

    for (const line of lines) {
      try {
        messages.push(JSON.parse(line))
      } catch (_err) {}
    }

    // Scan each context file for mentions
    for (const contextFile of contextFiles) {
      const { relativePath } = contextFile
      let toolCallCount = 0
      let messageCount = 0

      // Create variations to search for (handle different path separators)
      const pathVariations = [
        relativePath,
        relativePath.replace(/\\/g, '/'), // Normalize to forward slashes
        relativePath.replace(/\//g, '\\'), // Normalize to backslashes
      ]

      // Track seen messages to avoid duplicate counting within same message
      const seenMessageIds = new Set<string>()

      // Scan all messages
      for (const message of messages) {
        const messageId = message.uuid || message.timestamp || JSON.stringify(message)

        // Skip if we've already counted this message for this file
        if (seenMessageIds.has(messageId)) {
          continue
        }

        // Convert message to string for searching
        const messageStr = JSON.stringify(message).toLowerCase()
        const found = pathVariations.some(variation => messageStr.includes(variation.toLowerCase()))

        if (found) {
          // Determine if this is a tool call or a regular message
          const isToolCall = isToolCallMessage(message)

          if (isToolCall) {
            toolCallCount++
          } else {
            messageCount++
          }

          seenMessageIds.add(messageId)
        }
      }

      if (toolCallCount > 0 || messageCount > 0) {
        usageCounts.set(relativePath, { toolCalls: toolCallCount, messages: messageCount })
      }
    }

    return usageCounts
  }, [fileContent, contextFiles])
}

/**
 * Determine if a message is a tool call (tool_use or tool_result)
 */
function isToolCallMessage(message: any): boolean {
  // Check message type field
  if (message.type === 'tool_use' || message.type === 'tool_result') {
    return true
  }

  // Check for tool content in message.message.content array
  if (message.message?.content) {
    const content = message.message.content
    if (Array.isArray(content)) {
      return content.some((item: any) => item.type === 'tool_use' || item.type === 'tool_result')
    }
  }

  // Check for tool-related fields in the message
  if (message.tool_use_id || message.tool_name || message.input || message.callId) {
    return true
  }

  return false
}
