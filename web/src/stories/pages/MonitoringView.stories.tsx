import type { Meta, StoryObj } from "@storybook/react";
import { MonitoringView } from "../../App";
import { countriesById, demoBootstrap, demoNowMs, regionsById } from "../fixtures";

const meta = {
  title: "Pages/MonitoringView",
  component: MonitoringView,
  args: {
    bootstrap: demoBootstrap,
    countriesById: countriesById(),
    regionsById: regionsById(),
    nowMs: demoNowMs,
    syncAlert: null,
    recentListed24h: demoBootstrap.catalog.configs.slice(0, 3),
    onDismissSyncAlert: () => {},
  },
  argTypes: {
    bootstrap: { control: false },
    countriesById: { control: false },
    regionsById: { control: false },
    onDismissSyncAlert: { control: false },
  },
} satisfies Meta<typeof MonitoringView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithSyncAlert: Story = {
  args: { syncAlert: "同步失败：上游超时（demo）" },
};
