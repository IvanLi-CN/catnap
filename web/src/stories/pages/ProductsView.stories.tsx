import type { Meta, StoryObj } from "@storybook/react";
import { useMemo, useState } from "react";
import { type BootstrapResponse, type Config, ProductsView } from "../../App";
import { countriesById, demoBootstrap, regionsById } from "../fixtures";

function ProductsViewDemo() {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(demoBootstrap);
  const countries = useMemo(() => countriesById(), []);
  const regions = useMemo(() => regionsById(), []);

  return (
    <ProductsView
      bootstrap={bootstrap}
      countriesById={countries}
      regionsById={regions}
      onToggle={(configId, enabled) => {
        setBootstrap((prev) => {
          const nextConfigs = prev.catalog.configs.map((c) =>
            c.id === configId ? ({ ...c, monitorEnabled: enabled } satisfies Config) : c,
          );
          return {
            ...prev,
            catalog: { ...prev.catalog, configs: nextConfigs },
            monitoring: {
              ...prev.monitoring,
              enabledConfigIds: nextConfigs.filter((c) => c.monitorEnabled).map((c) => c.id),
            },
          };
        });
      }}
    />
  );
}

const meta = {
  title: "Pages/ProductsView",
  component: ProductsViewDemo,
} satisfies Meta<typeof ProductsViewDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
