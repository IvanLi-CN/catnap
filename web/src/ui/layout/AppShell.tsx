import { type ReactNode, useEffect, useRef } from "react";

export type AppShellProps = {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  sidebar?: ReactNode;
  children: ReactNode;
};

export function AppShell({ title, subtitle, actions, sidebar, children }: AppShellProps) {
  const contentRef = useRef<HTMLElement | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const scrollInnerRef = useRef<HTMLDivElement | null>(null);

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

    const ro = new ResizeObserver(() => updateFades());
    ro.observe(scrollEl);
    ro.observe(scrollInnerEl);

    return () => {
      scrollEl.removeEventListener("scroll", updateFades);
      window.removeEventListener("resize", updateFades);
      ro.disconnect();
    };
  }, []);

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
          <main className="content" ref={contentRef}>
            <div className="content-scroll" ref={scrollRef}>
              <div className="content-scroll-inner" ref={scrollInnerRef}>
                {children}
              </div>
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}
