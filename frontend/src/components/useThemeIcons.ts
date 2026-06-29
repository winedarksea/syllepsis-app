import { useMemo } from 'react';
import { useStore } from '../lib/store';
import { resolveThemeIcons, resolveThemeStyle } from '../theme/themes';
import type { SignatureSlot, ThemeIcon } from '../theme/themes';
import { getIconSet } from '../theme/icons/sets';

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
