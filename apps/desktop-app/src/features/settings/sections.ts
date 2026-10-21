/** The settings page's sections, in sidebar order. */

import type { IconSvgElement } from '@hugeicons/react'
import {
  ComputerSettingsIcon,
  InformationCircleIcon,
  PaintBoardIcon,
  Route01Icon,
  ServerStack01Icon,
  Shield01Icon,
  Stethoscope02Icon,
} from '@hugeicons/core-free-icons'

export type SectionId =
  | 'protection'
  | 'routing'
  | 'servers'
  | 'general'
  | 'appearance'
  | 'diagnostics'
  | 'about'

export interface Section {
  id: SectionId
  label: string
  description: string
  icon: IconSvgElement
}

/** Grouped: what the tunnel does, then how the app behaves. */
export const SECTION_GROUPS: readonly (readonly Section[])[] = [
  [
    {
      id: 'protection',
      label: 'Protection',
      description: 'What happens when the tunnel drops, and what stays off it.',
      icon: Shield01Icon,
    },
    {
      id: 'routing',
      label: 'Routing',
      description: 'Which traffic goes through Maizy, and where apps connect.',
      icon: Route01Icon,
    },
    {
      id: 'servers',
      label: 'Servers',
      description: 'Add servers you run yourself from a share link.',
      icon: ServerStack01Icon,
    },
  ],
  [
    {
      id: 'general',
      label: 'General',
      description: 'How Maizy starts and behaves on this machine.',
      icon: ComputerSettingsIcon,
    },
    {
      id: 'appearance',
      label: 'Appearance',
      description:
        'Pick a theme. Whatever the palette, the signal colour still means one thing - you are covered.',
      icon: PaintBoardIcon,
    },
    {
      id: 'diagnostics',
      label: 'Diagnostics',
      description: 'What Maizy is running right now, and a fix for a stuck network.',
      icon: Stethoscope02Icon,
    },
    {
      id: 'about',
      label: 'About',
      description: 'Who builds Maizy, and how your data is handled.',
      icon: InformationCircleIcon,
    },
  ],
]

export const SECTIONS: readonly Section[] = SECTION_GROUPS.flat()
