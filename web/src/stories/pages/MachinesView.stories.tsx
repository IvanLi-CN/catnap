import type { Meta, StoryObj } from "@storybook/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";
import {
  type BootstrapResponse,
  type LazycatAccountView,
  type LazycatMachineDetailLoginBridgeResponse,
  type LazycatMachineView,
  type LazycatMachinesResponse,
  MachinesView,
} from "../../App";
import { demoBootstrap } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

type DemoProps = {
  bootstrap?: BootstrapResponse;
  items?: LazycatMachineView[];
  fetchDelayMs?: number;
  syncDelayMs?: number;
  syncNextAccount?: LazycatAccountView;
  syncNextItems?: LazycatMachineView[];
  syncError?: string | null;
  detailBridgeDelayMs?: number;
  detailBridgePrimeDelayMs?: number;
  detailBridgeRedirectAfterMs?: number;
  detailBridgeError?: string | null;
};

function cloneBootstrap(bootstrap: BootstrapResponse = demoBootstrap): BootstrapResponse {
  return structuredClone(bootstrap);
}

function cloneMachines(items: LazycatMachineView[]): LazycatMachineView[] {
  return structuredClone(items);
}

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

function buildLazycatMachineDetailUrl(serviceId: number) {
  return `https://lxc.lazycat.wiki/servicedetail?id=${serviceId}`;
}

function buildLazycatMachineActionUrl(serviceId: number, action: "panel" | "vnc-console") {
  return `${window.location.origin}/api/lazycat/machines/${serviceId}/${action}`;
}

function buildLazycatMachineDetailLoginBridgeApiUrl(serviceId: number) {
  return `${window.location.origin}/api/lazycat/machines/${serviceId}/detail-login-bridge`;
}

function buildLazycatMachineDetailPopupName(serviceId: number) {
  return `lazycat-detail-${serviceId}-`;
}

function buildTrafficHistory(points: Array<[sampledAt: string, usedGb: number, limitGb: number]>) {
  return points.map(([sampledAt, usedGb, limitGb]) => ({
    sampledAt,
    usedGb,
    limitGb,
  }));
}

const readyAccount: LazycatAccountView = {
  connected: true,
  email: "demo@lazycat.example",
  state: "ready",
  machineCount: 3,
  lastSiteSyncAt: "2026-03-20T00:46:13Z",
  lastPanelSyncAt: "2026-03-20T00:47:07Z",
  lastError: null,
};

const syncingAccount: LazycatAccountView = {
  ...readyAccount,
  state: "syncing",
};

const partialFailureAccount: LazycatAccountView = {
  ...readyAccount,
  lastError: "部分面板同步失败（2/3），已保留最近一次成功缓存",
};

const disconnectedAccount: LazycatAccountView = {
  connected: false,
  state: "disconnected",
  machineCount: 0,
  lastSiteSyncAt: null,
  lastPanelSyncAt: null,
  lastError: null,
};

