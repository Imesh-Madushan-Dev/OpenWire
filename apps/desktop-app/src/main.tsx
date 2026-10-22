import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { applyChartPalette, readChartPalette } from './lib/chart-palette'
import { applyAppearance, followSystemMode, readMode, readTheme } from './lib/theme'

// Before the first render, so the window never flashes the wrong palette.
applyAppearance(readTheme(), readMode())
applyChartPalette(readChartPalette())
followSystemMode()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
