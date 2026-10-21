/** How each kind of device is drawn and named. */

import type { IconSvgElement } from '@hugeicons/react'
import {
  CctvIcon,
  GameController03Icon,
  HardDriveIcon,
  HelpCircleIcon,
  Home09Icon,
  LaptopIcon,
  PrinterIcon,
  Router01Icon,
  SmartPhone01Icon,
  SmartWatch01Icon,
  Speaker01Icon,
  Tablet01Icon,
  Tv01Icon,
  Wifi01Icon,
} from '@hugeicons/core-free-icons'

import type { Device, DeviceKind } from '@/lib/types'

/** `label` names the kind on its own; `noun` follows a maker ("Roku TV"). */
export const KINDS: Record<DeviceKind, { label: string; noun: string; icon: IconSvgElement }> = {
  router: { label: 'Router', noun: 'router', icon: Router01Icon },
  computer: { label: 'Computer', noun: 'computer', icon: LaptopIcon },
  phone: { label: 'Phone', noun: 'phone', icon: SmartPhone01Icon },
  tablet: { label: 'Tablet', noun: 'tablet', icon: Tablet01Icon },
  tv: { label: 'TV or streamer', noun: 'TV', icon: Tv01Icon },
  speaker: { label: 'Speaker', noun: 'speaker', icon: Speaker01Icon },
  console: { label: 'Game console', noun: 'console', icon: GameController03Icon },
  printer: { label: 'Printer', noun: 'printer', icon: PrinterIcon },
  camera: { label: 'Camera', noun: 'camera', icon: CctvIcon },
  storage: { label: 'Storage', noun: 'storage', icon: HardDriveIcon },
  smartHome: { label: 'Smart home', noun: 'smart device', icon: Home09Icon },
  wearable: { label: 'Wearable', noun: 'wearable', icon: SmartWatch01Icon },
  network: { label: 'Access point', noun: 'access point', icon: Wifi01Icon },
  unknown: { label: 'Unknown', noun: 'device', icon: HelpCircleIcon },
}

/** The picker's order: commonest first, the catch-all last. */
export const KIND_ORDER: DeviceKind[] = [
  'phone',
  'computer',
  'tablet',
  'tv',
  'speaker',
  'console',
  'printer',
  'camera',
  'storage',
  'smartHome',
  'wearable',
  'network',
  'router',
  'unknown',
]

/**
 * What to call a device in the list. Its own name wins; without one, the
 * best description the evidence allows, so no row is a bare address.
 */
export function deviceTitle(device: Device): string {
  if (device.name) return device.name
  const { noun } = KINDS[device.kind]
  if (device.vendor) return `${device.vendor} ${noun}`
  if (device.privateAddress) return `Private ${noun}`
  return device.role === 'router' ? 'Router' : 'Unknown device'
}
