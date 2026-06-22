// Thin wrapper over Material Symbols Outlined (self-hosted ligature font).
// Centralizes icon usage so views share one straight-stroke icon vocabulary
// per the Nordic/Icelandic guide. `name` is a Material Symbols ligature.

interface IconProps {
  name: string;
  /** px size; defaults to the theme's 20px symbol sizing */
  size?: number;
  /** filled vs. outlined glyph */
  fill?: boolean;
  className?: string;
  title?: string;
}

export function Icon({ name, size, fill, className, title }: IconProps) {
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
      {name}
    </span>
  );
}
