import type {
  BootstrapResponse,
  Config,
  Country,
  LogsResponse,
  NotificationRecord,
  NotificationRecordsResponse,
  Region,
} from "../App";

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

function money(amount: number, currency: string, period: "month" | "year" = "month") {
  return { amount, currency, period };
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
      { key: "Traffic", value: "1TB" },
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
    sourcePid: "128",
    name: "VPS • 2C/4G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "4G" },
      { key: "Disk", value: "80G" },
      { key: "Bandwidth", value: "100Mbps" },
      { key: "Traffic", value: "800GB" },
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
    sourcePid: "129",
    name: "VPS • 1C/2G",
    specs: [
      { key: "CPU", value: "1C" },
      { key: "RAM", value: "2G" },
      { key: "Disk", value: "60G" },
      { key: "Bandwidth", value: "200Mbps" },
      { key: "Traffic", value: "600GB" },
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
    sourcePid: "130",
    name: "VPS • 2C/8G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "8G" },
      { key: "Disk", value: "120G" },
      { key: "Bandwidth", value: "1Gbps" },
      { key: "Traffic", value: "1TB" },
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
    sourcePid: "131",
    name: "VPS • 4C/8G",
    specs: [
      { key: "CPU", value: "4C" },
      { key: "RAM", value: "8G" },
      { key: "Disk", value: "160G" },
      { key: "Traffic", value: "1.5TB" },
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
    sourcePid: "132",
    name: "VPS • 2C/2G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "2G" },
      { key: "Disk", value: "50G" },
      { key: "Bandwidth", value: "200Mbps" },
      { key: "Traffic", value: "500GB" },
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
    sourcePid: "133",
    name: "VPS • 2C/4G",
    specs: [
      { key: "CPU", value: "2C" },
      { key: "RAM", value: "4G" },
      { key: "Disk", value: "90G" },
      { key: "Bandwidth", value: "500Mbps" },
      { key: "Traffic", value: "900GB" },
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
      { key: "Traffic", value: "3TB" },
    ],
    price: money(79.0, "USD"),
    inventory: inv(0, new Date(demoNowMs - 1000 * 60 * 25).toISOString(), "unknown"),
    digest: "demo",
    lifecycle: life("active"),
    monitorSupported: true,
    monitorEnabled: true,
  },
];

export const demoYearlyCnyConfig: Config = {
  ...demoConfigs[1],
  id: "cfg-yearly-cny",
  name: "芬兰特惠年付 Mini",
  price: money(4.99, "CNY", "year"),
};

export const demoBootstrap: BootstrapResponse = {
  user: { id: "u-demo", displayName: "Demo" },
  catalog: {
    countries: demoCountries,
    regions: demoRegions,
    regionNotices: [
      {
        countryId: "jp",
        regionId: "jp-tokyo",
        text: "东京线路：默认优化国际出口，禁止高频滥用。",
      },
      {
        countryId: "us",
        regionId: "us-ca",
        text: "美国家宽：动态 IP，禁止发包与扫描等滥用行为。",
      },
    ],
    configs: demoConfigs,
    fetchedAt,
    source: { url: "https://example.com/catalog" },
  },
  monitoring: {
    enabledConfigIds: demoConfigs.filter((c) => c.monitorEnabled).map((c) => c.id),
    enabledPartitions: [
      { countryId: "jp", regionId: "jp-tokyo" },
      { countryId: "us", regionId: "us-ca" },
    ],
    poll: { intervalSeconds: 60, jitterPct: 0.2 },
  },
  settings: {
    poll: { intervalMinutes: 1, jitterPct: 0.2 },
    siteBaseUrl: "https://lxc.lazycat.wiki",
    catalogRefresh: { autoIntervalHours: 1 },
    monitoringEvents: {
      partitionListedEnabled: true,
      siteListedEnabled: true,
      delistedEnabled: true,
    },
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

export const demoNotificationRecords: NotificationRecord[] = [
  {
    id: "nr-3",
    createdAt: new Date(demoNowMs - 1000 * 60 * 2).toISOString(),
    kind: "monitoring.config",
    title: "配置更新",
    summary: "HKG-Pro.TRFC Pro · 库存 1｜¥90.00 / 月",
    partitionLabel: "中国香港 / HKG",
    telegramStatus: "success",
    webPushStatus: "skipped",
    items: [
      {
        configId: "lc:hkg:pro",
        countryName: "中国香港",
        regionName: "HKG",
        partitionLabel: "中国香港 / HKG",
        name: "HKG-Pro.TRFC Pro",
        specs: [
          { key: "CPU", value: "4 vCPU" },
          { key: "RAM", value: "8 GB" },
          { key: "Disk", value: "80 GB NVMe" },
          { key: "Bandwidth", value: "1 Gbps" },
        ],
        price: money(90, "CNY"),
        inventory: inv(1, new Date(demoNowMs - 1000 * 60 * 2).toISOString()),
        lifecycle: life("active"),
      },
    ],
  },
  {
    id: "nr-2",
    createdAt: new Date(demoNowMs - 1000 * 60 * 9).toISOString(),
    kind: "catalog.delisted",
    title: "已下架",
    summary: "HKG-Pro.TRFC Plus · 最近状态：库存 1｜¥54.00 / 月",
    partitionLabel: "中国香港 / HKG",
    telegramStatus: "error",
    webPushStatus: "skipped",
    items: [
      {
        configId: "lc:hkg:plus",
        countryName: "中国香港",
        regionName: "HKG",
        partitionLabel: "中国香港 / HKG",
        name: "HKG-Pro.TRFC Plus",
        specs: [
          { key: "CPU", value: "2 vCPU" },
          { key: "RAM", value: "4 GB" },
          { key: "Disk", value: "50 GB NVMe" },
          { key: "Traffic", value: "2 TB" },
        ],
        price: money(54, "CNY"),
        inventory: inv(1, new Date(demoNowMs - 1000 * 60 * 9).toISOString()),
        lifecycle: life("delisted", fetchedAt, new Date(demoNowMs - 1000 * 60 * 9).toISOString()),
      },
    ],
  },
  {
    id: "nr-1",
    createdAt: new Date(demoNowMs - 1000 * 60 * 18).toISOString(),
    kind: "catalog.partition_listed",
    title: "分区上新机",
    summary: "格陵兰特惠探针 · 库存 1｜¥0.88 / 月",
    partitionLabel: "格陵兰 / 格陵兰",
    telegramStatus: "success",
    webPushStatus: "success",
    items: [
      {
        configId: "lc:gl:probe",
        countryName: "格陵兰",
        regionName: "格陵兰",
        partitionLabel: "格陵兰 / 格陵兰",
        name: "格陵兰特惠探针",
        specs: [
          { key: "CPU", value: "1 vCPU" },
          { key: "RAM", value: "512 MB" },
          { key: "Disk", value: "10 GB SSD" },
          { key: "Traffic", value: "500 GB" },
        ],
        price: money(0.88, "CNY"),
        inventory: inv(1, new Date(demoNowMs - 1000 * 60 * 18).toISOString()),
        lifecycle: life("active"),
      },
      {
        configId: "lc:gl:probe-max",
        countryName: "格陵兰",
        regionName: "格陵兰",
        partitionLabel: "格陵兰 / 格陵兰",
        name: "格陵兰特惠探针 Max",
        specs: [
          { key: "CPU", value: "2 vCPU" },
          { key: "RAM", value: "1 GB" },
          { key: "Disk", value: "20 GB SSD" },
          { key: "Traffic", value: "1 TB" },
        ],
        price: money(1.48, "CNY"),
        inventory: inv(3, new Date(demoNowMs - 1000 * 60 * 18).toISOString()),
        lifecycle: life("active"),
      },
    ],
  },
];

export function demoNotificationRecordsResponse(params: {
  cursor: string | null;
  limit: number;
}): NotificationRecordsResponse {
  const sorted = [...demoNotificationRecords].sort(
    (a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt) || b.id.localeCompare(a.id),
  );
  const startIndex = params.cursor ? sorted.findIndex((item) => item.id === params.cursor) + 1 : 0;
  const items = sorted.slice(startIndex, startIndex + params.limit);
  const next = sorted[startIndex + params.limit];
  return { items, nextCursor: next ? next.id : null };
}
