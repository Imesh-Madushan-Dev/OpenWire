/**
 * The active license, for views that show it. Only ever provided while the
 * license is active - the gate renders nothing else until then.
 */

import { createContext, useContext } from 'react'

import type { LicenseStatus } from '@/lib/types'

export type ActiveLicense = Extract<LicenseStatus, { state: 'active' }>

export const LicenseContext = createContext<ActiveLicense | null>(null)

export function useLicense() {
  return useContext(LicenseContext)
}
