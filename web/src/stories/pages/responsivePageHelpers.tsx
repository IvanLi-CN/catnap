import type { ReactNode } from "react";
import { expect, userEvent, within } from "storybook/test";
import { AppShell } from "../../ui/layout/AppShell";
import { RESPONSIVE_BREAKPOINTS, type ResponsiveBreakpoint } from "../breakpoints";

type RouteKey = "monitoring" | "products" | "settings" | "logs" | "ops";

const ROUTE_LABELS: Array<{ key: RouteKey; label: string }> = [
  { key: "monitoring", label: "库存监控" },
  { key: "products", label: "全部产品" },
  { key: "settings", label: "系统设置" },
  { key: "logs", label: "日志" },
  { key: "ops", label: "采集观测台" },
];

export function ResponsivePageMatrix({
  route,
  title,
  subtitle,
  actions,
  renderPage,
}: {
  route: RouteKey;
  title: string;
  subtitle: string;
  actions?: ReactNode;
  renderPage: (bp: ResponsiveBreakpoint) => ReactNode;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 24, overflowX: "auto" }}>
      {RESPONSIVE_BREAKPOINTS.map((bp) => (
        <section
          key={bp.id}
          data-testid={`viewport-case-${bp.id}`}
          data-viewport-width={bp.width}
          style={{ display: "flex", flexDirection: "column", gap: 10 }}
        >
          <div className="pill sm" style={{ width: "fit-content" }}>
            {`${bp.width}x${bp.height} (${bp.range})`}
          </div>
          <div
            className="responsive-viewport-preview"
            data-testid={`viewport-frame-${bp.id}`}
            style={{ width: bp.width, height: bp.height }}
          >
            <AppShell
              title={title}
              subtitle={subtitle}
              actions={actions ?? <span className="pill sm">demo</span>}
              sidebar={
                <>
                  <div className="sidebar-title">导航</div>
                  {ROUTE_LABELS.map((item) => (
                    <div
                      className={item.key === route ? "nav-item active" : "nav-item"}
                      key={item.key}
                    >
                      {item.label}
                    </div>
                  ))}
                </>
              }
            >
              {renderPage(bp)}
            </AppShell>
          </div>
        </section>
      ))}
    </div>
  );
}

export async function expectResponsivePageCases(
  canvasElement: HTMLElement,
  pageTestId: string,
): Promise<void> {
  const canvas = within(canvasElement);

  for (const bp of RESPONSIVE_BREAKPOINTS) {
    const caseEl = await canvas.findByTestId(`viewport-case-${bp.id}`);
    const caseScope = within(caseEl);

    const viewportFrame = caseScope.getByTestId(`viewport-frame-${bp.id}`) as HTMLElement;
    const appRoot = caseScope.getByTestId("app-shell-root") as HTMLElement;
    const content = caseScope.getByTestId("app-shell-content") as HTMLElement;
    const appWidth = Math.round(appRoot.clientWidth);

    expect(caseScope.getByTestId(pageTestId)).toBeVisible();
    expect(content).toBeVisible();
    expect(viewportFrame.scrollWidth <= viewportFrame.clientWidth + 1).toBe(true);
    expect(content.scrollWidth <= content.clientWidth + 1).toBe(true);
    if (Math.abs(appWidth - bp.width) > 1) {
      throw new Error(
        `viewport mismatch for ${bp.id}: expected width=${bp.width}, got app width=${appWidth}`,
      );
    }

    const navToggle = caseScope.getByTestId("app-shell-mobile-nav-toggle");
    if (bp.width <= 1023) {
      expect(navToggle).toBeVisible();
      await userEvent.click(navToggle);
      const drawer = caseScope.getByTestId("app-shell-sidebar-drawer");
      expect(drawer).toHaveClass("open");
      const drawerWidth = Math.round(drawer.getBoundingClientRect().width);
      expect(drawerWidth <= appWidth + 1).toBe(true);
      await userEvent.click(caseScope.getByTestId("app-shell-sidebar-backdrop"));
      expect(drawer).not.toHaveClass("open");
    } else {
      expect(navToggle).not.toBeVisible();
      expect(caseScope.getByTestId("app-shell-sidebar-desktop")).toBeVisible();
    }
  }
}