const healthyMachines: LazycatMachineView[] = [
  {
    serviceId: 2312,
    serviceName: "港湾 Transit Basic",
    serviceCode: "srvA7K2M4N8P1Q",
    status: "Active",
    os: "Debian 12",
    primaryAddress: "198.51.100.24",
    extraAddresses: ["2001:db8:10::24"],
    expiresAt: "2026-04-13T11:34:27Z",
    billingCycle: "monthly",
    renewPrice: "¥8.00元/月付",
    firstPrice: "¥8.00元/月付",
    panelKind: "container",
    panelUrl: "https://edge-node-24.example.net:8443/container/dashboard?hash=8d1f0c27b4a9e3f2",
    detailUrl: buildLazycatMachineDetailUrl(2312),
    traffic: {
      usedGb: 61.53,
      limitGb: 750,
      resetDay: 13,
      cycleStartAt: "2026-03-13T00:00:00Z",
      cycleEndAt: "2026-04-13T00:00:00Z",
      history: buildTrafficHistory([
        ["2026-03-13T00:00:00Z", 0, 750],
        ["2026-03-14T00:00:00Z", 8.2, 750],
        ["2026-03-16T00:00:00Z", 18.6, 750],
        ["2026-03-18T00:00:00Z", 39.1, 750],
        ["2026-03-20T00:00:00Z", 61.53, 750],
      ]),
      lastResetAt: "2026-03-13T00:00:00Z",
      display: "GB",
    },
    portMappings: [
      {
        family: "v4",
        publicIp: "198.51.100.24",
        publicPort: 443,
        privateIp: "172.17.0.2",
        privatePort: 8443,
        protocol: "tcp",
        status: "enabled",
        description: "Web 面板",
      },
      {
        family: "v6",
        publicIp: "2001:db8:10::24",
        publicPort: 443,
        privateIp: "fd00::2",
        privatePort: 8443,
        protocol: "tcp",
        status: "enabled",
        description: "Web 面板 IPv6",
      },
      {
        family: "v4",
        publicIp: "198.51.100.24",
        publicPort: 5901,
        privateIp: "172.17.0.2",
        privatePort: 5901,
        protocol: "tcp",
        status: "enabled",
        description: "VNC Console",
      },
    ],
    lastSiteSyncAt: "2026-03-20T00:46:13Z",
    lastPanelSyncAt: "2026-03-20T00:47:07Z",
    detailState: "ready",
    detailError: null,
  },
  {
    serviceId: 2313,
    serviceName: "都会 Fiber Mini",
    serviceCode: "srvB6R9T2L5W3X",
    status: "Active",
    os: "Ubuntu 24.04",
    primaryAddress: "vm-bravo.edge.example.net",
    extraAddresses: [],
    expiresAt: "2026-04-11T12:24:42Z",
    billingCycle: "monthly",
    renewPrice: "¥9.34元/月付",
    firstPrice: "¥9.34元/月付",
    panelKind: "container",
    panelUrl: null,
    detailUrl: buildLazycatMachineDetailUrl(2313),
    traffic: {
      usedGb: 702,
      limitGb: 800,
      resetDay: 11,
      cycleStartAt: "2026-03-11T00:00:00Z",
      cycleEndAt: "2026-04-11T00:00:00Z",
      history: buildTrafficHistory([
        ["2026-03-11T00:00:00Z", 0, 800],
        ["2026-03-13T00:00:00Z", 72, 800],
        ["2026-03-15T00:00:00Z", 156, 800],
        ["2026-03-17T00:00:00Z", 310, 800],
        ["2026-03-19T00:00:00Z", 522, 800],
        ["2026-03-20T00:00:00Z", 702, 800],
      ]),
      lastResetAt: "2026-03-11T00:00:00Z",
      display: "GB",
    },
    portMappings: [
      {
        family: "v4",
        publicIp: "vm-bravo.edge.example.net",
        publicPort: 22,
        privateIp: "172.17.0.9",
        privatePort: 22,
        protocol: "tcp",
        status: "enabled",
        description: "SSH",
      },
    ],
    lastSiteSyncAt: "2026-03-20T00:46:13Z",
    lastPanelSyncAt: "2026-03-20T00:47:07Z",
    detailState: "ready",
    detailError: null,
  },
  {
    serviceId: 2314,
    serviceName: "Apex Compute Lite",
    serviceCode: "srvC5H8J1D4Z6M",
    status: "Active",
    os: "Debian 12",
    primaryAddress: "203.0.113.88",
    extraAddresses: [],
    expiresAt: "2026-06-20T05:43:33Z",
    billingCycle: "monthly",
    renewPrice: "¥0.00元/月付",
    firstPrice: "¥0.00元/月付",
    panelKind: "container",
    panelUrl: null,
    detailUrl: buildLazycatMachineDetailUrl(2314),
    traffic: {
      usedGb: 0,
      limitGb: 700,
      resetDay: 20,
      cycleStartAt: "2026-03-20T00:00:00Z",
      cycleEndAt: "2026-04-20T00:00:00Z",
      history: buildTrafficHistory([["2026-03-20T00:00:00Z", 0, 700]]),
      lastResetAt: "2026-03-20T00:00:00Z",
      display: "GB",
    },
    portMappings: [],
    lastSiteSyncAt: "2026-03-20T00:46:13Z",
    lastPanelSyncAt: "2026-03-20T00:47:07Z",
    detailState: "ready",
    detailError: null,
  },
];

