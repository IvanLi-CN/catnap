import type { Meta, StoryObj } from "@storybook/react";
import type { ComponentProps } from "react";
import { MonitoringView } from "../../App";
import { countriesById, demoBootstrap, demoNowMs, regionsById } from "../fixtures";
import { ResponsivePageMatrix, expectResponsivePageCases } from "./responsivePageHelpers";

const meta = {
  title: "Pages/MonitoringView",
  component: MonitoringView,
  args: {
    bootstrap: demoBootstrap,
    countriesById: countriesById(),
    regionsById: regionsById(),
    orderBaseUrl: demoBootstrap.catalog.source.url,
    nowMs: demoNowMs,
    syncAlert: null,
    recentListed24h: demoBootstrap.catalog.configs.slice(0, 3),
    onDismissSyncAlert: () => {},
    onOpenOrder: () => {},
  },
  argTypes: {
    bootstrap: { control: false },
    countriesById: { control: false },
    regionsById: { control: false },
    onDismissSyncAlert: { control: false },
    onOpenOrder: { control: false },
  },
} satisfies Meta<typeof MonitoringView>;

export default meta;
type Story = StoryObj<typeof meta>;

function MonitoringViewDemo(args: Story["args"]) {
  const mergedArgs = {
    ...(meta.args ?? {}),
    ...(args ?? {}),
  } as ComponentProps<typeof MonitoringView>;
  return <MonitoringView {...mergedArgs} />;
}

export const Default: Story = {
  render: (args) => <MonitoringViewDemo {...args} />,
};

export const WithSyncAlert: Story = {
  args: { syncAlert: "同步失败：上游超时（demo）" },
  render: (args) => <MonitoringViewDemo {...args} />,
};

export const ResponsiveAllBreakpoints: Story = {
  render: (args) => {
    const mergedArgs = {
      ...(meta.args ?? {}),
      ...(args ?? {}),
    } as ComponentProps<typeof MonitoringView>;

    return (
      <ResponsivePageMatrix
        route="monitoring"
        title="Catnap • 库存监控"
        subtitle="响应式断点 DOM 验收矩阵"
        actions={
          <>
            <span className="pill sm">最近刷新：1 分钟前</span>
            <span className="pill sm">立即刷新</span>
          </>
        }
        renderPage={() => <MonitoringView {...mergedArgs} />}
      />
    );
  },
  play: async ({ canvasElement }) => {
    await expectResponsivePageCases(canvasElement, "page-monitoring");
  },
};
