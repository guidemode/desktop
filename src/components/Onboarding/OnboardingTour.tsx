import { useCallback, useEffect } from 'react'
import Joyride, {
  type CallBackProps,
  type Step,
  STATUS,
  EVENTS,
  ACTIONS,
  type TooltipRenderProps,
} from 'react-joyride'
import { useNavigate } from 'react-router-dom'
import { useOnboarding } from '../../hooks/useOnboarding'

// Global styles for Joyride arrow to match border (base-300 color)
const joyrideStyles = `
  [data-theme="guideai-light"] .__floater__arrow polygon {
    fill: #e2e8f0 !important;
  }

  [data-theme="guideai-dark"] .__floater__arrow polygon {
    fill: #475569 !important;
  }
`

// Custom tooltip component using Tailwind/DaisyUI classes
function CustomTooltip({
  continuous,
  index,
  step,
  backProps,
  closeProps,
  primaryProps,
  skipProps,
  tooltipProps,
  isLastStep,
}: TooltipRenderProps) {
  return (
    <div
      {...tooltipProps}
      className="bg-base-200 text-base-content rounded-lg shadow-xl p-5 max-w-md border-2 border-base-300"
    >
      {step.title && <h3 className="text-lg font-bold mb-2">{step.title}</h3>}
      <div className="text-sm">{step.content}</div>
      <div className="flex items-center justify-between mt-4 gap-3">
        <button {...skipProps} className="btn btn-ghost btn-sm">
          {skipProps.title}
        </button>
        <div className="flex gap-2">
          {index > 0 && (
            <button {...backProps} className="btn btn-ghost btn-sm">
              {backProps.title}
            </button>
          )}
          {continuous && (
            <button {...primaryProps} className="btn btn-primary btn-sm">
              {isLastStep ? 'Finish' : primaryProps.title}
            </button>
          )}
          {!continuous && (
            <button {...closeProps} className="btn btn-primary btn-sm">
              {closeProps.title}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

// Helper function to restore scrolling on all potentially affected elements
function restoreScrolling() {
  document.body.style.overflow = ''
  document.documentElement.style.overflow = ''
  const mainElement = document.querySelector('main')
  if (mainElement instanceof HTMLElement) {
    mainElement.style.overflow = ''
  }
}

export function OnboardingTour() {
  const navigate = useNavigate()
  const { isTourRunning, currentStepIndex, completeTour, stopTour, setStepIndex } = useOnboarding()

  // Ensure scrolling is re-enabled when tour ends
  useEffect(() => {
    if (!isTourRunning) {
      // Use a slight delay to ensure Joyride has fully unmounted
      const timer = setTimeout(() => {
        restoreScrolling()
      }, 100)

      return () => clearTimeout(timer)
    }
  }, [isTourRunning])

  // Define tour steps
  const steps: Step[] = [
    {
      target: 'body',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Welcome to GuideAI!</h3>
          <p>
            Let's take a quick tour to help you get started tracking and analyzing your AI coding
            sessions.
          </p>
          <p className="mt-2 text-sm text-base-content/70">This tour will take about 2 minutes.</p>
        </div>
      ),
      placement: 'center',
      disableBeacon: true,
    },
    {
      target: '[data-tour="providers-card"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Active Providers</h3>
          <p>
            GuideAI can track sessions from multiple AI coding assistants like Claude Code, GitHub
            Copilot, and more.
          </p>
          <p className="mt-2">Let's configure Claude Code as an example.</p>
        </div>
      ),
      placement: 'bottom',
      disableBeacon: true,
    },
    {
      target: '[data-tour="claude-code-provider"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Provider Configuration</h3>
          <p>Click Next to view Claude Code's configuration page.</p>
        </div>
      ),
      placement: 'right',
      disableBeacon: true,
    },
    {
      target: '[data-tour="home-directory"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Home Directory</h3>
          <p>
            Set the directory where Claude Code stores your session data. This is typically{' '}
            <code className="bg-base-300 px-1 rounded">~/.claude</code>.
          </p>
          <p className="mt-2 text-sm text-info">
            Tip: Press Cmd+Shift+. in the folder picker to show hidden folders.
          </p>
        </div>
      ),
      placement: 'bottom',
      disableBeacon: true,
    },
    {
      target: '[data-tour="enable-toggle"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Enable Provider</h3>
          <p>
            Toggle this on to start tracking Claude Code sessions. GuideAI will automatically watch
            for new sessions.
          </p>
        </div>
      ),
      placement: 'left',
      disableBeacon: true,
    },
    {
      target: '[data-tour="file-watching"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">File Watching</h3>
          <p>
            Once enabled, the file watcher monitors your session files in real-time. You can pause
            and resume watching at any time.
          </p>
        </div>
      ),
      placement: 'top',
      disableBeacon: true,
    },
    {
      target: '[data-tour="provider-logs-button"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Provider Logs</h3>
          <p>Each provider has a logs button where you can view detailed activity logs.</p>
          <p className="mt-2 text-sm text-base-content/70">
            Logs help troubleshoot any issues with file watching or uploads.
          </p>
        </div>
      ),
      placement: 'bottom',
      disableBeacon: true,
    },
    {
      target: '[data-tour="sessions-nav"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Sessions</h3>
          <p>Click Next to view all your tracked AI coding sessions.</p>
        </div>
      ),
      placement: 'right',
      disableBeacon: true,
    },
    {
      target: 'body',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Sessions</h3>
          <p>
            View all tracked sessions here. You can filter, search, analyze metrics, and view full
            transcripts of your AI conversations.
          </p>
        </div>
      ),
      placement: 'center',
      disableBeacon: true,
    },
    {
      target: '[data-tour="upload-queue-nav"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Upload Queue</h3>
          <p>
            Click Next to view the upload queue where you can monitor pending and failed session
            uploads.
          </p>
        </div>
      ),
      placement: 'right',
      disableBeacon: true,
    },
    {
      target: 'body',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Upload Queue</h3>
          <p>
            Here you can see pending, processing, and failed uploads. You can retry failed uploads
            or clear them from the queue.
          </p>
        </div>
      ),
      placement: 'center',
      disableBeacon: true,
    },
    {
      target: '[data-tour="settings-nav"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Settings</h3>
          <p>
            The Settings page is where you can manage all your application settings and access logs
            for the entire application and all providers.
          </p>
          <p className="mt-2 text-sm text-base-content/70">Click Next to view Settings.</p>
        </div>
      ),
      placement: 'right',
      disableBeacon: true,
    },
    {
      target: '[data-tour="ai-processing"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">AI Processing Keys</h3>
          <p>
            Add your Claude or Gemini API key here to enable local AI-powered session summaries and
            quality scores.
          </p>
          <p className="mt-2 text-sm text-base-content/70">
            Keys are stored locally and used only on your machine—never sent to GuideAI servers.
          </p>
        </div>
      ),
      placement: 'top',
      disableBeacon: true,
    },
    {
      target: '[data-tour="sync-status-card"]',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">Sync to Cloud</h3>
          <p>
            To sync your sessions to the GuideAI cloud, sign in with GitHub, Google, GitLab, or
            email.
          </p>
          <p className="mt-2">Once logged in, you can choose sync options on each provider page:</p>
          <ul className="list-disc list-inside space-y-1 mt-2 text-sm">
            <li>
              <strong>Nothing:</strong> Local only
            </li>
            <li>
              <strong>Metrics Only:</strong> Privacy mode
            </li>
            <li>
              <strong>Transcript & Metrics:</strong> Full sync
            </li>
          </ul>
        </div>
      ),
      placement: 'bottom',
      disableBeacon: true,
    },
    {
      target: 'body',
      content: (
        <div>
          <h3 className="text-lg font-bold mb-2">You're All Set!</h3>
          <p>You now know how to configure providers, sync sessions, and view your analytics.</p>
          <p className="mt-3 text-sm text-base-content/70">
            You can restart this tour anytime from the Settings page or by clicking the help button
            in the header.
          </p>
        </div>
      ),
      placement: 'center',
      disableBeacon: true,
    },
  ]

  // Handle Joyride callbacks
  const handleJoyrideCallback = useCallback(
    (data: CallBackProps) => {
      const { status, type, action, index } = data

      // Handle tour completion or skipping
      if (status === STATUS.FINISHED || status === STATUS.SKIPPED) {
        completeTour()

        // Immediately restore scrolling on all potentially affected elements
        setTimeout(() => {
          restoreScrolling()
        }, 50)

        return
      }

      // Handle close button
      if (action === ACTIONS.CLOSE) {
        stopTour()

        // Immediately restore scrolling on all potentially affected elements
        setTimeout(() => {
          restoreScrolling()
        }, 50)

        return
      }

      // Update step index on step transitions
      if (type === EVENTS.STEP_AFTER) {
        const nextIndex = action === ACTIONS.PREV ? index - 1 : index + 1

        // Handle navigation transitions - navigate first, then advance step
        // Step 2 -> 3: Navigate to provider page
        if (index === 2 && action === ACTIONS.NEXT) {
          navigate('/provider/claude-code')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Step 7 -> 8: Navigate to Sessions
        else if (index === 7 && action === ACTIONS.NEXT) {
          navigate('/sessions')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Step 9 -> 10: Navigate to Upload Queue
        else if (index === 9 && action === ACTIONS.NEXT) {
          navigate('/upload-queue')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Step 11 -> 12: Navigate to Settings for AI Processing
        else if (index === 11 && action === ACTIONS.NEXT) {
          navigate('/settings')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Step 12 -> 13: Navigate back to Dashboard for sync status
        else if (index === 12 && action === ACTIONS.NEXT) {
          navigate('/')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 3 -> 2
        else if (index === 3 && action === ACTIONS.PREV) {
          navigate('/')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 8 -> 7
        else if (index === 8 && action === ACTIONS.PREV) {
          navigate('/provider/claude-code')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 10 -> 9
        else if (index === 10 && action === ACTIONS.PREV) {
          navigate('/sessions')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 11 -> 10
        else if (index === 11 && action === ACTIONS.PREV) {
          navigate('/upload-queue')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 12 -> 11
        else if (index === 12 && action === ACTIONS.PREV) {
          navigate('/upload-queue')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Going backwards: Step 13 -> 12
        else if (index === 13 && action === ACTIONS.PREV) {
          navigate('/settings')
          setTimeout(() => setStepIndex(nextIndex), 300)
        }
        // Default: just advance the step
        else {
          setStepIndex(nextIndex)
        }
      }

      // Handle target not found - skip to next step
      if (type === EVENTS.TARGET_NOT_FOUND) {
        console.warn('Tour target not found, advancing step')
        setStepIndex(index + 1)
      }
    },
    [completeTour, stopTour, setStepIndex, navigate]
  )

  return (
    <>
      <style>{joyrideStyles}</style>
      <Joyride
        steps={steps}
        run={isTourRunning}
        stepIndex={currentStepIndex}
        continuous
        showProgress
        showSkipButton
        callback={handleJoyrideCallback}
        tooltipComponent={CustomTooltip}
        styles={{
          options: {
            overlayColor: 'rgba(0, 0, 0, 0.5)',
            zIndex: 10000,
          },
          tooltipContent: {
            padding: 0,
          },
          spotlight: {
            borderRadius: '0.5rem',
          },
        }}
        floaterProps={{
          styles: {
            arrow: {
              length: 8,
              spread: 12,
            },
            floater: {
              filter: 'drop-shadow(0 4px 6px rgba(0, 0, 0, 0.1))',
            },
          },
        }}
        locale={{
          back: 'Back',
          close: 'Close',
          last: 'Finish',
          next: 'Next',
          skip: 'Skip Tour',
        }}
        disableScrolling={false}
        scrollToFirstStep={true}
        scrollOffset={100}
        spotlightPadding={4}
      />
    </>
  )
}