const degradedMachines: LazycatMachineView[] = [
  healthyMachines[0],
  {
    serviceId: 2315,
    serviceName: "北湾 NAT 02",
    serviceCode: "srvD4F7K0Q9V2N",
    status: "Active",
    os: "Debian 12",
    primaryAddress: "192.0.2.45",
    extraAddresses: ["2001:db8:20::45"],
    expiresAt: "2026-02-18T09:36:22Z",
    billingCycle: "monthly",
    renewPrice: "¥1.50元/月付",
    firstPrice: "¥1.50元/月付",
    panelKind: null,
    panelUrl: null,
    detailUrl: buildLazycatMachineDetailUrl(2315),
    traffic: null,
    portMappings: [
      {
        family: "nat",
        publicIp: "192.0.2.45",
        publicPort: 28080,
        privateIp: "10.0.0.7",
        privatePort: 8080,
        protocol: "tcp",
        status: "cached",
        description: "最近成功缓存",
      },
    ],
    lastSiteSyncAt: "2026-03-20T00:46:13Z",
    lastPanelSyncAt: "2026-03-19T18:10:00Z",
    detailState: "stale",
    detailError: "面板 TLS 校验失败，已保留最近一次成功缓存。",
  },
  {
    serviceId: 2316,
    serviceName: "Sandbox Free IPv6",
    serviceCode: "srvE3L6P9S1Y4R",
    status: "Active",
    os: "Alpine",
    primaryAddress: "[2001:db8:30::88]",
    extraAddresses: [],
    expiresAt: "1970-01-01T00:00:00Z",
    billingCycle: "free",
    renewPrice: "¥0.00元/免费",
    firstPrice: "¥0.00元/免费",
    panelKind: null,
    panelUrl: null,
    detailUrl: buildLazycatMachineDetailUrl(2316),
    traffic: null,
    portMappings: [],
    lastSiteSyncAt: "2026-03-20T00:46:13Z",
    lastPanelSyncAt: null,
    detailState: "error",
    detailError: "NAT 代理返回 500：连接服务器失败。",
  },
];

function buildBootstrapWithLazycat(account: LazycatAccountView): BootstrapResponse {
  const bootstrap = cloneBootstrap();
  bootstrap.lazycat = structuredClone(account);
  return bootstrap;
}

function findMachineCard(canvasElement: HTMLElement, title: string) {
  const heading = within(canvasElement).getByText(title);
  const card = heading.closest(".machines-card");
  if (!(card instanceof HTMLElement)) {
    throw new Error(`Unable to find machine card for ${title}`);
  }
  return card;
}

function expectMachineActionOrder(card: HTMLElement, expected: string[]) {
  const actions = card.querySelector(".machines-card-actions");
  if (!(actions instanceof HTMLElement)) {
    throw new Error("Unable to find machine action row");
  }
  const labels = Array.from(actions.querySelectorAll("button")).map(
    (button) => button.textContent?.trim() ?? "",
  );
  expect(labels).toEqual(expected);
}

function MachinesViewDemo({
  bootstrap: initialBootstrap = buildBootstrapWithLazycat(readyAccount),
  items: initialItems = healthyMachines,
  fetchDelayMs = 0,
  syncDelayMs = 0,
  syncNextAccount,
  syncNextItems,
  syncError = null,
  detailBridgeDelayMs = 0,
  detailBridgePrimeDelayMs = 5,
  detailBridgeRedirectAfterMs = 1_000,
  detailBridgeError = null,
}: DemoProps) {
  const bootstrap = useMemo(() => cloneBootstrap(initialBootstrap), [initialBootstrap]);
  const [account, setAccount] = useState<LazycatAccountView>(() =>
    structuredClone(bootstrap.lazycat),
  );
  const [items, setItems] = useState<LazycatMachineView[]>(() => cloneMachines(initialItems));
  const accountRef = useRef(account);
  const itemsRef = useRef(items);

  useEffect(() => {
    setAccount(structuredClone(bootstrap.lazycat));
    setItems(cloneMachines(initialItems));
  }, [bootstrap, initialItems]);

  useEffect(() => {
    accountRef.current = account;
  }, [account]);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const fetchMachines = async (): Promise<LazycatMachinesResponse> => {
    if (fetchDelayMs > 0) {
      await delay(fetchDelayMs);
    }
    return {
      account: structuredClone(accountRef.current),
      items: cloneMachines(itemsRef.current),
    };
  };

  const onSync = async () => {
    if (syncDelayMs > 0) {
      await delay(syncDelayMs);
    }
    if (syncError) {
      throw new Error(syncError);
    }

    const nextAccount = structuredClone(
      syncNextAccount ?? {
        ...accountRef.current,
        state: "ready",
        lastSiteSyncAt: "2026-03-20T00:50:00Z",
        lastPanelSyncAt: "2026-03-20T00:50:12Z",
        lastError: null,
      },
    );
    const nextItems = cloneMachines(syncNextItems ?? itemsRef.current);

    accountRef.current = nextAccount;
    itemsRef.current = nextItems;
    setAccount(nextAccount);
    setItems(nextItems);
    return nextAccount;
  };

  const fetchDetailLoginBridge = async (
    serviceId: number,
  ): Promise<LazycatMachineDetailLoginBridgeResponse> => {
    window.dispatchEvent(
      new CustomEvent("machines-story-detail-login-bridge", {
        detail: {
          serviceId,
          url: buildLazycatMachineDetailLoginBridgeApiUrl(serviceId),
        },
      }),
    );
    if (detailBridgeDelayMs > 0) {
      await delay(detailBridgeDelayMs);
    }
    if (detailBridgeError) {
      throw new Error(detailBridgeError);
    }
    return {
      loginUrl: "https://lxc.lazycat.wiki/login?action=email",
      targetUrl: buildLazycatMachineDetailUrl(serviceId),
      email: "demo@lazycat.example",
      password: "secret",
      token: `bridge-token-${serviceId}`,
      primeDelayMs: detailBridgePrimeDelayMs,
      redirectAfterMs: detailBridgeRedirectAfterMs,
    };
  };

  return (
    <MachinesView
      bootstrap={{ ...bootstrap, lazycat: account }}
      onSync={onSync}
      onRefreshAccount={async () => structuredClone(accountRef.current)}
      fetchMachines={fetchMachines}
      fetchDetailLoginBridge={fetchDetailLoginBridge}
    />
  );
}

