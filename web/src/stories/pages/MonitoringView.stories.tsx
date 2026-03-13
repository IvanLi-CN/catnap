import type { Meta, StoryObj } from "@storybook/react";
import type { ComponentProps } from "react";
import { expect, within } from "storybook/test";
import { MonitoringView } from "../../App";
import { countriesById, demoBootstrap, demoNowMs, regionsById } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

const meta = {
  title: "Pages/MonitoringView",
  component: MonitoringView,
  args: {
    bootstrap: demoBootstrap,
    countriesById: countriesById(),
    regionsById: regionsById(),
    orderBaseUrl: demoBootstrap.catalog.source.url,
    nowMs: demoNowMs,
    syncAlert: null,
    recentListed24h: demoBootstrap.catalog.configs.slice(0, 3),
    onDismissSyncAlert: () => {},
    onOpenOrder: () => {},
  },
  argTypes: {
    bootstrap: { control: false },
    countriesById: { control: false },
    regionsById: { control: false },
    onDismissSyncAlert: { control: false },
    onOpenOrder: { control: false },
  },
} satisfies Meta<typeof MonitoringView>;

export default meta;
type Story = StoryObj<typeof meta>;

function cloneBootstrap() {
  return structuredClone(demoBootstrap);
}

function buildCountryNoticeBoundaryBootstrap() {
  const bootstrap = cloneBootstrap();
  const usRegionConfig = bootstrap.catalog.configs.find((cfg) => cfg.id === "cfg-3");
  const cloudConfig = bootstrap.catalog.configs.find((cfg) => cfg.id === "cfg-cloud-1");
  const sharedNotice = "特惠年付机，IPV6入口，出口cloudflare warp";

  bootstrap.catalog.configs = [];
  if (usRegionConfig) {
    bootstrap.catalog.configs.push(
      {
        ...usRegionConfig,
        id: "cfg-us-default-monitor",
        regionId: null,
        name: "VPS • 4C/8G（美国）",
        inventory: {
          ...usRegionConfig.inventory,
          quantity: 7,
          status: "available",
        },
        monitorEnabled: true,
      },
      {
        ...usRegionConfig,
        id: "cfg-us-ca-monitor",
        monitorEnabled: true,
      },
    );
  }
  if (cloudConfig) {
    bootstrap.catalog.configs.push({
      ...cloudConfig,
      monitorEnabled: true,
    });
  }

  bootstrap.catalog.regionNotices = [
    {
      countryId: "us",
      regionId: null,
      text: sharedNotice,
    },
    {
      countryId: "us",
      regionId: "us-ca",
      text: sharedNotice,
    },
    {
      countryId: "cloud",
      regionId: null,
      text: "云产品全区共享库存，不区分可用区。",
    },
  ];
  bootstrap.monitoring.enabledConfigIds = bootstrap.catalog.configs.map((cfg) => cfg.id);
  bootstrap.monitoring.enabledPartitions = [{ countryId: "us", regionId: "us-ca" }];

  return bootstrap;
}

async function findMonitoringSection(canvasElement: HTMLElement, title: string) {
  await within(canvasElement).findByTestId("page-monitoring");
  const heading = Array.from(canvasElement.querySelectorAll(".panel-section .panel-title")).find(
    (node) => node.textContent?.trim() === title,
  );
  const section = heading?.closest(".panel-section");
  if (!(section instanceof HTMLElement)) {
    throw new Error(`Unable to find monitoring section for ${title}`);
  }
  return section;
}

function MonitoringViewDemo(args: Story["args"]) {
  const mergedArgs = {
    ...(meta.args ?? {}),
    ...(args ?? {}),
  } as ComponentProps<typeof MonitoringView>;
  return <MonitoringView {...mergedArgs} />;
}

export const Default: Story = {
  render: (args) => <MonitoringViewDemo {...args} />,
};

export const WithSyncAlert: Story = {
  args: { syncAlert: "同步失败：上游超时（demo）" },
  render: (args) => <MonitoringViewDemo {...args} />,
};

export const CountryNoticeFollowsRegionBoundary: Story = {
  args: {
    bootstrap: buildCountryNoticeBoundaryBootstrap(),
    recentListed24h: [],
  },
  render: (args) => <MonitoringViewDemo {...args} />,
  play: async ({ canvasElement }) => {
    const usSection = await findMonitoringSection(canvasElement as HTMLElement, "美国");
    const californiaSection = await findMonitoringSection(
      canvasElement as HTMLElement,
      "美国 / 加州",
    );
    const cloudSection = await findMonitoringSection(canvasElement as HTMLElement, "云服务器");
    const sharedNotice = "特惠年付机，IPV6入口，出口cloudflare warp";

    expect(within(usSection).queryByText(sharedNotice)).not.toBeInTheDocument();
    expect(within(californiaSection).getByText(sharedNotice)).toBeVisible();
    expect(within(cloudSection).getByText("云产品全区共享库存，不区分可用区。")).toBeVisible();
  },
};

export const ResponsiveAllBreakpoints: Story = {
  render: (args) => {
    const mergedArgs = {
      ...(meta.args ?? {}),
      ...(args ?? {}),
    } as ComponentProps<typeof MonitoringView>;

    return (
      <ResponsivePageStory
        route="monitoring"
        title="Catnap • 库存监控"
        subtitle="使用顶部 Viewport 选择断点进行验收"
        actions={
          <>
            <span className="pill sm">最近刷新：1 分钟前</span>
            <span className="pill sm">立即刷新</span>
          </>
        }
        renderPage={() => <MonitoringView {...mergedArgs} />}
      />
    );
  },
  play: async ({ canvasElement }) => {
    await expectResponsiveBreakpoints(canvasElement, "page-monitoring");
  },
};
