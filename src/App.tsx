import { BrowserRouter as Router, Routes, Route, Navigate } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useAuth } from './hooks/useAuth'
import Login from './components/Login'
import AppLayout from './components/Layout/AppLayout'
import OverviewPage from './pages/OverviewPage'
import ProviderPage from './pages/ProviderPage'
import SettingsPage from './pages/SettingsPage'
import LogsPage from './pages/LogsPage'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000, // 5 minutes
      retry: 2,
    },
  },
})

function App() {
  const { user, isLoading } = useAuth()

  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-screen bg-base-100" data-theme="guideai">
        <span className="loading loading-spinner loading-lg"></span>
      </div>
    )
  }

  if (!user) {
    return (
      <div className="min-h-screen bg-base-100" data-theme="guideai">
        <div className="container mx-auto px-2 py-3 max-w-md">
          <div className="text-center mb-4">
            <div className="flex items-center justify-center gap-2 mb-1">
              <div className="avatar">
                <div className="w-8 rounded">
                  <img src="/logo-colored.png" alt="GuideAI" className="w-full h-full object-contain" />
                </div>
              </div>
              <h1 className="text-lg font-bold text-primary">GuideAI</h1>
            </div>
            <p className="text-sm text-base-content/70">Desktop Manager</p>
          </div>
          <Login />
        </div>
      </div>
    )
  }

  return (
    <QueryClientProvider client={queryClient}>
      <Router>
        <AppLayout>
          <Routes>
            <Route path="/" element={<Navigate to="/overview" replace />} />
            <Route path="/overview" element={<OverviewPage />} />
            <Route path="/provider/:providerId" element={<ProviderPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/logs" element={<LogsPage />} />
          </Routes>
        </AppLayout>
      </Router>
    </QueryClientProvider>
  )
}

export default App