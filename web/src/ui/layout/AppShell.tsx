import type { ReactNode } from "react";

export type AppShellProps = {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  sidebar?: ReactNode;
  children: ReactNode;
};

export function AppShell({ title, subtitle, actions, sidebar, children }: AppShellProps) {
  return (
    <div className="app">
      <div className="shell">
        <header className="topbar">
          <div className="topbar-left">
            <div className="topbar-title">{title}</div>
            {subtitle ? <div className="topbar-subtitle">{subtitle}</div> : null}
          </div>
          <div className="topbar-right">{actions}</div>
        </header>

        <div className="layout">
          <nav className="sidebar">{sidebar}</nav>
          <main className="content">{children}</main>
        </div>
      </div>
    </div>
  );
}
