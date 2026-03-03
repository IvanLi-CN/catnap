import type { Meta, StoryObj } from "@storybook/react";
import type { ComponentProps } from "react";
import { MonitoringView } from "../../App";
import { countriesById, demoBootstrap, demoNowMs, regionsById } from "../fixtures";

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
