import type { Meta, StoryObj } from "@storybook/react";
import { MonitoringSection } from "../../App";
import { demoConfigs, demoNowMs } from "../fixtures";

const meta = {
  title: "Components/MonitoringSection",
  component: MonitoringSection,
  render: (args) => (
    <div style={{ padding: 24 }}>
      <div className="panel">
        <MonitoringSection {...args} />
      </div>
    </div>
  ),
} satisfies Meta<typeof MonitoringSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    collapseKey: "catnap:storybook:collapse:demo",
    title: "日本 / 东京",
    items: demoConfigs.filter((c) => c.countryId === "jp").slice(0, 3),
    nowMs: demoNowMs,
  },
};
