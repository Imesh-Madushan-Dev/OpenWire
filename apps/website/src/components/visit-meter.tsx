'use client';

import { useEffect, useState } from 'react';
import { formatBytes } from '@/lib/format';

type Tally = { bytes: number; requests: number; cached: number };

/**
 * What this visit has cost so far, counted in the browser from the page's own requests with the
 * Resource Timing API: the way OpenWire counts every app. Nothing is sent anywhere.
 */
export function VisitMeter() {
  const [tally, setTally] = useState<Tally | null>(null);

  useEffect(() => {
    performance.setResourceTimingBufferSize?.(2000);
    const total: Tally = { bytes: 0, requests: 0, cached: 0 };
    const add = (entries: PerformanceEntryList) => {
      for (const entry of entries as PerformanceResourceTiming[]) {
        total.requests++;
        if (entry.transferSize > 0) total.bytes += entry.transferSize;
        else if (entry.decodedBodySize > 0) total.cached++;
      }
      setTally({ ...total });
    };
    const observers = (['navigation', 'resource'] as const).map((type) => {
      const observer = new PerformanceObserver((list) => add(list.getEntries()));
      observer.observe({ type, buffered: true });
      return observer;
    });
    return () => observers.forEach((observer) => observer.disconnect());
  }, []);

  return (
    <section className="meter" aria-labelledby="meter-title">
      <h2 id="meter-title" className="label">
        This visit, so far
      </h2>
      <p className="meter__row" aria-live="off">
        <span className="meter__value">
          <i aria-hidden="true" />
          {tally ? formatBytes(tally.bytes) : 'Counting'} downloaded
        </span>
        <span className="meter__value">{tally ? tally.requests : '...'} requests</span>
      </p>
      <p>
        {tally && tally.cached > 0 ? `${tally.cached} came from your browser’s cache. ` : ''}
        Counted on your side from this page’s own requests, the way OpenWire counts every app. None of it is
        sent anywhere.
      </p>
    </section>
  );
}
