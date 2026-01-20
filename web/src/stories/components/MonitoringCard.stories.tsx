import type { Meta, StoryObj } from "@storybook/react";
import { MonitoringCard } from "../../App";
import { demoConfigs, demoNowMs } from "../fixtures";

const meta = {
  title: "Components/MonitoringCard",
  component: MonitoringCard,
  render: (args) => (
    <div style={{ padding: 24 }}>
      <MonitoringCard {...args} />
    </div>
  ),
} satisfies Meta<typeof MonitoringCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Available: Story = {
  args: { cfg: demoConfigs[1], nowMs: demoNowMs },
};

export const Unknown: Story = {
  args: { cfg: demoConfigs[3], nowMs: demoNowMs },
};
