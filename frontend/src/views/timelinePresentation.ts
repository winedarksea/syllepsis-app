import type { GraphTimelineNodeDate, TimelineDateField } from '../types';

const TIMELINE_DATE_FIELD_LABELS: Record<TimelineDateField, string> = {
  created: 'Created',
  updated: 'Updated',
  scheduled: 'Scheduled',
  completed: 'Completed',
};

export function formatTimelineNodeDate(
  timelineDate: GraphTimelineNodeDate,
  locale?: string,
): string {
  const date = new Date(timelineDate.at_ms);
  if (timelineDate.date_only) {
    return new Intl.DateTimeFormat(locale, {
      dateStyle: 'medium',
      timeZone: 'UTC',
    }).format(date);
  }
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}

export function formatTimelineDateSource(timelineDate: GraphTimelineNodeDate): string {
  const source = TIMELINE_DATE_FIELD_LABELS[timelineDate.source_field];
  return timelineDate.used_fallback ? `${source} fallback` : source;
}
