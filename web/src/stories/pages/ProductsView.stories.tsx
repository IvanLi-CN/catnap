import type { Meta, StoryObj } from "@storybook/react";
import { useMemo, useState } from "react";
import {
  type ArchiveFilterMode,
  type BootstrapResponse,
  type Config,
  ProductsView,
} from "../../App";
import { countriesById, demoBootstrap, regionsById } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

function ProductsViewDemo() {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(demoBootstrap);
  const [archiveFilterMode, setArchiveFilterMode] = useState<ArchiveFilterMode>("active");
  const countries = useMemo(() => countriesById(), []);
  const regions = useMemo(() => regionsById(), []);

  return (
    <ProductsView
      bootstrap={bootstrap}
      countriesById={countries}
      regionsById={regions}
      orderBaseUrl={bootstrap.catalog.source.url}
      archiveFilterMode={archiveFilterMode}
      onArchiveFilterModeChange={setArchiveFilterMode}
      onArchiveDelisted={async () => {
        const now = new Date().toISOString();
        const archivedIds = bootstrap.catalog.configs
          .filter((cfg) => cfg.lifecycle.state === "delisted" && !cfg.lifecycle.cleanupAt)
          .map((cfg) => cfg.id);
        setBootstrap((prev) => ({
          ...prev,
          catalog: {
            ...prev.catalog,
            configs: prev.catalog.configs.map((cfg) =>
              archivedIds.includes(cfg.id)
                ? ({
                    ...cfg,
                    lifecycle: {
                      ...cfg.lifecycle,
                      cleanupAt: now,
                    },
                  } satisfies Config)
                : cfg,
            ),
          },
        }));
        return { archivedCount: archivedIds.length, archivedAt: now, archivedIds };
      }}
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
      onTogglePartition={(countryId, regionId, enabled) => {
        setBootstrap((prev) => ({
          ...prev,
          monitoring: {
            ...prev.monitoring,
            enabledPartitions: enabled
              ? [
                  ...prev.monitoring.enabledPartitions.filter(
                    (partition) =>
                      !(
                        partition.countryId === countryId &&
                        (partition.regionId ?? null) === regionId
                      ),
                  ),
                  { countryId, regionId },
                ]
              : prev.monitoring.enabledPartitions.filter(
                  (partition) =>
                    !(
                      partition.countryId === countryId && (partition.regionId ?? null) === regionId
                    ),
                ),
          },
        }));
      }}
      onOpenOrder={() => {}}
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

export const ResponsiveAllBreakpoints: Story = {
  render: () => (
    <ResponsivePageStory
      route="products"
      title="Catnap • 全部产品"
      subtitle="使用顶部 Viewport 选择断点进行验收"
      actions={
        <>
          <span className="pill sm">同步中</span>
          <span className="pill sm">立即刷新</span>
        </>
      }
      renderPage={() => <ProductsViewDemo />}
    />
  ),
  play: async ({ canvasElement }) => {
    await expectResponsiveBreakpoints(canvasElement, "page-products");
  },
};
