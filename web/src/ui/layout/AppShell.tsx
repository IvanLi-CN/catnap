import { type ReactNode, useEffect, useId, useRef, useState } from "react";

export type AppShellProps = {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  sidebar?: ReactNode;
  contentClassName?: string;
  scrollInnerClassName?: string;
  children: ReactNode;
};

export function AppShell({
  title,
  subtitle,
  actions,
  sidebar,
  contentClassName,
  scrollInnerClassName,
  children,
}: AppShellProps) {
  const contentRef = useRef<HTMLElement | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const scrollInnerRef = useRef<HTMLDivElement | null>(null);
  const mobileDrawerRef = useRef<HTMLElement | null>(null);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const mobileDrawerId = useId();
  const hasSidebar = Boolean(sidebar);

  useEffect(() => {
    const contentEl = contentRef.current;
    const scrollEl = scrollRef.current;
    const scrollInnerEl = scrollInnerRef.current;
    if (!contentEl || !scrollEl || !scrollInnerEl) return;

    const updateFades = () => {
      const { scrollTop, scrollHeight, clientHeight } = scrollEl;
      const hasTop = scrollTop > 1;
      const hasBottom = scrollTop + clientHeight < scrollHeight - 1;
      contentEl.dataset.fadeTop = hasTop ? "1" : "0";
      contentEl.dataset.fadeBottom = hasBottom ? "1" : "0";
    };

    // Initial paint might occur before async data renders into the scroll container.
    // Schedule a few passes so the bottom fade appears even before the first user scroll.
    updateFades();
    requestAnimationFrame(() => updateFades());
    requestAnimationFrame(() => requestAnimationFrame(() => updateFades()));

    scrollEl.addEventListener("scroll", updateFades, { passive: true });
    window.addEventListener("resize", updateFades, { passive: true });

    const ResizeObserverCtor = (globalThis as unknown as { ResizeObserver?: typeof ResizeObserver })
      .ResizeObserver;
    const ro = ResizeObserverCtor ? new ResizeObserverCtor(() => updateFades()) : null;
    ro?.observe(scrollEl);
    ro?.observe(scrollInnerEl);

    return () => {
      scrollEl.removeEventListener("scroll", updateFades);
      window.removeEventListener("resize", updateFades);
      ro?.disconnect();
    };
  }, []);

  useEffect(() => {
    if (!mobileNavOpen) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileNavOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mobileNavOpen]);

  useEffect(() => {
    const media = window.matchMedia("(min-width: 1024px)");
    const closeOnDesktop = (event: MediaQueryListEvent) => {
      if (event.matches) setMobileNavOpen(false);
    };

    if (media.matches) setMobileNavOpen(false);
    media.addEventListener("change", closeOnDesktop);
    return () => media.removeEventListener("change", closeOnDesktop);
  }, []);

  useEffect(() => {
    const drawerEl = mobileDrawerRef.current;
    if (!drawerEl) return;

    const closeOnNavClick = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target?.closest("a,button")) return;
      setMobileNavOpen(false);
    };
    drawerEl.addEventListener("click", closeOnNavClick);
    return () => drawerEl.removeEventListener("click", closeOnNavClick);
  }, []);

  return (
    <div className={`app${mobileNavOpen ? " mobile-nav-open" : ""}`} data-testid="app-shell-root">
      <div className="shell" data-testid="app-shell-frame">
        <header className="topbar" data-testid="app-shell-topbar">
          {hasSidebar ? (
            <button
              type="button"
              className="topbar-menu-btn btn"
              aria-label={mobileNavOpen ? "关闭导航菜单" : "打开导航菜单"}
              aria-controls={mobileDrawerId}
              aria-expanded={mobileNavOpen}
              onClick={() => setMobileNavOpen((prev) => !prev)}
              data-testid="app-shell-mobile-nav-toggle"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                <path
                  fill="currentColor"
                  d={
                    mobileNavOpen
                      ? "M18.3 5.71L12 12l6.3 6.29l-1.42 1.42L10.59 13.4L4.29 19.7l-1.41-1.41L9.17 12L2.88 5.7l1.41-1.41l6.3 6.29l6.29-6.29z"
                      : "M4 7h16v2H4zm0 5h16v2H4zm0 5h16v2H4z"
                  }
                />
              </svg>
            </button>
          ) : null}
          <div className="topbar-left">
            <div className="topbar-title">{title}</div>
            {subtitle ? <div className="topbar-subtitle">{subtitle}</div> : null}
          </div>
          <div className="topbar-right" data-testid="app-shell-actions">
            {actions}
          </div>
        </header>

        <div className="layout">
          <nav className="sidebar" data-testid="app-shell-sidebar-desktop">
            {sidebar}
          </nav>
          <main
            className={`content${contentClassName ? ` ${contentClassName}` : ""}`}
            ref={contentRef}
            data-testid="app-shell-content"
          >
            <div className="content-scroll" ref={scrollRef}>
              <div
                className={`content-scroll-inner${scrollInnerClassName ? ` ${scrollInnerClassName}` : ""}`}
                ref={scrollInnerRef}
              >
                {children}
              </div>
            </div>
          </main>
        </div>
      </div>

      {hasSidebar ? (
        <div
          className={`mobile-nav-backdrop${mobileNavOpen ? " open" : ""}`}
          aria-hidden={!mobileNavOpen}
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setMobileNavOpen(false);
          }}
          data-testid="app-shell-sidebar-backdrop"
        >
          <nav
            id={mobileDrawerId}
            ref={mobileDrawerRef}
            className={`mobile-nav-drawer${mobileNavOpen ? " open" : ""}`}
            aria-label="移动端导航"
            data-testid="app-shell-sidebar-drawer"
          >
            {sidebar}
          </nav>
        </div>
      ) : null}
    </div>
  );
}
