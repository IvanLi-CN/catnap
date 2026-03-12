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
      name: "VPS • 4C/8G（美国）",
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

function buildTopologyOnlyBootstrap(): BootstrapResponse {
  const bootstrap = cloneBootstrap();

  bootstrap.catalog.countries = [
    ...bootstrap.catalog.countries,
    { id: "nl", name: "荷兰" },
    { id: "sg", name: "新加坡" },
  ];
  bootstrap.catalog.regions = [
    ...bootstrap.catalog.regions,
    {
      id: "nl-ams",
      countryId: "nl",
      name: "阿姆斯特丹",
      locationName: "NL-West",
    },
  ];

  return bootstrap;
}

async function findPanelSection(canvasElement: HTMLElement, title: string) {
  await within(canvasElement).findByTestId("page-products");
  const heading = Array.from(canvasElement.querySelectorAll(".panel-section .panel-title")).find(
    (node) => node.textContent?.trim() === title,
  );
  const section = heading?.closest(".panel-section");
  if (!(section instanceof HTMLElement)) {
    throw new Error(`Unable to find panel section for ${title}`);
  }
  return section;
}

async function findProductRegionBlock(canvasElement: HTMLElement, title: string) {
  await within(canvasElement).findByTestId("page-products");
  const heading = Array.from(canvasElement.querySelectorAll(".product-region-title")).find(
    (node) => node.textContent?.trim() === title,
  );
  const block = heading?.closest(".product-region-block");
  if (!(block instanceof HTMLElement)) {
    throw new Error(`Unable to find product region block for ${title}`);
  }
  return block;
}

function ProductsViewDemo({ bootstrap: initialBootstrap = demoBootstrap }: DemoProps) {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(() =>
    cloneBootstrap(initialBootstrap),
  );
  const [archiveFilterMode, setArchiveFilterMode] = useState<ArchiveFilterMode>("active");
  const countries = useMemo(() => {
    const next = countriesById();
    for (const country of bootstrap.catalog.countries) {
      next.set(country.id, country);
    }
    return next;
  }, [bootstrap.catalog.countries]);
  const regions = useMemo(() => {
    const next = regionsById();
    for (const region of bootstrap.catalog.regions) {
      next.set(region.id, region);
    }
    return next;
  }, [bootstrap.catalog.regions]);

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
    const japanSection = await findPanelSection(canvasElement as HTMLElement, "日本");
    const usSection = await findPanelSection(canvasElement as HTMLElement, "美国");
    const tokyoBlock = await findProductRegionBlock(canvasElement as HTMLElement, "东京");
    const osakaBlock = await findProductRegionBlock(canvasElement as HTMLElement, "大阪");
    const californiaBlock = await findProductRegionBlock(canvasElement as HTMLElement, "加州");

    expect(within(japanSection).getByTestId("country-monitor-jp")).toHaveTextContent("监控：关");
    expect(within(usSection).getByTestId("country-monitor-us")).toHaveTextContent("监控：开");
    expect(within(tokyoBlock).getByTestId("region-monitor-jp-tokyo")).toHaveTextContent("监控：开");
    expect(within(osakaBlock).getByTestId("region-monitor-jp-osaka")).toHaveTextContent("监控：关");
    expect(within(californiaBlock).getByTestId("region-monitor-us-ca")).toHaveTextContent(
      "监控：关",
    );
    expect(within(usSection).queryByText("默认可用区")).not.toBeInTheDocument();
    expect(within(usSection).getByText("VPS • 4C/8G（美国）")).toBeVisible();
    expect(within(tokyoBlock).getAllByText("监控：开")).toHaveLength(2);

    await userEvent.click(within(osakaBlock).getByTestId("region-monitor-jp-osaka"));

    const enabledToggle = await within(osakaBlock).findByTestId("region-monitor-jp-osaka");
    expect(enabledToggle).toBeVisible();
    await userEvent.click(enabledToggle);
    expect(await within(osakaBlock).findByTestId("region-monitor-jp-osaka")).toHaveTextContent(
      "监控：关",
    );
    expect(within(osakaBlock).getAllByText("监控：关")).toHaveLength(3);
  },
};

export const TopologyOnlyScopes: Story = {
  args: {
    bootstrap: buildTopologyOnlyBootstrap(),
  },
  play: async ({ canvasElement }) => {
    const [countrySelect] = within(canvasElement as HTMLElement).getAllByRole("combobox");

    await userEvent.selectOptions(countrySelect, "nl");
    const netherlandsSection = await findPanelSection(canvasElement as HTMLElement, "荷兰");
    const amsterdamBlock = await findProductRegionBlock(canvasElement as HTMLElement, "阿姆斯特丹");
    expect(within(netherlandsSection).getByTestId("country-monitor-nl")).toHaveTextContent(
      "监控：关",
    );
    expect(within(amsterdamBlock).getByTestId("region-monitor-nl-ams")).toHaveTextContent(
      "监控：关",
    );
    expect(within(amsterdamBlock).getByText("当前暂无套餐。")).toBeVisible();

    await userEvent.selectOptions(countrySelect, "sg");
    const singaporeSection = await findPanelSection(canvasElement as HTMLElement, "新加坡");
    expect(within(singaporeSection).getByTestId("country-monitor-sg")).toHaveTextContent(
      "监控：关",
    );
    expect(within(singaporeSection).getByText("当前暂无可用区与套餐。")).toBeVisible();
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
