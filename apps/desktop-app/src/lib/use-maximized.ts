/** Whether the window is maximised, kept in step with OS resizes and snaps. */

import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

export function useMaximized() {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    const win = getCurrentWindow()
    let disposed = false
    let unlisten: (() => void) | undefined

    const sync = () => {
      win
        .isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value)
        })
        .catch(() => {})
    }

    sync()
    // Maximise, restore and Aero Snap all arrive as resizes.
    void win.onResized(sync).then((stop) => {
      if (disposed) stop()
      else unlisten = stop
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  return maximized
}
