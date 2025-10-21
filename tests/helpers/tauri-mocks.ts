/**
 * Mock Tauri invoke commands for testing
 */

import { vi } from 'vitest'

export function createMockInvoke() {
	return vi.fn().mockImplementation((command: string, args?: any) => {
		switch (command) {
			case 'execute_sql':
				return Promise.resolve([])
			case 'read_session_file':
				return Promise.resolve('{"type":"user","content":"test"}')
			case 'load_config_command':
				return Promise.resolve(null)
			case 'save_config_command':
				return Promise.resolve(true)
			case 'clear_config_command':
				return Promise.resolve(true)
			case 'login_command':
				return Promise.resolve({ success: true })
			case 'logout_command':
				return Promise.resolve(true)
			default:
				return Promise.reject(new Error(`Unknown command: ${command}`))
		}
	})
}

export function mockInvokeWithData(dataMap: Record<string, any>) {
	return vi.fn().mockImplementation((command: string, args?: any) => {
		if (command in dataMap) {
			return Promise.resolve(dataMap[command])
		}
		return Promise.reject(new Error(`No mock data for command: ${command}`))
	})
}

export function mockInvokeWithError(command: string, error: Error) {
	return vi.fn().mockImplementation((cmd: string) => {
		if (cmd === command) {
			return Promise.reject(error)
		}
		return Promise.resolve(null)
	})
}
