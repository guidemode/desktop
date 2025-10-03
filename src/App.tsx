import { BrowserRouter as Router, Routes, Route, useNavigate } from 'react-router-dom'
import { useAuth } from './hooks/useAuth'
import { useDatabase } from './hooks/useDatabase'
import { useSessionIngest } from './hooks/useSessionIngest'
import AppLayout from './components/Layout/AppLayout'
import DashboardPage from './pages/DashboardPage'
import OverviewPage from './pages/OverviewPage'
import ProviderPage from './pages/ProviderPage'
import SessionsPage from './pages/SessionsPage'
import SessionDetailPage from './pages/SessionDetailPage'
import ProjectsPage from './pages/ProjectsPage'
import SettingsPage from './pages/SettingsPage'
import UploadQueuePage from './pages/UploadQueuePage'
import { ToastContainer } from './components/ToastContainer'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

function AppContent() {
  const navigate = useNavigate()

  // Start listening for session detection events
  useSessionIngest()

  // NOTE: Background metric processing is available via useBackgroundProcessing()
  // but not enabled by default. Can be triggered manually from UI if needed.

  useEffect(() => {
    // Listen for navigation events from the menubar window
    const unlisten = listen('navigate', (event) => {
      const route = event.payload as string
      navigate(route)
    })

    return () => {
      unlisten.then(fn => fn())
    }
  }, [navigate])

  return (
    <AppLayout>
      <Routes>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/overview" element={<OverviewPage />} />
        <Route path="/provider/:providerId" element={<ProviderPage />} />
        <Route path="/sessions" element={<SessionsPage />} />
        <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
        <Route path="/projects" element={<ProjectsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/upload-queue" element={<UploadQueuePage />} />
      </Routes>
    </AppLayout>
  )
}

function App() {
  const { isLoading } = useAuth()
  const { isReady: isDbReady, error: dbError } = useDatabase()

  // Get theme from localStorage or default to dark
  const theme = typeof window !== 'undefined'
    ? (localStorage.getItem('theme') || 'guideai-dark')
    : 'guideai-dark'

  if (isLoading || !isDbReady) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-base-100" data-theme={theme}>
        <div className="text-center">
          <span className="loading loading-spinner loading-lg"></span>
          {!isDbReady && <p className="mt-4 text-sm text-base-content/70">Initializing database...</p>}
        </div>
      </div>
    )
  }

  if (dbError) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-base-100" data-theme={theme}>
        <div className="alert alert-error max-w-md">
          <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <div>
            <div className="font-bold">Database Error</div>
            <div className="text-sm">{dbError.message}</div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <Router>
      <AppContent />
      <ToastContainer />
    </Router>
  )
}

export default App