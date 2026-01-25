import type { Meta, StoryObj } from "@storybook/react";
import { MonitoringCard } from "../../App";
import { countriesById, demoConfigs, demoNowMs } from "../fixtures";

const demoCountriesById = countriesById();
const usConfig = demoConfigs.find((c) => c.countryId === "us") ?? demoConfigs[1];

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
  args: { cfg: demoConfigs[1], countriesById: demoCountriesById, nowMs: demoNowMs },
};

export const Unknown: Story = {
  args: { cfg: demoConfigs[3], countriesById: demoCountriesById, nowMs: demoNowMs },
};

export const UnitedStates: Story = {
  args: { cfg: usConfig, countriesById: demoCountriesById, nowMs: demoNowMs },
};