const meta = {
  title: "Pages/MachinesView",
  component: MachinesViewDemo,
  tags: ["autodocs"],
} satisfies Meta<typeof MachinesViewDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...readyAccount,
      machineCount: healthyMachines.length,
    }),
    items: healthyMachines,
  },
};

export const Loading: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...readyAccount,
      machineCount: healthyMachines.length,
    }),
    items: healthyMachines,
    fetchDelayMs: 900,
  },
};

export const VncAction: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...readyAccount,
      machineCount: healthyMachines.length,
    }),
    items: healthyMachines,
    detailBridgeRedirectAfterMs: 25,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByTestId("page-machines");

    const openCalls: Array<{ url: string; target: string }> = [];
    const submitCalls: Array<{ action: string; method: string; target: string }> = [];
    const replaceCalls: Array<{ url: string; target: string }> = [];
    const detailBridgeCalls: Array<{ serviceId: number; url: string }> = [];
    const originalOpen = window.open;
    const originalSubmit = HTMLFormElement.prototype.submit;
    const onDetailBridge = (event: Event) => {
      const detail = (event as CustomEvent<{ serviceId: number; url: string }>).detail;
      detailBridgeCalls.push(detail);
    };
    window.addEventListener("machines-story-detail-login-bridge", onDetailBridge as EventListener);
    window.open = ((url?: string | URL, target?: string) => {
      openCalls.push({
        url: url == null ? "" : String(url),
        target: target == null ? "" : String(target),
      });
      return {
        closed: false,
        opener: window,
        location: {
          replace(nextUrl: string | URL) {
            replaceCalls.push({
              url: String(nextUrl),
              target: target == null ? "" : String(target),
            });
          },
        },
      } as unknown as Window;
    }) as typeof window.open;
    HTMLFormElement.prototype.submit = function submit() {
      submitCalls.push({
        action: this.action,
        method: this.method.toUpperCase(),
        target: this.target,
      });
    };

    try {
      const vncCard = findMachineCard(canvasElement as HTMLElement, "港湾 Transit Basic");
      expectMachineActionOrder(vncCard, ["打开详情页", "打开面板", "打开 VNC", "展开详情"]);
      await userEvent.click(within(vncCard).getByRole("button", { name: "打开详情页" }));
      await waitFor(() => expect(openCalls.length).toBe(1));
      expect(openCalls[0]?.url).toBe(buildLazycatMachineDetailUrl(2312));
      expect(openCalls[0]?.target).toContain(buildLazycatMachineDetailPopupName(2312));
      await waitFor(() =>
        expect(detailBridgeCalls).toContainEqual({
          serviceId: 2312,
          url: buildLazycatMachineDetailLoginBridgeApiUrl(2312),
        }),
      );
      await waitFor(() =>
        expect(submitCalls).toContainEqual({
          action: "https://lxc.lazycat.wiki/login?action=email",
          method: "POST",
          target: openCalls[0]?.target ?? "",
        }),
      );
      await waitFor(
        () =>
          expect(replaceCalls).toContainEqual({
            url: buildLazycatMachineDetailUrl(2312),
            target: openCalls[0]?.target ?? "",
          }),
        { timeout: 2_500 },
      );
      expect(submitCalls).not.toContainEqual({
        action: buildLazycatMachineDetailUrl(2312),
        method: "GET",
        target: openCalls[0]?.target ?? "",
      });

      await userEvent.click(within(vncCard).getByRole("button", { name: "打开面板" }));
      await userEvent.click(within(vncCard).getByRole("button", { name: "打开 VNC" }));
      await waitFor(() => expect(openCalls.length).toBe(3));
      expect(openCalls[1]?.url).toBe("");
      expect(openCalls[1]?.target).toMatch(/^lazycat-panel-2312-/);
      expect(openCalls[2]?.url).toBe("");
      expect(openCalls[2]?.target).toMatch(/^lazycat-vnc-2312-/);
      expect(submitCalls).toContainEqual({
        action: buildLazycatMachineActionUrl(2312, "panel"),
        method: "POST",
        target: openCalls[1]?.target ?? "",
      });
      expect(submitCalls).toContainEqual({
        action: buildLazycatMachineActionUrl(2312, "vnc-console"),
        method: "POST",
        target: openCalls[2]?.target ?? "",
      });

      const livePanelCard = findMachineCard(canvasElement as HTMLElement, "Apex Compute Lite");
      expectMachineActionOrder(livePanelCard, ["打开详情页", "打开面板", "打开 VNC", "展开详情"]);
      const detailButton = within(livePanelCard).getByRole("button", { name: "打开详情页" });
      expect(detailButton).toBeEnabled();
      await userEvent.click(detailButton);
      await waitFor(() => expect(openCalls.length).toBe(4));
      expect(openCalls[3]?.url).toBe(buildLazycatMachineDetailUrl(2314));
      expect(openCalls[3]?.target).toContain(buildLazycatMachineDetailPopupName(2314));
      await waitFor(() =>
        expect(detailBridgeCalls).toContainEqual({
          serviceId: 2314,
          url: buildLazycatMachineDetailLoginBridgeApiUrl(2314),
        }),
      );
      await waitFor(() =>
        expect(submitCalls).toContainEqual({
          action: "https://lxc.lazycat.wiki/login?action=email",
          method: "POST",
          target: openCalls[3]?.target ?? "",
        }),
      );
      await waitFor(
        () =>
          expect(replaceCalls).toContainEqual({
            url: buildLazycatMachineDetailUrl(2314),
            target: openCalls[3]?.target ?? "",
          }),
        { timeout: 2_500 },
      );
      expect(submitCalls).not.toContainEqual({
        action: buildLazycatMachineDetailUrl(2314),
        method: "GET",
        target: openCalls[3]?.target ?? "",
      });

      const panelButton = within(livePanelCard).getByRole("button", { name: "打开面板" });
      expect(panelButton).toBeEnabled();
      await userEvent.click(panelButton);
      const liveVncButton = within(livePanelCard).getByRole("button", { name: "打开 VNC" });
      expect(liveVncButton).toBeEnabled();
      await userEvent.click(liveVncButton);
      await waitFor(() => expect(openCalls.length).toBe(6));
      expect(openCalls[4]?.url).toBe("");
      expect(openCalls[4]?.target).toMatch(/^lazycat-panel-2314-/);
      expect(openCalls[5]?.url).toBe("");
      expect(openCalls[5]?.target).toMatch(/^lazycat-vnc-2314-/);
      expect(submitCalls).toContainEqual({
        action: buildLazycatMachineActionUrl(2314, "panel"),
        method: "POST",
        target: openCalls[4]?.target ?? "",
      });
      expect(submitCalls).toContainEqual({
        action: buildLazycatMachineActionUrl(2314, "vnc-console"),
        method: "POST",
        target: openCalls[5]?.target ?? "",
      });
    } finally {
      window.removeEventListener(
        "machines-story-detail-login-bridge",
        onDetailBridge as EventListener,
      );
      window.open = originalOpen;
      HTMLFormElement.prototype.submit = originalSubmit;
    }
  },
};

