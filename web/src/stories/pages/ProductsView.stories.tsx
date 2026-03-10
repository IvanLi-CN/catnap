import type { Meta, StoryObj } from "@storybook/react";
import { useMemo, useState } from "react";
import { expect, userEvent, within } from "storybook/test";
import {
  type ArchiveFilterMode,
  type BootstrapResponse,
  type Config,
  ProductsView,
} from "../../App";
import { countriesById, demoBootstrap, regionsById } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

type DemoProps = {
  bootstrap?: BootstrapResponse;
};

function cloneBootstrap(bootstrap: BootstrapResponse = demoBootstrap): BootstrapResponse {
  return structuredClone(bootstrap);
}

function buildPartitionMonitoringBootstrap(): BootstrapResponse {
  const bootstrap = cloneBootstrap();
  const baseUsConfig = bootstrap.catalog.configs.find((cfg) => cfg.id === "cfg-3");

  bootstrap.catalog.configs = bootstrap.catalog.configs.filter((cfg) =>
    ["cfg-1", "cfg-4", "cfg-2", "cfg-2b"].includes(cfg.id),
  );

  if (baseUsConfig) {
    bootstrap.catalog.configs.push({
      ...baseUsConfig,
      id: "cfg-us-default",
      regionId: null,
      name: "VPS • 4C/8G（国家默认）",
      inventory: {
        ...baseUsConfig.inventory,
        quantity: 7,
        status: "available",
      },
      lifecycle: {
        ...baseUsConfig.lifecycle,
        state: "active",
        delistedAt: null,
        cleanupAt: null,
      },
      monitorEnabled: false,
    });
  }

  bootstrap.catalog.regionNotices = bootstrap.catalog.regionNotices.filter(
    (notice) => notice.countryId === "jp",
  );
  bootstrap.monitoring.enabledConfigIds = bootstrap.catalog.configs
    .filter((cfg) => cfg.monitorEnabled)
    .map((cfg) => cfg.id);
  bootstrap.monitoring.enabledPartitions = [
    { countryId: "jp", regionId: "jp-tokyo" },
    { countryId: "us", regionId: null },
  ];

  return bootstrap;
}

async function findPanelSection(canvasElement: HTMLElement, title: string) {
  const heading = await within(canvasElement).findByText(title);
  const section = heading.closest(".panel-section");
  if (!(section instanceof HTMLElement)) {
    throw new Error(`Unable to find panel section for ${title}`);
  }
  return section;
}

function ProductsViewDemo({ bootstrap: initialBootstrap = demoBootstrap }: DemoProps) {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(() =>
    cloneBootstrap(initialBootstrap),
  );
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

export const PartitionMonitoringFocus: Story = {
  args: {
    bootstrap: buildPartitionMonitoringBootstrap(),
  },
  play: async ({ canvasElement }) => {
    const tokyoSection = await findPanelSection(canvasElement as HTMLElement, "日本 / 东京");
    const osakaSection = await findPanelSection(canvasElement as HTMLElement, "日本 / 大阪");
    const defaultCountrySection = await findPanelSection(
      canvasElement as HTMLElement,
      "美国 / 默认",
    );

    expect(within(tokyoSection).getByRole("button", { name: "分区上新：开" })).toBeVisible();
    expect(
      within(defaultCountrySection).getByRole("button", { name: "分区上新：开" }),
    ).toBeVisible();
    expect(within(osakaSection).getByRole("button", { name: "分区上新：关" })).toBeVisible();
    expect(within(tokyoSection).getByText("监控：开")).toBeVisible();

    await userEvent.click(within(osakaSection).getByRole("button", { name: "分区上新：关" }));

    const enabledToggle = await within(osakaSection).findByRole("button", { name: "分区上新：开" });
    expect(enabledToggle).toBeVisible();
    await userEvent.click(enabledToggle);
    expect(await within(osakaSection).findByRole("button", { name: "分区上新：关" })).toBeVisible();
    expect(within(osakaSection).getAllByText("监控：关")).toHaveLength(2);
  },
};

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
