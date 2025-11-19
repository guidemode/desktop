import Database from '@tauri-apps/plugin-sql'

let dbInstance: Database | null = null

/**
 * Initialize the local SQLite database
 * Database is stored in Tauri's app data directory
 * (e.g., ~/Library/Application Support/com.guidemode.desktop/ on macOS)
 */
export async function initializeDatabase() {
  if (dbInstance) {
    return dbInstance
  }

  try {
    // Connect to SQLite database (plugin handles the path)
    dbInstance = await Database.load('sqlite:guidemode.db')

    console.log('✓ Local database initialized successfully')
    return dbInstance
  } catch (error) {
    console.error('Failed to initialize database:', error)
    throw error
  }
}

/**
 * Get the database instance (must call initializeDatabase first)
 */
export function getDatabase() {
  if (!dbInstance) {
    throw new Error('Database not initialized. Call initializeDatabase() first.')
  }
  return dbInstance
}

/**
 * Close the database connection
 */
export async function closeDatabase() {
  if (dbInstance) {
    await dbInstance.close()
    dbInstance = null
  }
}
