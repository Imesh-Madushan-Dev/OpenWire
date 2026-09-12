/**
 * Owns the window until a valid license is active.
 *
 * This is the user-facing half only. The backend refuses to bring a tunnel up
 * without a license regardless of what the webview renders, so stepping around
 * this screen gets a UI that cannot connect.
 */

import { useEffect, useState, type ReactNode } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  Copy01Icon,
  Key01Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import { motion } from 'motion/react'

import { TitleBar } from '@/components/title-bar'
import { Button } from '@/components/ui/button'
import { api, VpnError } from '@/lib/ipc'
import { staggerChild, staggerParent } from '@/lib/motion'
import type { LicenseStatus } from '@/lib/types'

import { LicenseContext, type ActiveLicense } from './license-context'

/** Re-checked on this cadence, so a license that lapses returns to this screen. */
const RECHECK_MS = 60_000

type InactiveLicense = Exclude<LicenseStatus, ActiveLicense>

export function LicenseGate({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<LicenseStatus | null>(null)
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    let cancelled = false

    const check = () =>
      api
        .licenseStatus()
        .then((next) => {
          if (cancelled) return
          setStatus(next)
          setFailed(false)
        })
        .catch(() => {
          if (!cancelled) setFailed(true)
        })

    void check()
    const timer = window.setInterval(() => void check(), RECHECK_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [attempt])

  if (status?.state === 'active') {
    return <LicenseContext.Provider value={status}>{children}</LicenseContext.Provider>
  }

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-white/8 bg-background">
      <TitleBar onFeedback={() => undefined} />

      <main className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-8 py-6">
        {status ? (
          <Activation status={status} onActivated={setStatus} />
        ) : failed ? (
          <div className="flex flex-col items-center gap-3 text-center">
            <p className="text-sm text-muted-foreground">Couldn't check your license.</p>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setFailed(false)
                setAttempt((value) => value + 1)
              }}
            >
              Try again
            </Button>
          </div>
        ) : null}
      </main>
    </div>
  )
}

function Activation({
  status,
  onActivated,
}: {
  status: InactiveLicense
  onActivated: (status: LicenseStatus) => void
}) {
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (status.state === 'unsupported') {
    return (
      <p className="max-w-sm text-center text-sm text-muted-foreground">
        Licensing isn't available on this platform yet.
      </p>
    )
  }

  const activate = async () => {
    setBusy(true)
    setError(null)
    try {
      onActivated(await api.activateLicense(key))
    } catch (cause) {
      setError(cause instanceof VpnError ? cause.message : 'Could not activate this key.')
    } finally {
      setBusy(false)
    }
  }

  // Why the stored key stopped working, when there is one.
  const banner =
    status.state === 'expired'
      ? `The license for ${status.name} expired on ${formatDate(status.expiresAt)}. Paste a renewed key to continue.`
      : status.state === 'invalid'
        ? status.reason
        : null

  return (
    <motion.div
      variants={staggerParent}
      initial="initial"
      animate="animate"
      className="flex w-full max-w-md flex-col gap-5"
    >
      <motion.header variants={staggerChild} className="flex flex-col items-center gap-3 text-center">
        <div className="grid size-12 place-items-center rounded-2xl bg-primary/12">
          <span aria-hidden className="text-lg font-bold text-primary">
            M
          </span>
        </div>
        <div>
          <h1 className="font-heading text-xl font-semibold">Activate Maizy VPN</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            This copy needs a license key issued for this computer.
          </p>
        </div>
      </motion.header>

      {banner && (
        <motion.p
          variants={staggerChild}
          role="status"
          className="flex items-start gap-2 rounded-2xl bg-warning/10 px-4 py-3 text-xs leading-relaxed text-warning"
        >
          <HugeiconsIcon icon={AlertCircleIcon} size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
          {banner}
        </motion.p>
      )}

      <MachineId value={status.machineId} />

      <motion.form
        variants={staggerChild}
        onSubmit={(event) => {
          event.preventDefault()
          void activate()
        }}
        className="panel flex flex-col gap-3 rounded-2xl p-4"
      >
        <label
          htmlFor="license-key"
          className="text-xs font-semibold tracking-wide text-muted-foreground uppercase"
        >
          Step 2 &middot; Paste your license key
        </label>

        <textarea
          id="license-key"
          value={key}
          onChange={(event) => {
            setKey(event.target.value)
            setError(null)
          }}
          rows={4}
          spellCheck={false}
          autoComplete="off"
          placeholder="MZY1..."
          aria-invalid={error ? true : undefined}
          data-selectable
          className="w-full resize-none rounded-2xl border border-input bg-input/30 px-3 py-2.5 font-mono text-xs leading-relaxed break-all outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive"
        />

        {error && (
          <p role="alert" className="flex items-start gap-1.5 text-xs leading-relaxed text-destructive">
            <HugeiconsIcon icon={AlertCircleIcon} size={14} strokeWidth={2} className="mt-0.5 shrink-0" />
            {error}
          </p>
        )}

        <Button type="submit" disabled={busy || key.trim().length === 0}>
          <HugeiconsIcon icon={Key01Icon} strokeWidth={2} />
          {busy ? 'Activating...' : 'Activate'}
        </Button>
      </motion.form>
    </motion.div>
  )
}

function MachineId({ value }: { value: string }) {
  const [copy, setCopy] = useState<'idle' | 'copied' | 'failed'>('idle')

  // The confirmation clears itself; setState happens in the timer, not the effect.
  useEffect(() => {
    if (copy === 'idle') return
    const timer = window.setTimeout(() => setCopy('idle'), 1800)
    return () => window.clearTimeout(timer)
  }, [copy])

  const copyId = async () => {
    try {
      await navigator.clipboard.writeText(value)
      setCopy('copied')
    } catch {
      setCopy('failed')
    }
  }

  return (
    <motion.section variants={staggerChild} className="panel flex flex-col gap-3 rounded-2xl p-4">
      <h2 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
        Step 1 &middot; Send us your Machine ID
      </h2>

      <div className="flex items-center gap-2">
        <code
          data-selectable
          className="numeric min-w-0 flex-1 truncate rounded-xl bg-white/5 px-3 py-2.5 font-mono text-sm tracking-wider"
        >
          {value}
        </code>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void copyId()}
          aria-label="Copy Machine ID"
        >
          <HugeiconsIcon icon={copy === 'copied' ? Tick02Icon : Copy01Icon} strokeWidth={2} />
          {copy === 'copied' ? 'Copied' : 'Copy'}
        </Button>
      </div>

      <p className="text-xs leading-relaxed text-muted-foreground" aria-live="polite">
        {copy === 'failed'
          ? "Couldn't copy automatically. Select the ID above and copy it."
          : 'This ID identifies this computer and nothing else. Your key will only work here.'}
      </p>
    </motion.section>
  )
}

function formatDate(seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  })
}