export const PartialFailure: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...partialFailureAccount,
      machineCount: degradedMachines.length,
    }),
    items: degradedMachines,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByTestId("page-machines");
    expect(canvas.getByText("部分面板同步失败（2/3），已保留最近一次成功缓存")).toBeVisible();

    const staleCard = findMachineCard(canvasElement as HTMLElement, "北湾 NAT 02");
    await userEvent.click(within(staleCard).getByRole("button", { name: "展开详情" }));

    await within(staleCard).findByText("端口映射");
    expect(within(staleCard).getByText("最近成功缓存")).toBeVisible();
    expect(
      within(staleCard).getByText("面板 TLS 校验失败，已保留最近一次成功缓存。"),
    ).toBeVisible();
  },
};

export const DetailLoginBridgeFailure: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...readyAccount,
      machineCount: healthyMachines.length,
    }),
    items: healthyMachines,
    detailBridgeError: "懒猫登录桥接失败",
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByTestId("page-machines");

    const submitCalls: Array<{ action: string; method: string; target: string }> = [];
    const originalOpen = window.open;
    const originalSubmit = HTMLFormElement.prototype.submit;
    window.open = ((_url?: string | URL, _target?: string) =>
      ({
        closed: false,
        opener: window,
      }) as unknown as Window) as typeof window.open;
    HTMLFormElement.prototype.submit = function submit() {
      submitCalls.push({
        action: this.action,
        method: this.method.toUpperCase(),
        target: this.target,
      });
    };

    try {
      const card = findMachineCard(canvasElement as HTMLElement, "港湾 Transit Basic");
      await userEvent.click(within(card).getByRole("button", { name: "打开详情页" }));
      await waitFor(() => expect(canvas.getByText("懒猫登录桥接失败")).toBeVisible());
      expect(submitCalls).toHaveLength(0);
    } finally {
      window.open = originalOpen;
      HTMLFormElement.prototype.submit = originalSubmit;
    }
  },
};

