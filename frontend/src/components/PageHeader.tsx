// Shared topbar for app views. Lays out three competing elements consistently across screen
// sizes: an in-flow sidebar-menu button (narrow shells only), the page title, and the page's
// tools/filters. On narrow shells the tools wrap to a second row beneath the menu+title so the
// floating overlay toggle never has to cover the header.

import type { ReactNode } from 'react';
import { Icon } from './Icon';
import { useStore } from '../lib/store';
import './PageHeader.css';

/** In-flow sidebar toggle. Hidden on desktop (persistent sidebar); shown on narrow shells.
 *  Exported so views with bespoke toolbars (e.g. Graph) can place it without a full PageHeader. */
export function SidebarMenuButton() {
  const setSidebarOpen = useStore((s) => s.setSidebarOpen);
  return (
    <button
      className="page-header-menu"
      type="button"
      onClick={() => setSidebarOpen(true)}
      aria-label="Open navigation"
    >
      <Icon name="menu" size={22} />
    </button>
  );
}

interface PageHeaderProps {
  /** Page name; rendered left of the tools. Omit for views where tools are the header. */
  title?: ReactNode;
  /** Optional leading glyph/badge shown before the title. */
  icon?: ReactNode;
  /** Tools, filters, and actions for this page. */
  children?: ReactNode;
  /** Optional second row: tabs, sub-toolbars, or metadata. */
  secondary?: ReactNode;
}

export function PageHeader({ title, icon, children, secondary }: PageHeaderProps) {
  const hasTitle = icon != null || title != null;
  return (
    <header className="page-header">
      <div className="page-header-bar">
        <SidebarMenuButton />
        {hasTitle && (
          <h2 className="page-header-title">
            {icon}
            {title}
          </h2>
        )}
        {children != null && <div className="page-header-tools">{children}</div>}
      </div>
      {secondary != null && <div className="page-header-secondary">{secondary}</div>}
    </header>
  );
}
