import type React from 'react'
import { useLocation } from 'react-router-dom'
import Header from './Header'
import SideNav from './SideNav'

interface AppLayoutProps {
  children: React.ReactNode
}

function AppLayout({ children }: AppLayoutProps) {
  const location = useLocation()

  // Pages that should use full width without max-width constraint
  const isFullWidthPage =
    location.pathname === '/logs' ||
    (location.pathname.startsWith('/provider/') && location.search.includes('showLogs=true'))

  return (
    <div className="h-screen bg-base-100 overflow-hidden">
      <Header />
      <div className="flex h-[calc(100vh-3rem)]">
        <SideNav />
        <main className="flex-1 overflow-auto main-gradient p-6">
          {isFullWidthPage ? children : <div className="max-w-7xl mx-auto">{children}</div>}
        </main>
      </div>
    </div>
  )
}

export default AppLayout
