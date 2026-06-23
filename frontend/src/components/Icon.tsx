// Thin wrapper over Material Symbols Outlined (self-hosted ligature font).
// Centralizes icon usage so views share one straight-stroke icon vocabulary
// per the Nordic/Icelandic guide. `name` is a Material Symbols ligature.
//
// When a `slot` is provided and the active theme's icon set resolves path-data for that slot,
// a controlled <svg><path> is rendered instead — inheriting currentColor, no markup risk.

import { useMemo } from 'react';
import { useStore } from '../lib/store';
import { resolveThemeIcons, resolveThemeStyle, SLOT_FALLBACK, type SignatureSlot, type ThemeIcon } from '../theme/themes';
import { getIconSet } from '../theme/icons/sets';

interface IconProps {
  name: string;
  /** px size; defaults to the theme's 20px symbol sizing */
  size?: number;
  /** filled vs. outlined glyph */
  fill?: boolean;
  className?: string;
  title?: string;
  /** Signature slot: resolves theme icon-set override before falling back to `name` ligature. */
  slot?: SignatureSlot;
}

/** Returns the active theme's merged icon overrides (set + per-slot). Memoized by themeId. */
export function useThemeIcons(): Partial<Record<SignatureSlot, ThemeIcon>> {
  const { themeId, customThemes } = useStore();
  return useMemo(() => {
    const style = resolveThemeStyle(themeId, customThemes);
    const set = getIconSet(style.iconSet);
    const overrides = resolveThemeIcons(themeId, customThemes);
    return { ...set, ...overrides };
  }, [themeId, customThemes]);
}

/** Returns the active theme's resolved ThemeStyle. Memoized by themeId. */
export function useThemeStyle() {
  const { themeId, customThemes } = useStore();
  return useMemo(() => resolveThemeStyle(themeId, customThemes), [themeId, customThemes]);
}

function SvgIcon({ icon, size, className, title }: { icon: ThemeIcon; size?: number; className?: string; title?: string }) {
  const paths = Array.isArray(icon.path) ? icon.path : [icon.path];
  const px = size ?? 20;
  return (
    <svg
      viewBox={icon.viewBox ?? '0 0 24 24'}
      width={px}
      height={px}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
      aria-label={title}
      style={{ display: 'inline-block', flexShrink: 0 }}
    >
      {title && <title>{title}</title>}
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}

export function Icon({ name, size, fill, className, title, slot }: IconProps) {
  const icons = useThemeIcons();
  const resolved = slot ? icons[slot] : undefined;

  if (resolved) {
    return <SvgIcon icon={resolved} size={size} className={className} title={title} />;
  }

  // Fallback to Material Symbols ligature. When a slot is given but the set has no override,
  // use the slot's canonical fallback name instead of requiring callers to duplicate it.
  const ligature = slot ? (SLOT_FALLBACK[slot] ?? name) : name;

  return (
    <span
      className={`material-symbols-outlined${className ? ` ${className}` : ''}`}
      style={{
        fontSize: size,
        fontVariationSettings: fill ? "'FILL' 1, 'wght' 400, 'GRAD' 0, 'opsz' 20" : undefined,
      }}
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
      aria-label={title}
      title={title}
    >
      {ligature}
    </span>
  );
}