export const Disconnected: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat(disconnectedAccount),
    items: [],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = await canvas.findByTestId("page-machines");
    expect(page).toBeVisible();
    expect(page.textContent).toContain("还没有连接懒猫云账号。");
    expect(canvas.getByRole("button", { name: "立即同步" })).toBeDisabled();
  },
};

export const SyncActionFlow: Story = {
  args: {
    bootstrap: buildBootstrapWithLazycat({
      ...syncingAccount,
      machineCount: healthyMachines.length,
    }),
    items: healthyMachines,
    syncDelayMs: 400,
    syncNextAccount: {
      ...readyAccount,
      machineCount: healthyMachines.length,
      lastSiteSyncAt: "2026-03-20T00:50:00Z",
      lastPanelSyncAt: "2026-03-20T00:50:12Z",
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const syncSummary = canvas.getByText("最近同步").closest(".machines-summary-card");
    const previousSummaryText = syncSummary?.textContent ?? "";
    const syncButton = await canvas.findByRole("button", { name: "立即同步" });
    await userEvent.click(syncButton);
    expect(await canvas.findByRole("button", { name: "同步中" })).toBeDisabled();
    await waitFor(() => {
      expect(canvas.getByRole("button", { name: "立即同步" })).toBeEnabled();
    });
    await waitFor(() => {
      expect(syncSummary?.textContent).not.toBe(previousSummaryText);
    });
  },
};

export const ResponsiveAllBreakpoints: Story = {
  render: () => (
    <ResponsivePageStory
      route="machines"
      title="Catnap • 机器资产"
      subtitle="懒猫云账号只读缓存 • 自动续会话 • 主站与面板信息统一收口"
      actions={
        <>
          <span className="pill badge warn">有新版本 v0.10.0</span>
          <span className="pill sm">主题 · 系统</span>
        </>
      }
      renderPage={() => (
        <MachinesViewDemo
          bootstrap={buildBootstrapWithLazycat({
            ...partialFailureAccount,
            machineCount: degradedMachines.length,
          })}
          items={degradedMachines}
        />
      )}
    />
  ),
  play: async ({ canvasElement }) => {
    await expectResponsiveBreakpoints(canvasElement, "page-machines");
    const card = findMachineCard(canvasElement as HTMLElement, "北湾 NAT 02");
    expect(within(card).getByRole("button", { name: "打开详情页" })).toBeVisible();
  },
};
