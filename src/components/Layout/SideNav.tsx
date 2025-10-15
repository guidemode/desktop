import { useLocation, useNavigate } from 'react-router-dom'
import { CODING_AGENTS } from '../../types/providers'
import ProviderIcon from '../icons/ProviderIcon'
import { useAuth } from '../../hooks/useAuth'
import { useProviderConfig } from '../../hooks/useProviderConfig'
import { useDirectoryExists } from '../../hooks/useDirectoryExists'
import { open } from '@tauri-apps/plugin-shell'

interface NavItem {
  path: string
  label: string
  icon: string
  type?: 'main' | 'section' | 'provider'
}

interface ProviderNavItemProps {
  item: NavItem
  provider: typeof CODING_AGENTS[0] | undefined
  isActive: boolean
  onClick: () => void
}

function ProviderNavItem({ item, provider, isActive, onClick }: ProviderNavItemProps) {
  // Get provider config to check home directory
  const { data: config } = useProviderConfig(provider?.id || '')
  const homeDir = config?.homeDirectory || provider?.defaultHomeDirectory

  // Check if directory exists
  const { data: directoryExists } = useDirectoryExists(homeDir, !!provider)

  // Determine if provider is unavailable (directory doesn't exist)
  const isUnavailable = provider && directoryExists === false

  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
        isActive
          ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
          : isUnavailable
          ? 'text-base-content/50 hover:bg-base-200/50 opacity-60'
          : 'text-base-content hover:bg-base-200'
      }`}
    >
      {provider ? (
        <div className={`flex-shrink-0 w-5 h-5 flex items-center justify-center ${isUnavailable ? 'opacity-50' : ''}`}>
          <ProviderIcon providerId={provider.id} size={20} />
        </div>
      ) : (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={item.icon} />
        </svg>
      )}
      <span className="flex-1">{item.label}</span>
    </button>
  )
}

const navItems: NavItem[] = [
  {
    path: '/',
    label: 'Dashboard',
    icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6',
    type: 'main'
  },
  {
    path: '/sessions',
    label: 'Sessions',
    icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z',
    type: 'main'
  },
  {
    path: '/projects',
    label: 'Projects',
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
    type: 'main'
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
    icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z',
    type: 'section'
  },
]

function SideNav() {
  const location = useLocation()
  const navigate = useNavigate()
  const { user } = useAuth()

  const handleNavClick = (path: string) => {
    navigate(path)
  }

  const handleVisitGuideAI = async () => {
    if (user?.serverUrl) {
      await open(user.serverUrl)
    }
  }

  return (
    <aside className="w-64 bg-base-100 border-r border-base-300 h-full">
      {/* Navigation Menu */}
      <nav className="p-4 space-y-1">
        {/* Dashboard */}
        <div className="mb-4">
          {navItems.filter(item => item.type === 'main').map(item => {
            const isActive = location.pathname === item.path
            const dataTour = item.path === '/sessions' ? 'sessions-nav' : undefined

            return (
              <button
                key={item.path}
                onClick={() => handleNavClick(item.path)}
                className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
                  isActive
                    ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
                    : 'text-base-content hover:bg-base-200'
                }`}
                data-tour={dataTour}
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
                <ProviderNavItem
                  key={item.path}
                  item={item}
                  provider={provider}
                  isActive={isActive}
                  onClick={() => handleNavClick(item.path)}
                />
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
            {navItems.filter(item => item.type === 'section').map(item => {
              const isActive = location.pathname === item.path
              const dataTour = item.path === '/upload-queue' ? 'upload-queue-nav' :
                               item.path === '/sessions' ? 'sessions-nav' :
                               item.path === '/settings' ? 'settings-nav' : undefined

              return (
                <button
                  key={item.path}
                  onClick={() => handleNavClick(item.path)}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all text-left ${
                    isActive
                      ? 'bg-gradient-to-r from-green-600 to-blue-600 text-white shadow-sm hover:from-green-700 hover:to-blue-700'
                      : 'text-base-content hover:bg-base-200'
                  }`}
                  data-tour={dataTour}
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

        {/* Visit GuideAI - Bottom Section */}
        {user && (
          <div className="mt-6 pt-4 border-t border-base-300">
            <button
              onClick={handleVisitGuideAI}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-lg transition-all bg-base-200 hover:bg-base-300 border border-base-300"
            >
              <div className="avatar">
                <div className="w-5 rounded">
                  <img src="/logo-32-optimized.png" alt="GuideAI" className="w-full h-full object-contain" />
                </div>
              </div>
              <span className="font-semibold text-base-content">Visit GuideAI</span>
              <svg className="w-4 h-4 text-base-content/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </button>
          </div>
        )}
      </nav>
    </aside>
  )
}

export default SideNav