import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { type Config, ProductCard } from "../../App";
import { demoConfigs } from "../fixtures";

function ProductCardDemo({ initial }: { initial: Config }) {
  const [cfg, setCfg] = useState<Config>(initial);
  return (
    <div style={{ padding: 24 }}>
      <ProductCard
        cfg={cfg}
        onToggle={(configId, enabled) => {
          setCfg((prev) => (prev.id === configId ? { ...prev, monitorEnabled: enabled } : prev));
        }}
      />
    </div>
  );
}

const meta = {
  title: "Components/ProductCard",
  component: ProductCardDemo,
} satisfies Meta<typeof ProductCardDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Cloud: Story = {
  args: { initial: demoConfigs[0] },
};

export const MonitorOn: Story = {
  args: { initial: demoConfigs[1] },
};

export const MonitorOff: Story = {
  args: { initial: demoConfigs[2] },
};
