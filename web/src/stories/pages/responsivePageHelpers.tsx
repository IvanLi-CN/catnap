import type { ReactNode } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";
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

export function ResponsivePageStory({
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
  renderPage: () => ReactNode;
}) {
  return (
    <div className="responsive-page-story" data-testid="responsive-page-story">
      <div className="responsive-page-frame" data-testid="responsive-page-frame">
        <AppShell
          title={title}
          subtitle={subtitle}
          actions={actions ?? <span className="pill sm">demo</span>}
          sidebar={
            <>
              <div className="sidebar-title">导航</div>
              {ROUTE_LABELS.map((item) => (
                <div className={item.key === route ? "nav-item active" : "nav-item"} key={item.key}>
                  {item.label}
                </div>
              ))}
            </>
          }
        >
          {renderPage()}
        </AppShell>
      </div>
    </div>
  );
}

function setViewportSize(frame: HTMLElement, breakpoint: ResponsiveBreakpoint) {
  frame.style.width = `${breakpoint.width}px`;
  frame.style.height = `${breakpoint.height}px`;
}

export async function expectResponsiveBreakpoints(
  canvasElement: HTMLElement,
  pageTestId: string,
): Promise<void> {
  const canvas = within(canvasElement);
  const frame = canvas.getByTestId("responsive-page-frame") as HTMLElement;

  for (const bp of RESPONSIVE_BREAKPOINTS) {
    setViewportSize(frame, bp);

    await waitFor(() => {
      const appRoot = canvas.getByTestId("app-shell-root") as HTMLElement;
      const appWidth = Math.round(appRoot.clientWidth);
      expect(Math.abs(appWidth - bp.width) <= 1).toBe(true);
    });

    const pageRoot = canvas.getByTestId(pageTestId);
    const content = canvas.getByTestId("app-shell-content") as HTMLElement;
    const appRoot = canvas.getByTestId("app-shell-root") as HTMLElement;
    const navToggle = canvas.getByTestId("app-shell-mobile-nav-toggle");
    const appWidth = Math.round(appRoot.clientWidth);

    expect(pageRoot).toBeVisible();
    expect(content).toBeVisible();
    expect(frame.scrollWidth <= frame.clientWidth + 1).toBe(true);
    expect(content.scrollWidth <= content.clientWidth + 1).toBe(true);

    if (bp.width <= 1023) {
      expect(navToggle).toBeVisible();
      await userEvent.click(navToggle);
      const drawer = canvas.getByTestId("app-shell-sidebar-drawer");
      expect(drawer).toHaveClass("open");
      const drawerWidth = Math.round(drawer.getBoundingClientRect().width);
      expect(drawerWidth <= appWidth + 1).toBe(true);
      await userEvent.click(canvas.getByTestId("app-shell-sidebar-backdrop"));
      expect(drawer).not.toHaveClass("open");
    } else {
      expect(navToggle).not.toBeVisible();
      expect(canvas.getByTestId("app-shell-sidebar-desktop")).toBeVisible();
    }
  }

  frame.style.removeProperty("width");
  frame.style.removeProperty("height");
}
