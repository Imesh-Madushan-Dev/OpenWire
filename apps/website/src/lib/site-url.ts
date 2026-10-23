// The production origin, for canonical URLs, Open Graph images and the sitemap.
// Set SITE_URL when deploying; on Vercel the project's production domain is used automatically.
export const siteUrl =
  process.env.SITE_URL?.replace(/\/$/, '') ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:3000');
