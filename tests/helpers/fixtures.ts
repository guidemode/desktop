/**
 * Test fixtures for desktop app tests
 */

import type { LocalSession } from '../../src/types/session'

export function createMockSession(overrides?: Partial<LocalSession>): LocalSession {
	const now = Date.now()
	return {
		id: 'row-1',
		sessionId: 'session-123',
		provider: 'claude-code',
		fileName: 'session.jsonl',
		filePath: '/tmp/session.jsonl',
		projectName: 'Test Project',
		projectId: 'proj-123',
		sessionStartTime: now - 60000,
		sessionEndTime: now,
		fileSize: 2048,
		durationMs: 60000,
		processingStatus: 'completed',
		processedAt: now,
		assessmentStatus: 'completed',
		assessmentCompletedAt: now,
		assessmentRating: 4,
		aiModelSummary: 'Test session summary',
		aiModelQualityScore: 0.9,
		aiModelMetadata: null,
		createdAt: now,
		uploadedAt: now,
		syncedToServer: true,
		metrics: {
			response_latency_ms: 120,
			task_completion_time_ms: 450,
			read_write_ratio: 1.5,
			input_clarity_score: 0.7,
			task_success_rate: 0.8,
			iteration_count: 3,
			process_quality_score: 0.9,
			used_plan_mode: true,
			used_todo_tracking: false,
			interruption_rate: 0.1,
			session_length_minutes: 15,
			error_count: 2,
			fatal_errors: 0,
		},
		...overrides,
	}
}

export function createMockConfig() {
	return {
		apiKey: 'test-api-key',
		serverUrl: 'https://test.guidemode.dev',
		username: 'testuser',
		tenantId: 'tenant-123',
		tenantName: 'Test Org',
	}
}

export function createMockProviderConfig() {
	return {
		'claude-code': {
			enabled: true,
			path: '~/.claude/projects',
		},
		cursor: {
			enabled: false,
			path: '',
		},
		copilot: {
			enabled: true,
			path: '~/.copilot/sessions',
		},
	}
}

export const MOCK_JSONL_CONTENT = `{"type":"user","content":"Test message","timestamp":"2025-01-15T10:00:00.000Z"}
{"type":"assistant","content":"Test response","timestamp":"2025-01-15T10:00:05.000Z"}`

export const MOCK_SESSION_LIST = [
	createMockSession({ sessionId: 'session-1', projectName: 'Project A' }),
	createMockSession({ sessionId: 'session-2', projectName: 'Project B', provider: 'cursor' }),
	createMockSession({ sessionId: 'session-3', projectName: 'Project A', provider: 'copilot' }),
]
