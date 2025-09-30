import { useLocation, useNavigate } from 'react-router-dom'
import { CODING_AGENTS } from '../../types/providers'

interface NavItem {
  path: string
  label: string
  icon: string
  type?: 'section' | 'provider'
}

const navItems: NavItem[] = [
  {
    path: '/overview',
    label: 'Overview',
    icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z',
    type: 'section'
  },
  ...CODING_AGENTS.map(agent => ({
    path: `/provider/${agent.id}`,
    label: agent.name,
    icon: agent.icon,
    type: 'provider' as const
  })),
  {
    path: '/upload-queue',
    label: 'Upload Queue',
    icon: 'M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12',
    type: 'section'
  },
  {
    path: '/settings',
    label: 'Settings',
    icon: 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z',
    type: 'section'
  },
]

function SideNav() {
  const location = useLocation()
  const navigate = useNavigate()

  const handleNavClick = (path: string) => {
    navigate(path)
  }

  return (
    <aside className="w-64 bg-base-100 border-r border-base-300 h-full">
      {/* Navigation Menu */}
      <nav className="p-4 space-y-1">
        {/* General Section */}
        <div className="mb-4">
          {navItems.filter(item => item.type === 'section' || !item.type).slice(0, 1).map(item => {
            const isActive = location.pathname === item.path

            return (
              <button
                key={item.path}
                onClick={() => handleNavClick(item.path)}
                className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
                  isActive
                    ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
                    : 'text-base-content hover:bg-base-200'
                }`}
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={item.icon} />
                </svg>
                <span className="flex-1">{item.label}</span>
              </button>
            )
          })}
        </div>

        {/* Providers Section */}
        <div className="mb-4">
          <div className="px-4 py-2 text-xs font-semibold text-base-content/60 uppercase tracking-wider">
            Providers
          </div>
          <div className="space-y-1">
            {navItems.filter(item => item.type === 'provider').map(item => {
              const isActive = location.pathname === item.path
              const provider = CODING_AGENTS.find(agent => item.path === `/provider/${agent.id}`)

              return (
                <button
                  key={item.path}
                  onClick={() => handleNavClick(item.path)}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
                    isActive
                      ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
                      : 'text-base-content hover:bg-base-200'
                  }`}
                >
                  {provider ? (
                    <div className={`avatar placeholder w-5 h-5`}>
                      <div className={`bg-gradient-to-r ${provider.color} rounded w-5 h-5 text-white flex items-center justify-center`}>
                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={provider.icon} />
                        </svg>
                      </div>
                    </div>
                  ) : (
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={item.icon} />
                    </svg>
                  )}
                  <span className="flex-1">{item.label}</span>
                </button>
              )
            })}
          </div>
        </div>

        {/* System Section */}
        <div>
          <div className="px-4 py-2 text-xs font-semibold text-base-content/60 uppercase tracking-wider">
            System
          </div>
          <div className="space-y-1">
            {navItems.filter(item => item.type === 'section' || !item.type).slice(1).map(item => {
              const isActive = location.pathname === item.path

              return (
                <button
                  key={item.path}
                  onClick={() => handleNavClick(item.path)}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
                    isActive
                      ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
                      : 'text-base-content hover:bg-base-200'
                  }`}
                >
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={item.icon} />
                  </svg>
                  <span className="flex-1">{item.label}</span>
                </button>
              )
            })}
          </div>
        </div>
      </nav>
    </aside>
  )
}

export default SideNav