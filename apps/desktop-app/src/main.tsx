import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { applyChartPalette, readChartPalette } from './lib/chart-palette'
import { applyTheme, followSystemTheme, readTheme } from './lib/theme'

// Before the first render, so the window never flashes the wrong palette.
applyTheme(readTheme())
applyChartPalette(readChartPalette())
followSystemTheme()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
