import type { BootstrapResponse, Config, Country, LogsResponse, Region } from "../App";

export const demoNowMs = Date.now();

export const demoCountries: Country[] = [
  { id: "cloud", name: "云服务器" },
  { id: "jp", name: "日本" },
  { id: "us", name: "美国" },
];

export const demoRegions: Region[] = [
  { id: "jp-tokyo", countryId: "jp", name: "东京", locationName: "JP-East" },
  { id: "jp-osaka", countryId: "jp", name: "大阪", locationName: "JP-West" },
  { id: "us-ca", countryId: "us", name: "加州", locationName: "US-West" },
];

function money(amount: number, currency: string) {
  return { amount, currency, period: "month" };
}

function inv(quantity: number, checkedAtIso: string, status?: Config["inventory"]["status"]) {
  return {
    quantity,
    checkedAt: checkedAtIso,
    status: status ?? (quantity > 0 ? "available" : "unavailable"),
  };
}

const fetchedAt = new Date(demoNowMs - 1000 * 35).toISOString();

function life(
  state: Config["lifecycle"]["state"] = "active",
  listedAtIso: string = fetchedAt,
  delistedAtIso?: string | null,
): Config["lifecycle"] {
  return {
    state,
    listedAt: listedAtIso,
    delistedAt: delistedAtIso ?? null,
  };
}

export const demoConfigs: Config[] = [
  {
    id: "cfg-cloud-1",
    countryId: "cloud",
    regionId: null,
    name: "懒猫云 • 1C/2G",
    specs: [
      { key: "CPU", value: "1C" },
      { key: "RAM", value: "2G" },
      { key: "Disk", value: "30G" },
    ],
    price: money(9.9, "USD"),
    inventory: inv(999, fetchedAt),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: false,
    monitorEnabled: false,
  },
  {
    id: "cfg-1",
    countryId: "jp",
    regionId: "jp-tokyo",
    name: "VPS • 2C/4G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "4G" },
      { key: "Disk", value: "80G" },
      { key: "Bandwidth", value: "100Mbps" },
    ],
    price: money(19.9, "USD"),
    inventory: inv(3, new Date(demoNowMs - 1000 * 60 * 2).toISOString()),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: true,
  },
  {
    id: "cfg-4",
    countryId: "jp",
    regionId: "jp-tokyo",
    name: "VPS • 1C/2G",
    specs: [
      { key: "CPU", value: "1C" },
      { key: "RAM", value: "2G" },
      { key: "Disk", value: "60G" },
      { key: "Bandwidth", value: "200Mbps" },
    ],
    price: money(14.9, "USD"),
    inventory: inv(12, new Date(demoNowMs - 1000 * 60 * 4).toISOString()),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: false,
  },
  {
    id: "cfg-5",
    countryId: "jp",
    regionId: "jp-tokyo",
    name: "VPS • 2C/8G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "8G" },
      { key: "Disk", value: "120G" },
      { key: "Bandwidth", value: "1Gbps" },
    ],
    price: money(29.9, "USD"),
    inventory: inv(0, new Date(demoNowMs - 1000 * 60 * 8).toISOString()),
    digest: "demo",
    lifecycle: life("delisted", fetchedAt, new Date(demoNowMs - 1000 * 60 * 30).toISOString()),
    monitorSupported: true,
    monitorEnabled: false,
  },
  {
    id: "cfg-2",
    countryId: "jp",
    regionId: "jp-osaka",
    name: "VPS • 4C/8G",
    specs: [
      { key: "CPU", value: "4C" },
      { key: "RAM", value: "8G" },
      { key: "Disk", value: "160G" },
    ],
    price: money(39.9, "USD"),
    inventory: inv(0, new Date(demoNowMs - 1000 * 60 * 6).toISOString()),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: false,
  },
  {
    id: "cfg-2b",
    countryId: "jp",
    regionId: "jp-osaka",
    name: "VPS • 2C/2G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "2G" },
      { key: "Disk", value: "50G" },
      { key: "Bandwidth", value: "200Mbps" },
    ],
    price: money(17.9, "USD"),
    inventory: inv(5, new Date(demoNowMs - 1000 * 60 * 3).toISOString()),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: false,
  },
  {
    id: "cfg-2c",
    countryId: "jp",
    regionId: "jp-osaka",
    name: "VPS • 2C/4G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "4G" },
      { key: "Disk", value: "90G" },
      { key: "Bandwidth", value: "500Mbps" },
    ],
    price: money(24.9, "USD"),
    inventory: inv(0, new Date(demoNowMs - 1000 * 60 * 11).toISOString()),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: false,
  },
  {
    id: "cfg-3",
    countryId: "us",
    regionId: "us-ca",
    name: "VPS • 8C/16G",
    specs: [
      { key: "CPU", value: "8C" },
      { key: "RAM", value: "16G" },
      { key: "Disk", value: "320G" },
    ],
    price: money(79.0, "USD"),
    inventory: inv(0, new Date(demoNowMs - 1000 * 60 * 25).toISOString(), "unknown"),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: true,
  },
];

export const demoBootstrap: BootstrapResponse = {
  user: { id: "u-demo", displayName: "Demo" },
  app: {
    effectiveVersion: "0.1.0",
    webDistBuildId: "demo-build-id-00000000",
    repoUrl: "https://github.com/IvanLi-CN/catnap",
  },
  catalog: {
    countries: demoCountries,
    regions: demoRegions,
    configs: demoConfigs,
    fetchedAt,
    source: { url: "https://example.com/catalog" },
  },
  monitoring: {
    enabledConfigIds: demoConfigs.filter((c) => c.monitorEnabled).map((c) => c.id),
    poll: { intervalSeconds: 60, jitterPct: 0.2 },
  },
  settings: {
    poll: { intervalMinutes: 1, jitterPct: 0.2 },
    siteBaseUrl: "https://lazycats.vip",
    catalogRefresh: { autoIntervalHours: 6 },
    monitoringEvents: { listedEnabled: true, delistedEnabled: true },
    notifications: {
      telegram: { enabled: true, configured: true, target: "@catnap" },
      webPush: { enabled: false, vapidPublicKey: "demo-vapid-public-key" },
    },
  },
};

export function countriesById() {
  return new Map<string, Country>(demoCountries.map((c) => [c.id, c]));
}

export function regionsById() {
  return new Map<string, Region>(demoRegions.map((r) => [r.id, r]));
}

export function demoLogsResponse(params: {
  level: string;
  cursor: string | null;
  limit: number;
}): LogsResponse {
  const base: LogsResponse["items"] = [
    {
      id: "l-1",
      ts: new Date(demoNowMs - 1000 * 10).toISOString(),
      level: "info",
      scope: "monitor",
      message: "同步完成：0→3",
    },
    {
      id: "l-2",
      ts: new Date(demoNowMs - 1000 * 30).toISOString(),
      level: "warn",
      scope: "inventory",
      message: "库存未知：上游超时",
    },
    {
      id: "l-3",
      ts: new Date(demoNowMs - 1000 * 90).toISOString(),
      level: "error",
      scope: "notify",
      message: "Telegram: 401 Unauthorized",
    },
  ];

  const items = base
    .filter((it) => params.level === "debug" || it.level === params.level)
    .slice(0, params.limit);
  return { items, nextCursor: params.cursor ? null : "cursor:next" };
}
