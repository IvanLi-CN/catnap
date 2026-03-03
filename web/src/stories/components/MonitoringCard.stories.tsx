import type { Meta, StoryObj } from "@storybook/react";
import { MonitoringCard } from "../../App";
import { countriesById, demoConfigs, demoNowMs } from "../fixtures";

const demoCountriesById = countriesById();
const usConfig = demoConfigs.find((c) => c.countryId === "us") ?? demoConfigs[1];
const toOrderLink = (cfg: { sourcePid?: string; sourceFid?: string; sourceGid?: string }) =>
  cfg.sourcePid
    ? {
        url: `https://lxc.lazycat.wiki/cart?action=configureproduct&pid=${cfg.sourcePid}`,
        mode: "configureproduct" as const,
      }
    : null;

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
  args: {
    cfg: demoConfigs[1],
    countriesById: demoCountriesById,
    nowMs: demoNowMs,
    orderLink: toOrderLink(demoConfigs[1]),
  },
};

export const Unknown: Story = {
  args: {
    cfg: demoConfigs[3],
    countriesById: demoCountriesById,
    nowMs: demoNowMs,
    orderLink: toOrderLink(demoConfigs[3]),
  },
};

export const UnitedStates: Story = {
  args: {
    cfg: usConfig,
    countriesById: demoCountriesById,
    nowMs: demoNowMs,
    orderLink: toOrderLink(usConfig),
  },
};
