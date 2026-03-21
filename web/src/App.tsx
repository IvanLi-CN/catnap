import flagAq from "@iconify-icons/flagpack/aq";
import flagAt from "@iconify-icons/flagpack/at";
import flagCa from "@iconify-icons/flagpack/ca";
import flagCh from "@iconify-icons/flagpack/ch";
import flagDe from "@iconify-icons/flagpack/de";
import flagFi from "@iconify-icons/flagpack/fi";
import flagGb from "@iconify-icons/flagpack/gb";
import flagGl from "@iconify-icons/flagpack/gl";
import flagHk from "@iconify-icons/flagpack/hk";
import flagIe from "@iconify-icons/flagpack/ie";
import flagIn from "@iconify-icons/flagpack/in";
import flagIs from "@iconify-icons/flagpack/is";
import flagJp from "@iconify-icons/flagpack/jp";
import flagKp from "@iconify-icons/flagpack/kp";
import flagRu from "@iconify-icons/flagpack/ru";
import flagSg from "@iconify-icons/flagpack/sg";
import flagTr from "@iconify-icons/flagpack/tr";
import flagTw from "@iconify-icons/flagpack/tw";
import flagUa from "@iconify-icons/flagpack/ua";
import flagUs from "@iconify-icons/flagpack/us";
import flagVn from "@iconify-icons/flagpack/vn";
import { Icon } from "@iconify/react";
import {
  type CSSProperties,
  type KeyboardEvent,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { LazycatTrafficCycleChart } from "./ui/charts/LazycatTrafficCycleChart";
import {
  buildLazycatTrafficCycle,
  formatTrafficValue as formatLazycatTrafficValue,
} from "./ui/charts/lazycatTrafficCycle";
import { MonitorToggle, type MonitorToggleState } from "./ui/controls/MonitorToggle";
import { SettingsFeedbackBubble } from "./ui/feedback/SettingsFeedbackBubble";
import { AppShell } from "./ui/layout/AppShell";
import { ThemeMenu } from "./ui/nav/ThemeMenu";
import "./app.css";

type ApiError = {
  error: { code: string; message: string };
};

export const SETTINGS_TEST_SUCCESS_BUBBLE_MS = 4_000;
const TOPOLOGY_PROBE_MINUTES = 15;

class ApiHttpError extends Error {
  status: number;
  statusText: string;

  constructor(status: number, statusText: string, message: string) {
    super(message);
    this.status = status;
    this.statusText = statusText;
  }
}

export type UserView = { id: string; displayName?: string };

export type Country = { id: string; name: string };
export type Region = {
  id: string;
  countryId: string;
  name: string;
  locationName?: string;
};
export type RegionNotice = {
  countryId: string;
  regionId?: string | null;
  text: string;
};

export type MonitoringPartition = {
  countryId: string;
  regionId?: string | null;
};

export type Spec = { key: string; value: string };
export type Money = { amount: number; currency: string; period: string };
export type Inventory = {
  status: "unknown" | "available" | "unavailable";
  quantity: number;
  checkedAt: string;
};

export type ConfigLifecycle = {
  state: "active" | "delisted";
  listedAt: string;
  delistedAt?: string | null;
  cleanupAt?: string | null;
};

export type Config = {
  id: string;
  countryId: string;
  regionId: string | null;
  sourcePid?: string;
  sourceFid?: string;
  sourceGid?: string;
  name: string;
  specs: Spec[];
  price: Money;
  inventory: Inventory;
  digest: string;
  lifecycle: ConfigLifecycle;
  monitorSupported: boolean;
  monitorEnabled: boolean;
};

export type SettingsView = {
  poll: { intervalMinutes: number; jitterPct: number };
  siteBaseUrl: string | null;
  catalogRefresh: { autoIntervalHours: number | null };
  monitoringEvents: {
    partitionCatalogChangeEnabled: boolean;
    regionPartitionChangeEnabled: boolean;
    siteRegionChangeEnabled: boolean;
  };
  notifications: {
    telegram: { enabled: boolean; configured: boolean; targets: string[] };
    webPush: { enabled: boolean; vapidPublicKey?: string };
  };
};

export type LazycatAccountView = {
  connected: boolean;
  email?: string | null;
  state: "disconnected" | "authenticating" | "syncing" | "ready" | "error" | string;
  machineCount: number;
  lastSiteSyncAt?: string | null;
  lastPanelSyncAt?: string | null;
  lastError?: string | null;
};

export type LazycatTrafficView = {
  usedGb: number;
  limitGb: number;
  resetDay: number;
  cycleStartAt: string;
  cycleEndAt: string;
  history: Array<{
    sampledAt: string;
    usedGb: number;
    limitGb: number;
  }>;
  lastResetAt?: string | null;
  display?: string | null;
};

export type LazycatPortMappingView = {
  family: string;
  publicIp?: string | null;
  publicPort?: number | null;
  publicPortEnd?: number | null;
  privateIp?: string | null;
  privatePort?: number | null;
  privatePortEnd?: number | null;
  protocol?: string | null;
  status?: string | null;
  description?: string | null;
};

export type LazycatMachineView = {
  serviceId: number;
  serviceName: string;
  serviceCode: string;
  status: string;
  os?: string | null;
  primaryAddress?: string | null;
  extraAddresses: string[];
  expiresAt?: string | null;
  billingCycle?: string | null;
  renewPrice?: string | null;
  firstPrice?: string | null;
  traffic?: LazycatTrafficView | null;
  portMappings: LazycatPortMappingView[];
  lastSiteSyncAt?: string | null;
  lastPanelSyncAt?: string | null;
  detailState: string;
  detailError?: string | null;
};

export type LazycatMachinesResponse = {
  account: LazycatAccountView;
  items: LazycatMachineView[];
};

export type BootstrapResponse = {
  user: UserView;
  catalog: {
    countries: Country[];
    regions: Region[];
    regionNotices: RegionNotice[];
    configs: Config[];
    fetchedAt: string;
    source: { url: string };
  };
  monitoring: {
    enabledConfigIds: string[];
    enabledPartitions: MonitoringPartition[];
    poll: { intervalSeconds: number; jitterPct: number };
  };
  settings: SettingsView;
  lazycat: LazycatAccountView;
};

export type AboutUpdate = {
  enabled: boolean;
  status: "ok" | "disabled" | "error";
  checkedAt?: string;
  latestVersion?: string;
  latestUrl?: string;
  updateAvailable: boolean;
  message?: string;
};

export type AboutResponse = {
  version: string;
  webDistBuildId: string;
  repoUrl: string;
  update: AboutUpdate;
};

export type ProductsResponse = {
  configs: Config[];
  fetchedAt: string;
};

export type PartitionRefreshResponse = {
  countryId: string;
  regionId: string;
  refreshed: boolean;
};

export type ArchiveDelistedResponse = {
  archivedCount: number;
  archivedAt?: string | null;
  archivedIds: string[];
};

export type ArchiveFilterMode = "active" | "all" | "archived";

type ProductRegionGroup = {
  key: string;
  regionId: string;
  title: string;
  subtitle: string | null;
  partitionEnabled: boolean;
  refreshAvailable: boolean;
  notice: string | null;
  configs: Config[];
};

type ProductCountryGroup = {
  countryId: string;
  countryName: string;
  isCloud: boolean;
  countryMonitorEnabled: boolean;
  countryNotice: string | null;
  directConfigs: Config[];
  groups: ProductRegionGroup[];
};

export type PartitionRefreshState =
  | { kind: "running"; message?: string | null }
  | { kind: "success"; message: string }
  | { kind: "error"; message: string };

export type InventoryHistoryPoint = { tsMinute: string; quantity: number };
export type InventoryHistorySeries = {
  configId: string;
  points: InventoryHistoryPoint[];
};
export type InventoryHistoryResponse = {
  window: { from: string; to: string };
  series: InventoryHistorySeries[];
};

const EMPTY_HISTORY_BY_ID = new Map<string, InventoryHistoryPoint[]>();

export type RefreshStatusResponse = {
  state: "idle" | "syncing" | "success" | "error";
  done: number;
  total: number;
  message?: string;
};

export type CatalogRefreshStatus = {
  jobId: string;
  state: "idle" | "running" | "success" | "error";
  trigger: "manual" | "auto";
  done: number;
  total: number;
  message?: string | null;
  startedAt: string;
  updatedAt: string;
  current?: {
    urlKey: string;
    url: string;
    action: "fetch" | "cache";
    note?: string | null;
  } | null;
};

export type MonitoringResponse = {
  items: Config[];
  fetchedAt: string;
  recentListed24h: Config[];
};

export type LogsResponse = {
  items: Array<{
    id: string;
    ts: string;
    level: "debug" | "info" | "warn" | "error";
    scope: string;
    message: string;
    meta?: unknown;
  }>;
  nextCursor: string | null;
};

export type NotificationRecordItem = {
  configId?: string | null;
  countryName: string;
  regionName?: string | null;
  partitionLabel?: string | null;
  name: string;
  specs: Spec[];
  price: Money;
  inventory: Inventory;
  lifecycle: ConfigLifecycle;
};

export type NotificationRecordDelivery = {
  channel: string;
  target: string;
  status: string;
  error?: string | null;
};

export type NotificationRecord = {
  id: string;
  createdAt: string;
  kind: string;
  title: string;
  summary: string;
  partitionLabel?: string | null;
  telegramStatus: string;
  webPushStatus: string;
  telegramDeliveries?: NotificationRecordDelivery[];
  items: NotificationRecordItem[];
};

type TelegramTestDeliveryResult = {
  target: string;
  status: string;
  error?: string | null;
};

type TelegramTestResponse = {
  ok: boolean;
  status: string;
  results: TelegramTestDeliveryResult[];
};

export type NotificationRecordsResponse = {
  items: NotificationRecord[];
  nextCursor: string | null;
};

export type OpsRange = "24h" | "7d" | "30d";

export type OpsRateBucket = {
  total: number;
  success: number;
  failure: number;
  successRatePct: number;
  cacheHits: number;
};

export type OpsSparksResponse = {
  bucketSeconds: number;
  volume: number[];
  collectionSuccessRatePct: number[];
  notifyTelegramSuccessRatePct: number[];
  notifyWebPushSuccessRatePct: number[];
};

export type OpsSseUi = {
  status: "connected" | "reconnecting" | "reset";
  replayWindowSeconds: number | null;
  lastEventId: number | null;
  lastReset: { serverTime: string; reason: string; details?: string | null } | null;
};

export type OpsStateResponse = {
  serverTime: string;
  range: OpsRange;
  replayWindowSeconds: number;
  queue: {
    pending: number;
    running: number;
    deduped: number;
    oldestWaitSeconds: number | null;
    reasonCounts: Record<string, number>;
  };
  workers: Array<{
    workerId: string;
    state: "idle" | "running" | "error";
    task: { fid: string; gid: string | null } | null;
    startedAt: string | null;
    lastError: { ts: string; message: string } | null;
  }>;
  tasks: Array<{
    key: { fid: string; gid: string | null };
    state: "pending" | "running";
    enqueuedAt: string;
    reasonCounts: Record<string, number>;
    lastRun: { endedAt: string; ok: boolean } | null;
  }>;
  stats: {
    collection: OpsRateBucket;
    notify: { telegram?: OpsRateBucket; webPush?: OpsRateBucket };
  };
  sparks: OpsSparksResponse;
  topology: {
    status: string;
    refreshedAt: string | null;
    requestCount: number;
    message: string | null;
  };
  logTail: Array<{
    eventId: number;
    ts: string;
    level: "debug" | "info" | "warn" | "error";
    scope: string;
    message: string;
    meta?: unknown;
  }>;
};

export type Route =
  | "monitoring"
  | "products"
  | "notifications"
  | "machines"
  | "settings"
  | "logs"
  | "ops";

function getRoute(): Route {
  const raw = window.location.hash.replace(/^#/, "");
  if (
    raw === "products" ||
    raw === "notifications" ||
    raw === "machines" ||
    raw === "settings" ||
    raw === "logs" ||
    raw === "ops"
  ) {
    return raw;
  }
  return "monitoring";
}

function getNotificationTargetId(): string | null {
  const value = new URLSearchParams(window.location.search).get("notification")?.trim();
  return value ? value : null;
}

function routeTitle(route: Route): string {
  if (route === "products") return "全部产品";
  if (route === "notifications") return "通知记录";
  if (route === "machines") return "机器资产";
  if (route === "settings") return "系统设置";
  if (route === "logs") return "日志";
  if (route === "ops") return "采集观测台";
  return "库存监控";
}

function routeSubtitle(route: Route): string {
  if (route === "products") return "分组：国家地区 → 可用区域 → 配置 • 点击卡片下单（新标签页）";
  if (route === "notifications") return "按用户隔离 • 深链定位 • cursor 按需加载与无限滚动";
  if (route === "machines") return "懒猫云账号只读缓存 • 自动续会话 • 主站与面板信息统一收口";
  if (route === "settings") return "按用户隔离 • 自动保存（下次轮询使用新频率 + 抖动）";
  if (route === "logs") return "按用户隔离 • 支持过滤与分页（cursor）";
  if (route === "ops")
    return "全局共享 • 队列/worker/成功率/推送成功率 • SSE 实时 tail（断线自动续传/重置）";
  return "按国家地区 / 可用区分组展示；支持折叠，默认展开（折叠状态可记忆）";
}

function emptyLazycatAccount(): LazycatAccountView {
  return {
    connected: false,
    state: "disconnected",
    machineCount: 0,
    lastSiteSyncAt: null,
    lastPanelSyncAt: null,
    lastError: null,
  };
}

function lazycatAccountTone(state: string): string {
  if (state === "ready") return "on";
  if (state === "syncing" || state === "authenticating") return "warn";
  if (state === "error") return "err";
  return "disabled";
}

function lazycatAccountLabel(account: LazycatAccountView): string {
  if (!account.connected) return "未连接";
  if (account.state === "authenticating") return "登录中";
  if (account.state === "syncing") return "同步中";
  if (account.state === "ready") return "已连接";
  if (account.state === "error") return "异常";
  return account.state;
}

function lazycatDetailLabel(detailState: string): string {
  if (detailState === "ready") return "正常";
  if (detailState === "stale") return "使用缓存";
  if (detailState === "error") return "失败";
  if (detailState === "pending") return "待补全";
  return detailState;
}

function lazycatDetailClass(detailState: string): string {
  if (detailState === "ready") return "pill sm badge on";
  if (detailState === "stale") return "pill sm badge warn";
  if (detailState === "error") return "pill sm badge err";
  return "pill sm badge";
}

function InlineSpinner({ className = "sync-icon spin" }: { className?: string }) {
  return (
    <span className={className} aria-hidden="true">
      <svg viewBox="0 0 24 24" focusable="false">
        <title>Loading</title>
        <path
          fill="currentColor"
          d="M12 4a8 8 0 0 1 7.9 6.7a1 1 0 1 1-2 .3A6 6 0 1 0 18 12a1 1 0 1 1 2 0a8 8 0 1 1-8-8"
        />
      </svg>
    </span>
  );
}

function renderLazycatAccountBadge(account: LazycatAccountView) {
  const busy = account.state === "syncing" || account.state === "authenticating";
  return (
    <span className={`pill badge ${lazycatAccountTone(account.state)} lazycat-account-badge`}>
      {busy ? <InlineSpinner className="sync-icon spin lazycat-account-badge-spin" /> : null}
      {lazycatAccountLabel(account)}
    </span>
  );
}

function formatPortRange(start?: number | null, end?: number | null): string {
  if (start == null) return "—";
  if (end == null || end === start) return String(start);
  return `${start}-${end}`;
}

function formatLazycatAddress(address?: string | null): string {
  return address?.trim() ? address.trim() : "—";
}

type OrderLinkMode = "configureproduct";
type OrderLink = { url: string; mode: OrderLinkMode };
type OrderGuardDialog = {
  configId: string;
  configName: string;
  orderUrl: string;
  checking: boolean;
  initialQty: number;
  latestQty: number | null;
  latestCheckedAt: string | null;
  checkError: string | null;
};

function buildOrderLink(
  baseCartUrl: string,
  cfg: Pick<Config, "sourcePid" | "sourceFid" | "sourceGid">,
): OrderLink | null {
  const pid = cfg.sourcePid?.trim();
  if (!pid) return null;
  try {
    const url = new URL(baseCartUrl, window.location.origin);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    url.searchParams.set("action", "configureproduct");
    url.searchParams.set("pid", pid);
    return { url: url.toString(), mode: "configureproduct" };
  } catch {
    return null;
  }
}

function buildCountryCatalogLink(baseCartUrl: string, fid: string): string | null {
  const normalizedFid = fid.trim();
  if (!normalizedFid) return null;
  try {
    const url = new URL(baseCartUrl, window.location.origin);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    url.search = "";
    url.searchParams.set("fid", normalizedFid);
    return url.toString();
  } catch {
    return null;
  }
}

function findConfigInSnapshot(snapshot: Config[], target: Config): Config | null {
  const byId = snapshot.find((cfg) => cfg.id === target.id);
  if (byId) return byId;
  const targetRegion = target.regionId ?? "";
  return (
    snapshot.find(
      (cfg) =>
        cfg.name === target.name &&
        cfg.countryId === target.countryId &&
        (cfg.regionId ?? "") === targetRegion,
    ) ?? null
  );
}

function formatVersionDisplay(version: string | null | undefined): string {
  const v = (version ?? "").trim();
  if (!v) return "-";
  if (/^v/i.test(v)) return v;
  if (/^\d+\.\d+\.\d+$/.test(v)) return `v${v}`;
  return v;
}

function asGitVersionRef(version: string | null | undefined): string | null {
  const v = (version ?? "").trim();
  if (!v) return null;
  if (/^v\d+\.\d+\.\d+$/.test(v)) return v;
  if (/^\d+\.\d+\.\d+$/.test(v)) return `v${v}`;
  return null;
}

function encodeGitRefForPath(ref: string): string {
  return encodeURIComponent(ref).replaceAll("%2F", "/");
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { cache: "no-store", ...init });
  const text = await res.text();
  const tryJson = () => parseJsonText(text);

  if (!res.ok) {
    const body = tryJson() as ApiError | null;
    const msg = body?.error?.message ?? `HTTP ${res.status}`;
    throw new ApiHttpError(res.status, res.statusText, msg);
  }

  const body = tryJson() as T | null;
  if (body === null) throw new Error("Invalid JSON response");
  return body;
}

function parseJsonText(text: string): unknown | null {
  if (!text) return null;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

function formatResponseErrorMessage(res: Response, text: string, parsed: unknown): string {
  if (parsed && typeof parsed === "object" && "error" in parsed) {
    const maybeError = parsed as ApiError;
    if (maybeError.error?.message) return maybeError.error.message;
  }

  const trimmed = text.trim();
  if (trimmed) {
    return res.statusText ? `${res.status} ${res.statusText}: ${trimmed}` : trimmed;
  }

  return res.statusText ? `${res.status} ${res.statusText}` : `HTTP ${res.status}`;
}

function formatMoney(m: Money): string {
  const periodLabel = (() => {
    if (m.period === "month") return "月";
    if (m.period === "year") return "年";
    return m.period;
  })();
  if (m.currency === "CNY") {
    return `¥${m.amount.toFixed(2)} / ${periodLabel}`;
  }
  return `${m.amount.toFixed(2)} ${m.currency}/${m.period}`;
}

function formatRelativeTime(iso: string, nowMs: number): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return iso;
  const diffS = Math.max(0, Math.floor((nowMs - t) / 1000));
  if (diffS < 10) return "刚刚";
  if (diffS < 60) return `${diffS} 秒前`;
  const diffM = Math.floor(diffS / 60);
  if (diffM < 60) return `${diffM} 分钟前`;
  const diffH = Math.floor(diffM / 60);
  if (diffH < 48) return `${diffH} 小时前`;
  const diffD = Math.floor(diffH / 24);
  return `${diffD} 天前`;
}

function clampNumber(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

function formatAbsoluteTime(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return iso;
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(t));
}

function formatLocalTime(iso: string | null | undefined): string {
  if (!iso) return "-";
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return iso;
  return new Date(t).toLocaleString();
}

function formatLazycatTraffic(traffic?: LazycatTrafficView | null): string {
  if (!traffic) return "—";
  return `${formatLazycatTrafficValue(traffic.usedGb)} / ${formatLazycatTrafficValue(traffic.limitGb)} ${traffic.display?.trim() || "GB"}`;
}

function lazycatMachineStatusClass(status: string): string {
  const normalized = status.trim().toLowerCase();
  if (
    normalized.includes("active") ||
    normalized.includes("normal") ||
    normalized.includes("running") ||
    normalized.includes("正常")
  ) {
    return "pill sm badge on";
  }
  if (
    normalized.includes("pending") ||
    normalized.includes("processing") ||
    normalized.includes("待")
  ) {
    return "pill sm badge warn";
  }
  return "pill sm badge err";
}

function notificationKindLabel(kind: string): string {
  if (kind.startsWith("monitoring.")) {
    const labels = kind
      .slice("monitoring.".length)
      .split("+")
      .map((part) => part.trim())
      .filter(Boolean)
      .map((part) => {
        if (part === "restock") return "补货";
        if (part === "price") return "价格变动";
        if (part === "config") return "配置更新";
        return part;
      });
    if (labels.length > 0) return labels.join(" + ");
  }
  if (kind.includes("partition")) return "分区上新机";
  if (kind.includes("site")) return "全站上新机";
  if (kind.includes("delisted")) return "已下架";
  return kind || "通知";
}

function notificationStatusClass(status: string): string {
  if (status === "success") return "pill sm center notification-status ok";
  if (status === "partial_success") return "pill sm center notification-status warn";
  if (status === "error") return "pill sm center notification-status err";
  if (status === "skipped") return "pill sm center notification-status skip";
  return "pill sm center notification-status";
}

function notificationStatusLabel(status: string): string {
  if (status === "success") return "成功";
  if (status === "partial_success") return "部分成功";
  if (status === "error") return "失败";
  if (status === "skipped") return "跳过";
  if (status === "pending") return "发送中";
  if (status === "not_sent") return "未发送";
  return status || "未知";
}

function mergeNotificationRecordLists(
  current: NotificationRecord[],
  incoming: NotificationRecord[],
): NotificationRecord[] {
  const byId = new Map<string, NotificationRecord>();
  for (const item of current) byId.set(item.id, item);
  for (const item of incoming) byId.set(item.id, item);
  return Array.from(byId.values()).sort((a, b) => {
    const order = b.createdAt.localeCompare(a.createdAt);
    if (order !== 0) return order;
    return b.id.localeCompare(a.id);
  });
}

function useInventoryHistory(configIds: string[], refreshKey: string) {
  const ids = useMemo(() => {
    const out: string[] = [];
    const seen = new Set<string>();
    for (const raw of configIds) {
      const id = raw.trim();
      if (!id || seen.has(id)) continue;
      seen.add(id);
      out.push(id);
      if (out.length >= 200) break;
    }
    return out;
  }, [configIds]);

  const [window, setWindow] = useState<InventoryHistoryResponse["window"] | null>(null);
  const [byId, setById] = useState<Map<string, InventoryHistoryPoint[]>>(() => new Map());

  useEffect(() => {
    void refreshKey;
    if (ids.length === 0) {
      setWindow(null);
      setById(new Map());
      return;
    }

    let cancelled = false;

    async function run() {
      try {
        const res = await api<InventoryHistoryResponse>("/api/inventory/history", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ configIds: ids }),
        });
        if (cancelled) return;
        const m = new Map<string, InventoryHistoryPoint[]>();
        for (const s of res.series) m.set(s.configId, s.points);
        setWindow(res.window);
        setById(m);
      } catch {
        if (!cancelled) {
          setWindow(null);
          setById(new Map());
        }
      }
    }

    void run();
    return () => {
      cancelled = true;
    };
  }, [ids, refreshKey]);

  return { window, byId };
}

function TrendBackground({
  points,
  window,
}: {
  points: InventoryHistoryPoint[] | undefined;
  window: InventoryHistoryResponse["window"] | null;
}) {
  if (!window) return null;

  const fromMs = Date.parse(window.from);
  const toMs = Date.parse(window.to);
  if (!Number.isFinite(fromMs) || !Number.isFinite(toMs) || toMs <= fromMs) return null;

  const width = 100;
  const height = 40;
  const baselineY = height;

  const rawPoints = points ?? [];
  if (rawPoints.length === 0) {
    return (
      <svg
        className="trend-bg"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <path className="trend-empty" d={`M 0 ${height - 6} H ${width}`} />
      </svg>
    );
  }

  const sorted = rawPoints
    .map((p) => ({ ...p, ms: Date.parse(p.tsMinute) }))
    .filter((p) => Number.isFinite(p.ms))
    .sort((a, b) => a.ms - b.ms);

  if (sorted.length === 0) {
    return (
      <svg
        className="trend-bg"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <path className="trend-empty" d={`M 0 ${height - 6} H ${width}`} />
      </svg>
    );
  }

  const scaled = sorted.map((p) => {
    const t = clampNumber((p.ms - fromMs) / (toMs - fromMs), 0, 1);
    const x = t * width;
    const clamped = clampNumber(p.quantity, 0, 10);
    const y = baselineY - (clamped / 10) * height;
    return { x, y, raw: p.quantity };
  });

  const x0 = scaled[0].x;
  const y0 = scaled[0].y;

  let lineD = `M ${x0} ${y0}`;
  let areaD = `M ${x0} ${baselineY} L ${x0} ${y0}`;
  let overD = "";

  for (let i = 1; i < scaled.length; i += 1) {
    const { x, y } = scaled[i];
    lineD += ` H ${x} V ${y}`;
    areaD += ` H ${x} V ${y}`;

    const prev = scaled[i - 1];
    if (prev.raw > 10 && x > prev.x) {
      overD += `M ${prev.x} ${prev.y} H ${x} `;
    }
    if ((prev.raw > 10 || scaled[i].raw > 10) && prev.y !== y) {
      overD += `M ${x} ${prev.y} V ${y} `;
    }
  }

  const last = scaled[scaled.length - 1];
  if (width > last.x) {
    lineD += ` H ${width}`;
    areaD += ` H ${width}`;
    if (last.raw > 10) {
      overD += `M ${last.x} ${last.y} H ${width} `;
    }
  }
  areaD += ` L ${width} ${baselineY} Z`;

  return (
    <svg
      className="trend-bg"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <path className="trend-area" d={areaD} />
      <path className="trend-line" d={lineD} />
      {overD ? <path className="trend-over" d={overD} /> : null}
    </svg>
  );
}

function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) {
    out[i] = raw.charCodeAt(i);
  }
  return out;
}

export function App() {
  const [route, setRoute] = useState<Route>(() => getRoute());
  const [notificationTargetId, setNotificationTargetId] = useState<string | null>(() =>
    getNotificationTargetId(),
  );
  const [bootstrap, setBootstrap] = useState<BootstrapResponse | null>(null);
  const [about, setAbout] = useState<AboutResponse | null>(null);
  const [aboutLoading, setAboutLoading] = useState<boolean>(false);
  const [aboutError, setAboutError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [syncAlert, setSyncAlert] = useState<string | null>(null);
  const [catalogRefresh, setCatalogRefresh] = useState<CatalogRefreshStatus | null>(null);
  const [recentListed24h, setRecentListed24h] = useState<Config[]>([]);
  const [archiveFilterMode, setArchiveFilterMode] = useState<ArchiveFilterMode>("active");
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const [opsRange, setOpsRange] = useState<OpsRange>("24h");
  const [opsFollow, setOpsFollow] = useState<boolean>(true);
  const [opsHelpOpen, setOpsHelpOpen] = useState<boolean>(false);
  const [orderGuardDialog, setOrderGuardDialog] = useState<OrderGuardDialog | null>(null);
  const [opsSseUi, setOpsSseUi] = useState<OpsSseUi>({
    status: "reconnecting",
    replayWindowSeconds: null,
    lastEventId: null,
    lastReset: null,
  });
  const [partitionRefreshStates, setPartitionRefreshStates] = useState<
    Record<string, PartitionRefreshState>
  >({});
  const lastTerminalJobIdRef = useRef<string | null>(null);
  const orderGuardReqSeqRef = useRef<number>(0);
  const catalogBackfillPendingRef = useRef<boolean>(false);
  const partitionRefreshTimersRef = useRef<Record<string, number>>({});

  const applyProductsResponse = useCallback((res: ProductsResponse) => {
    setBootstrap((prev) =>
      prev
        ? {
            ...prev,
            catalog: {
              ...prev.catalog,
              configs: res.configs,
              fetchedAt: res.fetchedAt,
            },
            monitoring: {
              ...prev.monitoring,
              enabledConfigIds: res.configs.filter((c) => c.monitorEnabled).map((c) => c.id),
            },
          }
        : prev,
    );
  }, []);

  const hasBootstrap = bootstrap !== null;
  const applyLazycatAccount = useCallback((account: LazycatAccountView) => {
    setBootstrap((prev) => (prev ? { ...prev, lazycat: account } : prev));
  }, []);

  const countriesById = useMemo(() => {
    const m = new Map<string, Country>();
    for (const c of bootstrap?.catalog.countries ?? []) m.set(c.id, c);
    return m;
  }, [bootstrap]);

  const regionsById = useMemo(() => {
    const m = new Map<string, Region>();
    for (const r of bootstrap?.catalog.regions ?? []) m.set(r.id, r);
    return m;
  }, [bootstrap]);

  useEffect(() => {
    let cancelled = false;

    async function run() {
      try {
        const json = await api<BootstrapResponse>("/api/bootstrap");
        if (!cancelled) {
          setBootstrap(json);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    run();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const syncLocationUi = () => {
      setRoute(getRoute());
      setNotificationTargetId(getNotificationTargetId());
    };
    window.addEventListener("hashchange", syncLocationUi);
    window.addEventListener("popstate", syncLocationUi);
    return () => {
      window.removeEventListener("hashchange", syncLocationUi);
      window.removeEventListener("popstate", syncLocationUi);
    };
  }, []);

  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 10_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(
    () => () => {
      for (const timer of Object.values(partitionRefreshTimersRef.current)) {
        window.clearTimeout(timer);
      }
      partitionRefreshTimersRef.current = {};
    },
    [],
  );

  const refreshBootstrapSilently = useCallback(async () => {
    try {
      const json = await api<BootstrapResponse>("/api/bootstrap");
      setBootstrap(json);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const refreshLazycatAccount = useCallback(async () => {
    const json = await api<LazycatAccountView>("/api/lazycat/account");
    applyLazycatAccount(json);
    return json;
  }, [applyLazycatAccount]);

  const loginLazycat = useCallback(
    async (email: string, password: string) => {
      const json = await api<LazycatAccountView>("/api/lazycat/account", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ email, password }),
      });
      applyLazycatAccount(json);
      return json;
    },
    [applyLazycatAccount],
  );

  const syncLazycat = useCallback(async () => {
    const json = await api<LazycatAccountView>("/api/lazycat/sync", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({}),
    });
    applyLazycatAccount(json);
    return json;
  }, [applyLazycatAccount]);

  const disconnectLazycat = useCallback(async () => {
    await api<{ ok: boolean }>("/api/lazycat/account", { method: "DELETE" });
    applyLazycatAccount(emptyLazycatAccount());
  }, [applyLazycatAccount]);

  const refreshAbout = useCallback(async (force: boolean) => {
    setAboutLoading(true);
    setAboutError(null);
    try {
      const json = await api<AboutResponse>(force ? "/api/about?force=1" : "/api/about");
      setAbout(json);
      setAboutError(null);
    } catch (e) {
      setAboutError(e instanceof Error ? e.message : String(e));
    } finally {
      setAboutLoading(false);
    }
  }, []);

  const refreshMonitoringSilently = useCallback(async () => {
    try {
      const res = await api<MonitoringResponse>("/api/monitoring");
      setRecentListed24h(res.recentListed24h);
    } catch {
      // Ignore monitoring refresh errors.
    }
  }, []);

  const clearPartitionRefreshState = useCallback((partitionKey: string) => {
    const timer = partitionRefreshTimersRef.current[partitionKey];
    if (typeof timer === "number") {
      window.clearTimeout(timer);
      delete partitionRefreshTimersRef.current[partitionKey];
    }
    setPartitionRefreshStates((prev) => {
      if (!(partitionKey in prev)) return prev;
      const next = { ...prev };
      delete next[partitionKey];
      return next;
    });
  }, []);

  const schedulePartitionRefreshReset = useCallback((partitionKey: string) => {
    const existing = partitionRefreshTimersRef.current[partitionKey];
    if (typeof existing === "number") {
      window.clearTimeout(existing);
    }
    partitionRefreshTimersRef.current[partitionKey] = window.setTimeout(() => {
      delete partitionRefreshTimersRef.current[partitionKey];
      setPartitionRefreshStates((prev) => {
        if (!(partitionKey in prev)) return prev;
        const next = { ...prev };
        delete next[partitionKey];
        return next;
      });
    }, SETTINGS_TEST_SUCCESS_BUBBLE_MS);
  }, []);

  const refreshPartition = useCallback(
    async (countryId: string, regionId: string) => {
      const partitionKey = buildPartitionKey(countryId, regionId);
      clearPartitionRefreshState(partitionKey);
      setPartitionRefreshStates((prev) => ({
        ...prev,
        [partitionKey]: { kind: "running" },
      }));

      try {
        await api<PartitionRefreshResponse>("/api/catalog/refresh/partition", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ countryId, regionId }),
        });

        try {
          const nextBootstrap = await api<BootstrapResponse>("/api/bootstrap");
          setBootstrap(nextBootstrap);
          const products = await api<ProductsResponse>("/api/products");
          applyProductsResponse(products);
          setPartitionRefreshStates((prev) => ({
            ...prev,
            [partitionKey]: { kind: "success", message: "已刷新" },
          }));
        } catch {
          setPartitionRefreshStates((prev) => ({
            ...prev,
            [partitionKey]: {
              kind: "success",
              message: "已刷新，页面数据同步稍后重试",
            },
          }));
        }

        schedulePartitionRefreshReset(partitionKey);
      } catch (e) {
        setPartitionRefreshStates((prev) => ({
          ...prev,
          [partitionKey]: {
            kind: "error",
            message: e instanceof Error ? e.message : String(e),
          },
        }));
      }
    },
    [applyProductsResponse, clearPartitionRefreshState, schedulePartitionRefreshReset],
  );

  useEffect(() => {
    if (!hasBootstrap) return;
    void refreshAbout(false);
  }, [hasBootstrap, refreshAbout]);

  useEffect(() => {
    if (!hasBootstrap) return;
    if (route !== "monitoring") return;
    void refreshMonitoringSilently();
  }, [hasBootstrap, refreshMonitoringSilently, route]);
  useEffect(() => {
    if (!hasBootstrap) return;
    if (route !== "monitoring") return;

    const id = window.setInterval(() => void refreshMonitoringSilently(), 15_000);
    return () => window.clearInterval(id);
  }, [hasBootstrap, refreshMonitoringSilently, route]);

  useEffect(() => {
    if (!bootstrap?.lazycat.connected) return;
    if (!(bootstrap.lazycat.state === "syncing" || bootstrap.lazycat.state === "authenticating")) {
      return;
    }
    const id = window.setInterval(() => void refreshBootstrapSilently(), 3_000);
    return () => window.clearInterval(id);
  }, [bootstrap?.lazycat.connected, bootstrap?.lazycat.state, refreshBootstrapSilently]);

  useEffect(() => {
    if (!hasBootstrap) return;

    const es = new EventSource("/api/catalog/refresh/events");
    const onEvent = (ev: MessageEvent) => {
      try {
        const st = JSON.parse(ev.data) as CatalogRefreshStatus;
        setCatalogRefresh(st);
        if (st.state === "running") setSyncAlert(null);

        if (st.state === "success" || st.state === "error") {
          if (lastTerminalJobIdRef.current !== st.jobId) {
            lastTerminalJobIdRef.current = st.jobId;
            if (st.state === "success") {
              void refreshBootstrapSilently();
              void refreshMonitoringSilently();
            } else if (st.state === "error") {
              setSyncAlert(st.message ?? "刷新失败");
            }
          }
        }
      } catch {
        // Ignore parse errors.
      }
    };

    es.addEventListener("catalog.refresh", onEvent as EventListener);
    return () => {
      es.removeEventListener("catalog.refresh", onEvent as EventListener);
      es.close();
    };
  }, [hasBootstrap, refreshBootstrapSilently, refreshMonitoringSilently]);

  const catalogCountriesLen = bootstrap?.catalog.countries.length ?? 0;
  const catalogRegionsLen = bootstrap?.catalog.regions.length ?? 0;

  useEffect(() => {
    if (!hasBootstrap) return;
    const missingCatalogTopology = catalogCountriesLen === 0;
    if (missingCatalogTopology) {
      catalogBackfillPendingRef.current = true;
    }
    if (!missingCatalogTopology && !catalogBackfillPendingRef.current) {
      return;
    }

    let cancelled = false;
    let attempts = 0;
    const topologyRetryLimit = 45;
    let noticeGraceRetriesRemaining = 1;
    let timeoutId: number | null = null;

    const schedule = (delayMs: number) => {
      if (timeoutId !== null) window.clearTimeout(timeoutId);
      timeoutId = window.setTimeout(() => void retry(), delayMs);
    };

    async function retry() {
      attempts += 1;
      try {
        const json = await api<BootstrapResponse>("/api/bootstrap");
        if (cancelled) return;
        // Only backfill missing catalog metadata; avoid clobbering newer in-memory updates.
        setBootstrap((prev) => {
          if (!prev) return json;

          const prevCatalog = prev.catalog;
          const jsonCatalog = json.catalog;

          const hasCountries = prevCatalog.countries.length > 0;
          const hasRegions = prevCatalog.regions.length > 0;
          const hasRegionNotices = prevCatalog.regionNotices.length > 0;
          if (hasCountries && hasRegions && hasRegionNotices) return prev;

          const canBackfillCountries = !hasCountries && jsonCatalog.countries.length > 0;
          const canBackfillRegions = !hasRegions && jsonCatalog.regions.length > 0;
          const canBackfillRegionNotices =
            !hasRegionNotices && jsonCatalog.regionNotices.length > 0;
          if (!canBackfillCountries && !canBackfillRegions && !canBackfillRegionNotices) {
            return prev;
          }

          const nextCountries = canBackfillCountries
            ? jsonCatalog.countries
            : prevCatalog.countries;
          const nextRegions = canBackfillRegions ? jsonCatalog.regions : prevCatalog.regions;
          const nextRegionNotices = canBackfillRegionNotices
            ? jsonCatalog.regionNotices
            : prevCatalog.regionNotices;

          return {
            ...prev,
            catalog: {
              ...prevCatalog,
              countries: nextCountries,
              regions: nextRegions,
              regionNotices: nextRegionNotices,
            },
          };
        });

        const topologyReady = json.catalog.countries.length > 0;
        const noticesReady = json.catalog.regionNotices.length > 0;
        const reachedAttemptCap = attempts >= topologyRetryLimit;
        if (!topologyReady && !reachedAttemptCap) {
          // A full bootstrap can take tens of seconds on cold start; keep backfilling topology longer.
          schedule(Math.min(3_000, 900 + attempts * 250));
          return;
        }

        if (
          topologyReady &&
          !noticesReady &&
          noticeGraceRetriesRemaining > 0 &&
          !reachedAttemptCap
        ) {
          noticeGraceRetriesRemaining -= 1;
          schedule(900 + attempts * 250);
          return;
        }

        catalogBackfillPendingRef.current = false;
      } catch {
        if (cancelled) return;
        if (attempts < topologyRetryLimit) {
          schedule(Math.min(3_000, 900 + attempts * 250));
        } else {
          catalogBackfillPendingRef.current = false;
        }
      }
    }

    schedule(700);
    return () => {
      cancelled = true;
      if (timeoutId !== null) window.clearTimeout(timeoutId);
    };
  }, [catalogCountriesLen, hasBootstrap]);

  useEffect(() => {
    if (!hasBootstrap) return;
    if (route !== "monitoring" && route !== "products") return;

    let cancelled = false;

    async function run() {
      try {
        const res = await api<ProductsResponse>("/api/products");
        if (cancelled) return;
        applyProductsResponse(res);
      } catch {
        // Ignore errors on route switch to avoid UI flicker.
      }
    }

    void run();
    return () => {
      cancelled = true;
    };
  }, [applyProductsResponse, hasBootstrap, route]);

  useEffect(() => {
    if (!hasBootstrap) return;
    if (route !== "monitoring" && route !== "products") return;

    let cancelled = false;

    async function refresh() {
      try {
        const res = await api<ProductsResponse>("/api/products");
        if (cancelled) return;
        applyProductsResponse(res);
      } catch {
        // Ignore focus refresh errors.
      }
    }

    const onFocus = () => void refresh();
    const onVis = () => {
      if (document.visibilityState === "visible") void refresh();
    };

    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVis);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [applyProductsResponse, hasBootstrap, route]);

  useEffect(() => {
    const intervalSeconds = bootstrap?.monitoring.poll.intervalSeconds;
    if (!intervalSeconds) return;

    let cancelled = false;
    const intervalMs = Math.max(10_000, Math.min(30_000, Math.floor((intervalSeconds * 1000) / 6)));

    async function tick() {
      try {
        const res = await api<ProductsResponse>("/api/products");
        if (cancelled) return;
        applyProductsResponse(res);
      } catch {
        // Ignore background refresh errors to avoid UI flicker.
      }
    }

    const id = window.setInterval(() => void tick(), intervalMs);
    void tick();
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [applyProductsResponse, bootstrap?.monitoring.poll.intervalSeconds]);

  async function startCatalogRefresh() {
    if (catalogRefresh?.state === "running") return;
    setSyncAlert(null);
    try {
      const st = await api<CatalogRefreshStatus>("/api/catalog/refresh", { method: "POST" });
      setCatalogRefresh(st);
    } catch (e) {
      if (e instanceof ApiHttpError && e.status === 429) {
        setSyncAlert("刷新太频繁，请稍后再试。");
      } else {
        setSyncAlert(e instanceof Error ? e.message : String(e));
      }
    }
  }

  async function toggleMonitoring(configId: string, enabled: boolean) {
    if (!bootstrap) return;
    await api(`/api/monitoring/configs/${encodeURIComponent(configId)}`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ enabled }),
    });
    setBootstrap({
      ...bootstrap,
      catalog: {
        ...bootstrap.catalog,
        configs: bootstrap.catalog.configs.map((c) =>
          c.id === configId ? { ...c, monitorEnabled: enabled } : c,
        ),
      },
      monitoring: {
        ...bootstrap.monitoring,
        enabledConfigIds: enabled
          ? Array.from(new Set([...bootstrap.monitoring.enabledConfigIds, configId]))
          : bootstrap.monitoring.enabledConfigIds.filter((id) => id !== configId),
      },
    });
  }

  async function togglePartitionMonitoring(
    countryId: string,
    regionId: string | null,
    enabled: boolean,
  ) {
    if (!bootstrap) return;
    await api("/api/monitoring/partitions", {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ countryId, regionId, enabled }),
    });

    const partitionKey = buildPartitionKey(countryId, regionId);
    const nextPartitions = enabled
      ? [
          ...bootstrap.monitoring.enabledPartitions.filter(
            (partition) =>
              buildPartitionKey(partition.countryId, partition.regionId ?? null) !== partitionKey,
          ),
          { countryId, regionId },
        ]
      : bootstrap.monitoring.enabledPartitions.filter(
          (partition) =>
            buildPartitionKey(partition.countryId, partition.regionId ?? null) !== partitionKey,
        );

    setBootstrap({
      ...bootstrap,
      monitoring: {
        ...bootstrap.monitoring,
        enabledPartitions: nextPartitions,
      },
    });
  }

  async function saveSettings(
    next: SettingsView & { telegramBotToken?: string | null },
  ): Promise<SettingsView> {
    if (!bootstrap) throw new Error("settings not ready");
    const updated = await api<SettingsView>("/api/settings", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        poll: next.poll,
        siteBaseUrl: next.siteBaseUrl,
        catalogRefresh: next.catalogRefresh,
        monitoringEvents: next.monitoringEvents,
        notifications: {
          telegram: {
            enabled: next.notifications.telegram.enabled,
            botToken: next.telegramBotToken ?? null,
            targets: next.notifications.telegram.targets ?? [],
          },
          webPush: { enabled: next.notifications.webPush.enabled },
        },
      }),
    });
    setSyncAlert(null);
    setBootstrap({
      ...bootstrap,
      settings: updated,
      monitoring: {
        ...bootstrap.monitoring,
        poll: {
          intervalSeconds: updated.poll.intervalMinutes * 60,
          jitterPct: updated.poll.jitterPct,
        },
      },
    });
    return updated;
  }

  const openOrderUrl = useCallback((url: string) => {
    window.open(url, "_blank", "noopener,noreferrer");
  }, []);

  const archiveDelistedConfigs = useCallback(async () => {
    const archived = await api<ArchiveDelistedResponse>("/api/products/archive/delisted", {
      method: "POST",
    });
    try {
      const products = await api<ProductsResponse>("/api/products");
      applyProductsResponse(products);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSyncAlert(`归档已完成，但产品列表刷新失败：${msg}`);
    }
    void refreshMonitoringSilently();
    return archived;
  }, [applyProductsResponse, refreshMonitoringSilently]);

  const dismissOrderGuardDialog = useCallback(() => {
    orderGuardReqSeqRef.current += 1;
    setOrderGuardDialog(null);
  }, []);

  const guardAndOpenOrder = useCallback(
    async (cfg: Config, orderLink: OrderLink) => {
      const isOutOfStock = cfg.monitorSupported && cfg.inventory.quantity <= 0;
      if (!isOutOfStock) {
        openOrderUrl(orderLink.url);
        return;
      }

      const reqSeq = orderGuardReqSeqRef.current + 1;
      orderGuardReqSeqRef.current = reqSeq;

      setOrderGuardDialog({
        configId: cfg.id,
        configName: cfg.name,
        orderUrl: orderLink.url,
        checking: true,
        initialQty: cfg.inventory.quantity,
        latestQty: null,
        latestCheckedAt: cfg.inventory.checkedAt,
        checkError: null,
      });

      try {
        const res = await api<ProductsResponse>("/api/products");
        if (orderGuardReqSeqRef.current !== reqSeq) return;
        applyProductsResponse(res);

        const fresh = findConfigInSnapshot(res.configs, cfg);
        const latestQty = fresh?.inventory.quantity ?? null;
        const latestCheckedAt = fresh?.inventory.checkedAt ?? res.fetchedAt;
        const hasStockNow =
          (fresh?.inventory.status ?? "unknown") === "available" && (latestQty ?? 0) > 0;

        if (hasStockNow) {
          setOrderGuardDialog(null);
          openOrderUrl(orderLink.url);
          return;
        }

        setOrderGuardDialog((prev) =>
          prev && prev.configId === cfg.id
            ? {
                ...prev,
                checking: false,
                latestQty,
                latestCheckedAt,
                checkError: null,
              }
            : prev,
        );
      } catch (e) {
        if (orderGuardReqSeqRef.current !== reqSeq) return;
        const msg = e instanceof Error ? e.message : String(e);
        setOrderGuardDialog((prev) =>
          prev && prev.configId === cfg.id
            ? {
                ...prev,
                checking: false,
                checkError: msg,
              }
            : prev,
        );
      }
    },
    [applyProductsResponse, openOrderUrl],
  );

  const clearNotificationTargetId = useCallback(() => {
    setNotificationTargetId(null);
    const url = new URL(window.location.href);
    if (!url.searchParams.has("notification")) return;
    url.searchParams.delete("notification");
    window.history.replaceState(
      window.history.state,
      "",
      `${url.pathname}${url.search}${url.hash}`,
    );
  }, []);

  const title = `Catnap • ${routeTitle(route)}`;
  const subtitle = route === "monitoring" ? null : routeSubtitle(route);
  const repoUrl = about?.repoUrl?.trim() ? about.repoUrl.trim() : null;
  const repoBaseUrl = repoUrl ? repoUrl.replace(/\/+$/, "") : null;
  const versionRaw = about?.version ?? import.meta.env.VITE_APP_VERSION;
  const versionDisplay = formatVersionDisplay(versionRaw);
  const versionRef = asGitVersionRef(versionRaw);
  const versionHref =
    repoBaseUrl && versionRef
      ? `${repoBaseUrl}/releases/tag/${encodeGitRefForPath(versionRef)}`
      : null;
  const updateHref = about?.update?.latestUrl ?? null;
  const updatePill =
    about?.update.updateAvailable && about.update.latestVersion ? (
      <a
        className="pill badge warn topbar-update-pill"
        href={updateHref ?? "#settings"}
        {...(updateHref && /^https?:\/\//.test(updateHref)
          ? { target: "_blank", rel: "noopener noreferrer" }
          : {})}
        title={
          updateHref
            ? `有新版本：${formatVersionDisplay(about.update.latestVersion)}`
            : "有新版本：请在系统设置中查看"
        }
      >
        <span className="topbar-update-pill-dot" aria-hidden="true" />
        <span>{`有新版本 ${formatVersionDisplay(about.update.latestVersion)}`}</span>
      </a>
    ) : null;
  const sidebar = (
    <>
      <div className="sidebar-title">导航</div>
      <a className={route === "monitoring" ? "nav-item active" : "nav-item"} href="#monitoring">
        库存监控
      </a>
      <a className={route === "products" ? "nav-item active" : "nav-item"} href="#products">
        全部产品
      </a>
      <a
        className={route === "notifications" ? "nav-item active" : "nav-item"}
        href="#notifications"
      >
        通知记录
      </a>
      <a className={route === "machines" ? "nav-item active" : "nav-item"} href="#machines">
        机器资产
      </a>
      <a className={route === "settings" ? "nav-item active" : "nav-item"} href="#settings">
        系统设置
      </a>
      <a className={route === "logs" ? "nav-item active" : "nav-item"} href="#logs">
        日志
      </a>
      <a className={route === "ops" ? "nav-item active" : "nav-item"} href="#ops">
        采集观测台
      </a>

      <div className="sidebar-meta">
        <div className="sidebar-meta-divider" aria-hidden="true" />
        <div className="sidebar-meta-top">
          {versionHref ? (
            <a
              className="sidebar-meta-version"
              href={versionHref}
              target="_blank"
              rel="noopener noreferrer"
              title={`Version: ${versionDisplay}`}
            >
              <span className="mono">{versionDisplay}</span>
            </a>
          ) : (
            <span className="mono">{versionDisplay}</span>
          )}
          {repoBaseUrl ? (
            <a
              className="sidebar-meta-repo"
              href={repoBaseUrl}
              target="_blank"
              rel="noopener noreferrer"
              title={repoBaseUrl}
            >
              <span className="mono">GitHub</span>
            </a>
          ) : (
            <span className="mono muted">-</span>
          )}
        </div>
      </div>
    </>
  );

  const isRefreshing = catalogRefresh?.state === "running";
  const refreshIconClass = isRefreshing
    ? "sync-icon spin"
    : catalogRefresh?.state === "error"
      ? "sync-icon err"
      : catalogRefresh?.state === "success"
        ? "sync-icon ok"
        : "sync-icon";
  const refreshButtonText = isRefreshing
    ? `刷新中（${catalogRefresh?.done ?? 0}/${catalogRefresh?.total || "?"}）`
    : catalogRefresh?.state === "success"
      ? "已刷新"
      : catalogRefresh?.state === "error"
        ? "刷新失败"
        : "立即刷新";

  const actions =
    route === "ops" ? (
      <>
        <OpsSseIndicator sse={opsSseUi} />
        <OpsRangePill range={opsRange} onChange={setOpsRange} />
        <button
          type="button"
          className={`pill ${opsFollow ? "on" : ""}`}
          onClick={() => setOpsFollow((v) => !v)}
        >
          {opsFollow ? "跟随：开" : "跟随：关"}
        </button>
        <button type="button" className="pill" onClick={() => setOpsHelpOpen(true)}>
          帮助
        </button>
        {updatePill}
        <ThemeMenu />
      </>
    ) : (
      <>
        {isRefreshing ? (
          <span className="pill">{`目录刷新中（${catalogRefresh?.done ?? 0}/${catalogRefresh?.total || "?"}）`}</span>
        ) : null}
        {route === "products" ? (
          <button
            type="button"
            className="pill"
            disabled={loading || isRefreshing}
            title="按低压策略刷新已知目录页（优先 cache hit）"
            onClick={() => void startCatalogRefresh()}
          >
            {refreshButtonText}
          </button>
        ) : route === "monitoring" ? (
          bootstrap ? (
            <>
              <span className="pill">
                最近刷新：{formatRelativeTime(bootstrap.catalog.fetchedAt, nowMs)}
              </span>
              <button
                type="button"
                className="pill"
                disabled={isRefreshing}
                title="按低压策略刷新已知目录页（优先 cache hit）"
                onClick={() => void startCatalogRefresh()}
              >
                <span className={refreshIconClass} aria-hidden="true">
                  {isRefreshing ? (
                    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                      <path
                        fill="currentColor"
                        d="M12 4a8 8 0 0 1 7.9 6.7a1 1 0 1 1-2 .3A6 6 0 1 0 18 12a1 1 0 1 1 2 0a8 8 0 1 1-8-8"
                      />
                    </svg>
                  ) : catalogRefresh?.state === "error" ? (
                    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                      <path fill="currentColor" d="M1 21h22L12 2zm12-3h-2v-2h2zm0-4h-2v-4h2z" />
                    </svg>
                  ) : catalogRefresh?.state === "success" ? (
                    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                      <path
                        fill="currentColor"
                        d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2m-1 14l-4-4l1.4-1.4L11 13.2l5.6-5.6L18 9z"
                      />
                    </svg>
                  ) : (
                    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                      <path
                        fill="currentColor"
                        d="M12 6V3L8 7l4 4V8a4 4 0 1 1-4 4H6a6 6 0 1 0 6-6"
                      />
                    </svg>
                  )}
                </span>
                {refreshButtonText}
              </button>
            </>
          ) : null
        ) : null}
        {updatePill}
        <ThemeMenu />
      </>
    );

  return (
    <AppShell
      title={title}
      subtitle={subtitle}
      actions={actions}
      sidebar={sidebar}
      contentClassName={route === "ops" ? "ops-content" : undefined}
      scrollInnerClassName={route === "ops" ? "fill" : undefined}
    >
      {route !== "ops" ? loading ? <p className="muted">Loading...</p> : null : null}
      {route !== "ops" ? error ? <p className="error">{error}</p> : null : null}

      {route === "ops" ? (
        <OpsView
          range={opsRange}
          onRangeChange={setOpsRange}
          follow={opsFollow}
          onFollowChange={setOpsFollow}
          helpOpen={opsHelpOpen}
          onHelpOpenChange={setOpsHelpOpen}
          onSseUiChange={setOpsSseUi}
        />
      ) : bootstrap ? (
        route === "products" ? (
          <ProductsView
            bootstrap={bootstrap}
            countriesById={countriesById}
            regionsById={regionsById}
            orderBaseUrl={bootstrap.catalog.source.url}
            archiveFilterMode={archiveFilterMode}
            onArchiveFilterModeChange={setArchiveFilterMode}
            onArchiveDelisted={archiveDelistedConfigs}
            onToggle={toggleMonitoring}
            onTogglePartition={togglePartitionMonitoring}
            onRefreshPartition={refreshPartition}
            partitionRefreshStates={partitionRefreshStates}
            onOpenOrder={guardAndOpenOrder}
          />
        ) : route === "notifications" ? (
          <NotificationsView
            targetRecordId={notificationTargetId}
            nowMs={nowMs}
            onTargetHandled={clearNotificationTargetId}
          />
        ) : route === "machines" ? (
          <MachinesView
            bootstrap={bootstrap}
            onSync={syncLazycat}
            onRefreshAccount={refreshLazycatAccount}
          />
        ) : route === "settings" ? (
          <SettingsViewPanel
            bootstrap={bootstrap}
            about={about}
            aboutLoading={aboutLoading}
            aboutError={aboutError}
            onCheckUpdate={async () => refreshAbout(true)}
            onSave={saveSettings}
            onLazycatLogin={loginLazycat}
            onLazycatDisconnect={disconnectLazycat}
            onLazycatSync={syncLazycat}
            onRefreshLazycatAccount={refreshLazycatAccount}
          />
        ) : route === "logs" ? (
          <LogsView />
        ) : (
          <MonitoringView
            bootstrap={bootstrap}
            countriesById={countriesById}
            regionsById={regionsById}
            orderBaseUrl={bootstrap.catalog.source.url}
            nowMs={nowMs}
            syncAlert={syncAlert}
            recentListed24h={recentListed24h}
            onDismissSyncAlert={() => setSyncAlert(null)}
            onOpenOrder={guardAndOpenOrder}
          />
        )
      ) : null}

      {orderGuardDialog ? (
        <div
          className="ops-modal-backdrop"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) dismissOrderGuardDialog();
          }}
          role="presentation"
        >
          <dialog className="ops-modal order-guard-modal" open aria-label="库存拦截">
            <div className="ops-modal-title">库存拦截</div>
            <div className="ops-modal-body">
              <div className="muted">
                {`「${orderGuardDialog.configName}」当前库存为 ${orderGuardDialog.initialQty}，可能无法下单。`}
              </div>
              {orderGuardDialog.checking ? (
                <div className="muted">正在查询最新库存...</div>
              ) : orderGuardDialog.latestQty !== null ? (
                <div className="muted">
                  {`最新库存：${orderGuardDialog.latestQty}（更新时间：${
                    orderGuardDialog.latestCheckedAt
                      ? formatRelativeTime(orderGuardDialog.latestCheckedAt, nowMs)
                      : "—"
                  }）`}
                </div>
              ) : null}
              {orderGuardDialog.checkError ? (
                <div className="error order-guard-error">{`库存查询失败：${orderGuardDialog.checkError}`}</div>
              ) : null}
            </div>
            <div className="ops-modal-actions">
              <button type="button" className="pill" onClick={dismissOrderGuardDialog}>
                取消
              </button>
              <button
                type="button"
                className="pill warn"
                onClick={() => {
                  openOrderUrl(orderGuardDialog.orderUrl);
                  dismissOrderGuardDialog();
                }}
              >
                仍然打开
              </button>
            </div>
          </dialog>
        </div>
      ) : null}
    </AppShell>
  );
}

function buildPartitionKey(countryId: string, regionId: string | null | undefined): string {
  return `${countryId}::${regionId ?? ""}`;
}

function buildScopedRegionDomKey(countryId: string, regionId: string): string {
  return regionId.startsWith(`${countryId}-`) ? regionId : `${countryId}-${regionId}`;
}

function groupKey(c: Config): string {
  return buildPartitionKey(c.countryId, c.regionId);
}

function buildEnabledPartitionKeySet(partitions: MonitoringPartition[]): Set<string> {
  const out = new Set<string>();
  for (const partition of partitions) {
    out.add(buildPartitionKey(partition.countryId, partition.regionId ?? null));
  }
  return out;
}

function buildGroupNoticeByKey(notices: RegionNotice[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const notice of notices) {
    const text = notice.text.trim();
    if (!text) continue;
    out.set(`${notice.countryId}::${notice.regionId ?? ""}`, text);
  }
  return out;
}

function buildCountriesWithTopologyRegions(regions: Region[]): Set<string> {
  const out = new Set<string>();
  for (const region of regions) {
    out.add(region.countryId);
  }
  return out;
}

function resolveScopedGroupNotice(
  noticesByKey: Map<string, string>,
  countriesWithTopologyRegions: Set<string>,
  countryId: string,
  regionId: string | null,
): string | null {
  const notice = noticesByKey.get(buildPartitionKey(countryId, regionId));
  if (!notice) return null;
  if (regionId === null && countriesWithTopologyRegions.has(countryId)) {
    return null;
  }
  return notice;
}

function isArchivedDelisted(cfg: Config): boolean {
  return cfg.lifecycle.state === "delisted" && Boolean(cfg.lifecycle.cleanupAt);
}

function filterConfigsByArchiveMode(configs: Config[], mode: ArchiveFilterMode): Config[] {
  if (mode === "all") return configs;
  if (mode === "archived") return configs.filter((cfg) => isArchivedDelisted(cfg));
  return configs.filter((cfg) => !isArchivedDelisted(cfg));
}

type SpecCell = { label: string; value: string } | null;
type SpecSlotCell = { key: string; label: string; value: string } | { key: string; empty: true };

const SPEC_SLOTS = ["s1", "s2", "s3", "s4", "s5", "s6"] as const;
const SPEC_CARD_MAX_CELLS = 5;

function specBucket(key: string): {
  id: "cpu" | "ram" | "disk" | "bandwidth" | "traffic" | "ports" | "other";
  label: string;
  priority: number;
} {
  const k = key.trim().toLowerCase();
  if (!k) return { id: "other", label: "规格", priority: 900 };
  if (k === "cpu" || k.includes("cpu") || k.includes("核心"))
    return { id: "cpu", label: "CPU", priority: 10 };
  if (k === "ram" || k.includes("ram") || k.includes("memory") || k.includes("内存"))
    return { id: "ram", label: "内存", priority: 20 };
  if (
    k === "disk" ||
    k.includes("disk") ||
    k.includes("storage") ||
    k.includes("磁盘") ||
    k.includes("硬盘")
  )
    return { id: "disk", label: "磁盘", priority: 30 };
  if (k.includes("带宽") || k.includes("bandwidth"))
    return { id: "bandwidth", label: "带宽", priority: 40 };
  if (k.includes("流量") || k.includes("traffic") || k.includes("transfer"))
    return { id: "traffic", label: "流量", priority: 50 };
  if (k.includes("端口") || k.includes("port")) return { id: "ports", label: "端口", priority: 60 };
  return { id: "other", label: key.trim() || "规格", priority: 900 };
}

function buildSpecCells(specs: Spec[], maxCells: number): SpecCell[] {
  const picked = new Map<string, SpecCell>();
  const extras: Array<{ pr: number; idx: number; cell: SpecCell }> = [];

  for (let i = 0; i < specs.length; i += 1) {
    const rawKey = specs[i]?.key ?? "";
    const rawValue = specs[i]?.value ?? "";
    const key = rawKey.trim();
    const value = rawValue.trim();
    if (!key && !value) continue;

    const bucket = specBucket(key);
    const cell: SpecCell = { label: bucket.label, value: value || "—" };
    const bucketKey = bucket.id;

    if (bucketKey !== "other") {
      if (!picked.has(bucketKey)) picked.set(bucketKey, cell);
    } else {
      extras.push({ pr: bucket.priority, idx: i, cell });
    }
  }

  const ordered: SpecCell[] = [
    picked.get("cpu") ?? null,
    picked.get("ram") ?? null,
    picked.get("disk") ?? null,
    picked.get("bandwidth") ?? null,
    picked.get("traffic") ?? null,
    picked.get("ports") ?? null,
  ];

  extras.sort((a, b) => a.pr - b.pr || a.idx - b.idx);
  for (const e of extras) ordered.push(e.cell);

  const out: SpecCell[] = [];
  for (const c of ordered) {
    if (!c) continue;
    out.push(c);
    if (out.length >= maxCells) break;
  }

  return out;
}

const FLAGPACK_BY_ISO2 = {
  aq: flagAq,
  at: flagAt,
  ca: flagCa,
  ch: flagCh,
  de: flagDe,
  fi: flagFi,
  gb: flagGb,
  gl: flagGl,
  hk: flagHk,
  ie: flagIe,
  in: flagIn,
  is: flagIs,
  jp: flagJp,
  kp: flagKp,
  ru: flagRu,
  sg: flagSg,
  tr: flagTr,
  tw: flagTw,
  ua: flagUa,
  us: flagUs,
  vn: flagVn,
} as const;

type FlagpackIso2 = keyof typeof FLAGPACK_BY_ISO2;

const COUNTRY_NAME_TO_ISO2: Partial<Record<string, FlagpackIso2>> = {
  南极洲: "aq",
  朝鲜: "kp",
  格陵兰: "gl",
  新加坡: "sg",
  日本: "jp",
  中国台湾: "tw",
  中国香港: "hk",
  美国: "us",
  冰岛: "is",
  加拿大: "ca",
  爱尔兰: "ie",
  奥地利: "at",
  俄罗斯: "ru",
  乌克兰: "ua",
  瑞士: "ch",
  英国: "gb",
  德国: "de",
  芬兰: "fi",
  印度: "in",
  土耳其: "tr",
  越南: "vn",
};

function resolveCountryFlagWatermarkIcon(countryName: string | null) {
  if (!countryName) return null;
  if (countryName.includes("云服务器")) return null;

  const iso2 = COUNTRY_NAME_TO_ISO2[countryName];
  if (!iso2) return null;
  return FLAGPACK_BY_ISO2[iso2] ?? null;
}

export function ProductCard({
  cfg,
  countriesById,
  onToggle,
  orderLink = null,
  onOpenOrder,
  historyWindow = null,
  historyPoints,
}: {
  cfg: Config;
  countriesById: Map<string, Country>;
  onToggle: (configId: string, enabled: boolean) => void;
  orderLink?: OrderLink | null;
  onOpenOrder?: (cfg: Config, orderLink: OrderLink) => void;
  historyWindow?: InventoryHistoryResponse["window"] | null;
  historyPoints?: InventoryHistoryPoint[];
}) {
  const isCloud = !cfg.monitorSupported;
  const canOpenOrder = Boolean(orderLink?.url);
  const flagIcon = resolveCountryFlagWatermarkIcon(countriesById.get(cfg.countryId)?.name ?? null);
  const capTone =
    isCloud || (cfg.inventory.status !== "unknown" && cfg.inventory.quantity > 0) ? "" : "warn";
  const capText = isCloud
    ? null
    : cfg.inventory.status === "unknown"
      ? "?"
      : cfg.inventory.quantity > 10
        ? "10+"
        : String(cfg.inventory.quantity);
  const monitorState: MonitorToggleState = isCloud ? "disabled" : cfg.monitorEnabled ? "on" : "off";
  const foot = isCloud ? null : cfg.monitorEnabled ? "变化检测：补货 / 价格 / 配置" : null;
  const rawSpecCells = isCloud ? [] : buildSpecCells(cfg.specs, SPEC_CARD_MAX_CELLS);
  const specCells: SpecSlotCell[] = isCloud
    ? []
    : SPEC_SLOTS.slice(0, SPEC_CARD_MAX_CELLS).map((key, i) => {
        const c = rawSpecCells[i];
        return c ? { key, ...c } : { key, empty: true };
      });
  const openOrder = () => {
    if (!orderLink?.url) return;
    if (onOpenOrder) {
      onOpenOrder(cfg, orderLink);
      return;
    }
    window.open(orderLink.url, "_blank", "noopener,noreferrer");
  };
  const onCardKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!canOpenOrder) return;
    if (e.key === "Enter" || e.key === " " || e.key === "Spacebar") {
      e.preventDefault();
      openOrder();
    }
  };
  const orderOpenLabel = "打开下单页（新标签页）";
  const orderTitle = canOpenOrder ? `点击${orderOpenLabel}` : "暂无下单链接";

  return (
    <div
      className={`cfg-card ${isCloud ? "cloud" : "product"} ${
        canOpenOrder ? "card-linkable" : "card-link-disabled"
      }`}
      role={canOpenOrder ? "link" : undefined}
      tabIndex={canOpenOrder ? 0 : undefined}
      aria-label={canOpenOrder ? orderOpenLabel : undefined}
      title={orderTitle}
      onClick={canOpenOrder ? openOrder : undefined}
      onKeyDown={canOpenOrder ? onCardKeyDown : undefined}
    >
      {flagIcon ? (
        <span className="card-flag-watermark" aria-hidden="true">
          <Icon className="card-flag-icon" icon={flagIcon} />
        </span>
      ) : null}
      <TrendBackground points={historyPoints} window={historyWindow} />
      {capText ? <div className={`cfg-cap ${capTone}`}>{capText}</div> : null}
      <div className="card-content">
        <div className="cfg-title">
          <span className="title-text">{cfg.name}</span>
          {cfg.lifecycle.state === "delisted" ? <span className="pill sm err">下架</span> : null}
        </div>
        {isCloud ? null : (
          <div className="cfg-specs" aria-label="规格">
            {specCells.map((c) =>
              "empty" in c ? (
                <div className="spec-cell empty" key={c.key}>
                  <span className="spec-k"> </span>
                  <span className="spec-v"> </span>
                </div>
              ) : (
                <div className="spec-cell" key={c.key}>
                  <span className="spec-k">{c.label}</span>
                  <span className="spec-v">{c.value}</span>
                </div>
              ),
            )}
          </div>
        )}
        <div className="cfg-price">{formatMoney(cfg.price)}</div>
        {foot ? <div className="cfg-foot">{foot}</div> : null}
        <div className="cfg-pills">
          {!canOpenOrder ? <div className="cfg-order-hint">暂无下单链接</div> : null}
          <MonitorToggle
            onClick={
              cfg.monitorSupported
                ? (e) => {
                    e.stopPropagation();
                    onToggle(cfg.id, !cfg.monitorEnabled);
                  }
                : undefined
            }
            onKeyDown={cfg.monitorSupported ? (e) => e.stopPropagation() : undefined}
            state={monitorState}
          />
        </div>
      </div>
    </div>
  );
}

export function MonitoringCard({
  cfg,
  countriesById,
  nowMs,
  orderLink = null,
  onOpenOrder,
  historyWindow = null,
  historyPoints,
}: {
  cfg: Config;
  countriesById: Map<string, Country>;
  nowMs: number;
  orderLink?: OrderLink | null;
  onOpenOrder?: (cfg: Config, orderLink: OrderLink) => void;
  historyWindow?: InventoryHistoryResponse["window"] | null;
  historyPoints?: InventoryHistoryPoint[];
}) {
  const canOpenOrder = Boolean(orderLink?.url);
  const flagIcon = resolveCountryFlagWatermarkIcon(countriesById.get(cfg.countryId)?.name ?? null);
  const capTone = cfg.inventory.status === "unknown" || cfg.inventory.quantity === 0 ? "warn" : "";
  const capText =
    cfg.inventory.status === "unknown"
      ? "?"
      : cfg.inventory.quantity > 10
        ? "10+"
        : String(cfg.inventory.quantity);
  const rawSpecCells = buildSpecCells(cfg.specs, SPEC_CARD_MAX_CELLS);
  const specCells: SpecSlotCell[] = SPEC_SLOTS.slice(0, SPEC_CARD_MAX_CELLS).map((key, i) => {
    const c = rawSpecCells[i];
    return c ? { key, ...c } : { key, empty: true };
  });
  const openOrder = () => {
    if (!orderLink?.url) return;
    if (onOpenOrder) {
      onOpenOrder(cfg, orderLink);
      return;
    }
    window.open(orderLink.url, "_blank", "noopener,noreferrer");
  };
  const onCardKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!canOpenOrder) return;
    if (e.key === "Enter" || e.key === " " || e.key === "Spacebar") {
      e.preventDefault();
      openOrder();
    }
  };
  const orderOpenLabel = "打开下单页（新标签页）";
  const orderTitle = canOpenOrder ? `点击${orderOpenLabel}` : "暂无下单链接";
  return (
    <div
      className={`mon-card ${canOpenOrder ? "card-linkable" : "card-link-disabled"}`}
      role={canOpenOrder ? "link" : undefined}
      tabIndex={canOpenOrder ? 0 : undefined}
      aria-label={canOpenOrder ? orderOpenLabel : undefined}
      title={orderTitle}
      onClick={canOpenOrder ? openOrder : undefined}
      onKeyDown={canOpenOrder ? onCardKeyDown : undefined}
    >
      {flagIcon ? (
        <span className="card-flag-watermark" aria-hidden="true">
          <Icon className="card-flag-icon" icon={flagIcon} />
        </span>
      ) : null}
      <TrendBackground points={historyPoints} window={historyWindow} />
      <div className={`mon-cap ${capTone}`}>{capText}</div>
      <div className="card-content">
        <div className="mon-title">
          <span className="title-text">{cfg.name}</span>
          {cfg.lifecycle.state === "delisted" ? <span className="pill sm err">下架</span> : null}
        </div>
        <div className="mon-specs" aria-label="规格">
          {specCells.map((c) =>
            "empty" in c ? (
              <div className="spec-cell empty" key={c.key}>
                <span className="spec-k"> </span>
                <span className="spec-v"> </span>
              </div>
            ) : (
              <div className="spec-cell" key={c.key}>
                <span className="spec-k">{c.label}</span>
                <span className="spec-v">{c.value}</span>
              </div>
            ),
          )}
        </div>
        <div className="mon-price">{formatMoney(cfg.price)}</div>
        <div className="mon-pills">
          {!canOpenOrder ? <div className="mon-order-hint">暂无下单链接</div> : null}
          <div className="mon-update">{`更新：${formatRelativeTime(
            cfg.inventory.checkedAt,
            nowMs,
          )}`}</div>
        </div>
      </div>
    </div>
  );
}

export function ProductsView({
  bootstrap,
  countriesById,
  regionsById,
  orderBaseUrl,
  archiveFilterMode,
  onArchiveFilterModeChange,
  onArchiveDelisted,
  onToggle,
  onTogglePartition,
  onRefreshPartition,
  partitionRefreshStates,
  onOpenOrder,
}: {
  bootstrap: BootstrapResponse;
  countriesById: Map<string, Country>;
  regionsById: Map<string, Region>;
  orderBaseUrl: string;
  archiveFilterMode: ArchiveFilterMode;
  onArchiveFilterModeChange: (mode: ArchiveFilterMode) => void;
  onArchiveDelisted: () => Promise<ArchiveDelistedResponse>;
  onToggle: (configId: string, enabled: boolean) => void;
  onTogglePartition: (countryId: string, regionId: string | null, enabled: boolean) => void;
  onRefreshPartition: (countryId: string, regionId: string) => Promise<void>;
  partitionRefreshStates: Record<string, PartitionRefreshState>;
  onOpenOrder: (cfg: Config, orderLink: OrderLink) => void;
}) {
  const [countryFilter, setCountryFilter] = useState<string>("all");
  const [regionFilter, setRegionFilter] = useState<string>("all");
  const [search, setSearch] = useState<string>("");
  const [onlyMonitored, setOnlyMonitored] = useState<boolean>(false);
  const [archiveDialogOpen, setArchiveDialogOpen] = useState<boolean>(false);
  const [archiveSubmitting, setArchiveSubmitting] = useState<boolean>(false);
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [archiveResult, setArchiveResult] = useState<ArchiveDelistedResponse | null>(null);

  const regionOptions = useMemo(() => {
    const out: Array<{ id: string; label: string }> = [];
    for (const r of bootstrap.catalog.regions) {
      if (countryFilter !== "all" && r.countryId !== countryFilter) continue;
      const label = r.locationName ? `${r.name}（${r.locationName}）` : r.name;
      out.push({ id: r.id, label });
    }
    out.sort((a, b) => a.label.localeCompare(b.label, "zh-Hans-CN"));
    return out;
  }, [bootstrap, countryFilter]);

  useEffect(() => {
    if (regionFilter === "all") return;
    if (regionOptions.some((region) => region.id === regionFilter)) return;
    setRegionFilter("all");
  }, [regionFilter, regionOptions]);

  const enabledPartitionKeys = useMemo(
    () => buildEnabledPartitionKeySet(bootstrap.monitoring.enabledPartitions),
    [bootstrap.monitoring.enabledPartitions],
  );
  const groupNoticeByKey = useMemo(
    () => buildGroupNoticeByKey(bootstrap.catalog.regionNotices),
    [bootstrap.catalog.regionNotices],
  );
  const countriesWithTopologyRegions = useMemo(
    () => buildCountriesWithTopologyRegions(bootstrap.catalog.regions),
    [bootstrap.catalog.regions],
  );

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return filterConfigsByArchiveMode(bootstrap.catalog.configs, archiveFilterMode).filter(
      (cfg) => {
        if (onlyMonitored) {
          const configMonitored = cfg.monitorEnabled;
          const partitionMonitored = enabledPartitionKeys.has(
            buildPartitionKey(cfg.countryId, cfg.regionId),
          );
          const countryMonitored =
            cfg.regionId === null &&
            enabledPartitionKeys.has(buildPartitionKey(cfg.countryId, null));
          if (!configMonitored && !partitionMonitored && !countryMonitored) {
            return false;
          }
        }
        if (countryFilter !== "all" && cfg.countryId !== countryFilter) return false;
        if (regionFilter !== "all" && (cfg.regionId ?? "") !== regionFilter) return false;
        if (!q) return true;
        if (cfg.name.toLowerCase().includes(q)) return true;
        const specText = cfg.specs
          .map((s) => `${s.key} ${s.value}`.trim())
          .join(" ")
          .toLowerCase();
        if (specText.includes(q)) return true;
        const countryName = countriesById.get(cfg.countryId)?.name?.toLowerCase() ?? "";
        if (countryName.includes(q)) return true;
        const region = cfg.regionId ? regionsById.get(cfg.regionId) : null;
        const regionName = region?.name?.toLowerCase() ?? "";
        const regionLocation = region?.locationName?.toLowerCase() ?? "";
        return regionName.includes(q) || regionLocation.includes(q);
      },
    );
  }, [
    archiveFilterMode,
    bootstrap.catalog.configs,
    countriesById,
    countryFilter,
    enabledPartitionKeys,
    onlyMonitored,
    regionFilter,
    regionsById,
    search,
  ]);

  const archiveCandidates = useMemo(
    () =>
      bootstrap.catalog.configs.filter(
        (cfg) => cfg.lifecycle.state === "delisted" && !cfg.lifecycle.cleanupAt,
      ),
    [bootstrap],
  );
  const previewArchiveCandidates = useMemo(
    () => archiveCandidates.slice(0, 24),
    [archiveCandidates],
  );

  const countryGroups = useMemo(() => {
    type MutableRegionGroup = ProductRegionGroup & {
      sortLabel: string;
    };

    type MutableCountryGroup = Omit<ProductCountryGroup, "groups"> & {
      groups: MutableRegionGroup[];
      groupsByKey: Map<string, MutableRegionGroup>;
    };

    const byCountry = new Map<string, MutableCountryGroup>();
    const q = search.trim().toLowerCase();
    const matchesSearch = (...values: Array<string | null | undefined>) =>
      !q || values.some((value) => value?.toLowerCase().includes(q));
    const matchesCountryFilter = (countryId: string) =>
      countryFilter === "all" || countryId === countryFilter;
    const matchesRegionFilter = (regionId: string | null) =>
      regionFilter === "all" || (regionId ?? "") === regionFilter;
    const visibleCatalogRegionKeys = new Set(
      filterConfigsByArchiveMode(bootstrap.catalog.configs, archiveFilterMode)
        .filter((cfg) => {
          if (!cfg.regionId) {
            return false;
          }
          if (!matchesCountryFilter(cfg.countryId) || !matchesRegionFilter(cfg.regionId)) {
            return false;
          }
          if (!q) {
            return true;
          }
          if (cfg.name.toLowerCase().includes(q)) {
            return true;
          }
          const specText = cfg.specs
            .map((spec) => `${spec.key} ${spec.value}`.trim())
            .join(" ")
            .toLowerCase();
          if (specText.includes(q)) {
            return true;
          }
          const countryName = countriesById.get(cfg.countryId)?.name?.toLowerCase() ?? "";
          if (countryName.includes(q)) {
            return true;
          }
          const region = regionsById.get(cfg.regionId);
          const regionName = region?.name?.toLowerCase() ?? "";
          const regionLocation = region?.locationName?.toLowerCase() ?? "";
          return regionName.includes(q) || regionLocation.includes(q);
        })
        .map((cfg) => buildPartitionKey(cfg.countryId, cfg.regionId)),
    );

    const ensureCountry = (countryId: string) => {
      let country = byCountry.get(countryId);
      if (country) {
        return country;
      }

      const countryName = countriesById.get(countryId)?.name ?? countryId;
      const countryKey = buildPartitionKey(countryId, null);
      country = {
        countryId,
        countryName,
        isCloud: countryName.includes("云服务器"),
        countryMonitorEnabled: enabledPartitionKeys.has(countryKey),
        countryNotice: resolveScopedGroupNotice(
          groupNoticeByKey,
          countriesWithTopologyRegions,
          countryId,
          null,
        ),
        directConfigs: [],
        groups: [],
        groupsByKey: new Map(),
      };
      byCountry.set(countryId, country);
      return country;
    };

    const ensureRegionGroup = (
      country: MutableCountryGroup,
      regionId: string,
      title: string,
      subtitle: string | null,
      sortLabel: string,
      refreshAvailable: boolean,
    ) => {
      const groupKey = buildPartitionKey(country.countryId, regionId);
      let regionGroup = country.groupsByKey.get(groupKey);
      if (regionGroup) {
        if (refreshAvailable) {
          regionGroup.refreshAvailable = true;
        }
        return regionGroup;
      }

      regionGroup = {
        key: groupKey,
        regionId,
        title,
        subtitle,
        partitionEnabled: enabledPartitionKeys.has(groupKey),
        refreshAvailable,
        notice: resolveScopedGroupNotice(
          groupNoticeByKey,
          countriesWithTopologyRegions,
          country.countryId,
          regionId,
        ),
        configs: [],
        sortLabel,
      };
      country.groupsByKey.set(groupKey, regionGroup);
      country.groups.push(regionGroup);
      return regionGroup;
    };

    for (const cfg of filtered) {
      const country = ensureCountry(cfg.countryId);
      if (!cfg.regionId) {
        country.directConfigs.push(cfg);
        continue;
      }

      const region = regionsById.get(cfg.regionId);
      const regionLabel = region?.name ?? cfg.regionId;
      const regionGroup = ensureRegionGroup(
        country,
        cfg.regionId,
        regionLabel,
        region?.locationName ?? null,
        `${regionLabel}::${region?.locationName ?? ""}`,
        false,
      );
      regionGroup.configs.push(cfg);
    }

    if (archiveFilterMode !== "archived") {
      for (const country of bootstrap.catalog.countries) {
        if (!matchesCountryFilter(country.id)) {
          continue;
        }

        const topologyRegions = bootstrap.catalog.regions.filter(
          (region) => region.countryId === country.id,
        );
        const countryMonitorEnabled = enabledPartitionKeys.has(buildPartitionKey(country.id, null));
        const monitoredTopologyRegionIds = topologyRegions.filter((region) =>
          enabledPartitionKeys.has(buildPartitionKey(country.id, region.id)),
        );
        const countryMatchesSearch = matchesSearch(country.name);
        const visibleTopologyRegions = topologyRegions.filter((region) => {
          if (!matchesRegionFilter(region.id)) {
            return false;
          }
          const regionKey = buildPartitionKey(country.id, region.id);
          const regionMonitored = enabledPartitionKeys.has(regionKey);
          if (onlyMonitored && !regionMonitored) {
            const emptyTopologyScopeUnderCountryMonitor =
              countryMonitorEnabled && !visibleCatalogRegionKeys.has(regionKey);
            if (!emptyTopologyScopeUnderCountryMonitor) {
              return false;
            }
          }
          if (!q) {
            return true;
          }
          return countryMatchesSearch || matchesSearch(region.name, region.locationName ?? "");
        });

        const hasVisibleTopologyScopes = visibleTopologyRegions.length > 0;
        const countryScopeAllowedByRegionFilter =
          regionFilter === "all" || matchesRegionFilter(null);
        const shouldIncludeCountry =
          byCountry.has(country.id) ||
          (countryScopeAllowedByRegionFilter && countryMonitorEnabled) ||
          (countryScopeAllowedByRegionFilter && monitoredTopologyRegionIds.length > 0) ||
          (countryScopeAllowedByRegionFilter && !q && topologyRegions.length === 0) ||
          hasVisibleTopologyScopes ||
          countryMatchesSearch;

        if (!shouldIncludeCountry) {
          continue;
        }
        if (
          onlyMonitored &&
          !byCountry.has(country.id) &&
          !countryMonitorEnabled &&
          monitoredTopologyRegionIds.length === 0
        ) {
          continue;
        }
        if (q && !byCountry.has(country.id) && !countryMatchesSearch && !hasVisibleTopologyScopes) {
          continue;
        }
        if (
          !matchesRegionFilter(null) &&
          topologyRegions.length === 0 &&
          !byCountry.has(country.id)
        ) {
          continue;
        }

        const countryEntry = ensureCountry(country.id);
        for (const region of visibleTopologyRegions) {
          ensureRegionGroup(
            countryEntry,
            region.id,
            region.name,
            region.locationName ?? null,
            `${region.name}::${region.locationName ?? ""}`,
            true,
          );
        }
      }
    }

    const out = Array.from(byCountry.values()).map((country) => {
      country.groups.sort((a, b) => {
        return a.sortLabel.localeCompare(b.sortLabel, "zh-Hans-CN");
      });
      return {
        countryId: country.countryId,
        countryName: country.countryName,
        isCloud: country.isCloud,
        countryMonitorEnabled: country.countryMonitorEnabled,
        countryNotice: country.countryNotice,
        directConfigs: country.directConfigs,
        groups: country.groups,
      } satisfies ProductCountryGroup;
    });

    out.sort((a, b) => {
      if (a.isCloud && !b.isCloud) return 1;
      if (!a.isCloud && b.isCloud) return -1;
      return a.countryName.localeCompare(b.countryName, "zh-Hans-CN");
    });

    return out;
  }, [
    archiveFilterMode,
    bootstrap.catalog.configs,
    bootstrap.catalog.countries,
    bootstrap.catalog.regions,
    countriesById,
    countriesWithTopologyRegions,
    countryFilter,
    enabledPartitionKeys,
    filtered,
    groupNoticeByKey,
    onlyMonitored,
    regionFilter,
    regionsById,
    search,
  ]);

  const visibleIds = useMemo(() => filtered.map((c) => c.id), [filtered]);
  const { window: historyWindow, byId: historyById } = useInventoryHistory(
    visibleIds,
    bootstrap.catalog.fetchedAt,
  );

  const openArchiveDialog = () => {
    setArchiveError(null);
    setArchiveDialogOpen(true);
  };

  const closeArchiveDialog = () => {
    if (archiveSubmitting) return;
    setArchiveDialogOpen(false);
  };

  const confirmArchiveDelisted = async () => {
    if (archiveSubmitting) return;
    setArchiveSubmitting(true);
    setArchiveError(null);
    try {
      const res = await onArchiveDelisted();
      setArchiveResult(res);
      setArchiveDialogOpen(false);
    } catch (e) {
      setArchiveError(e instanceof Error ? e.message : String(e));
    } finally {
      setArchiveSubmitting(false);
    }
  };

  return (
    <div className="panel" data-testid="page-products">
      <div className="panel-section">
        <div className="panel-title">筛选与分组</div>
        <div className="controls">
          <div className="pill select" style={{ width: "200px" }}>
            <span className="pill-prefix">国家地区：</span>
            <select value={countryFilter} onChange={(e) => setCountryFilter(e.target.value)}>
              <option value="all">全部</option>
              {bootstrap.catalog.countries.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>

          <div className="pill select" style={{ width: "200px" }}>
            <span className="pill-prefix">可用区域：</span>
            <select
              value={regionFilter}
              onChange={(e) => setRegionFilter(e.target.value)}
              disabled={regionOptions.length === 0}
            >
              <option value="all">全部</option>
              {regionOptions.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.label}
                </option>
              ))}
            </select>
          </div>

          <div className="pill search" style={{ width: "312px" }}>
            <span className="pill-prefix">搜索：</span>
            <input
              placeholder="配置名 / 规格关键字…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>

          <button
            type="button"
            className={`pill ${onlyMonitored ? "on" : ""}`}
            style={{ width: "248px" }}
            onClick={() => setOnlyMonitored((v) => !v)}
          >
            仅看已监控
          </button>

          <div className="pill select" style={{ width: "216px" }}>
            <span className="pill-prefix">下架归档：</span>
            <select
              value={archiveFilterMode}
              onChange={(e) => onArchiveFilterModeChange(e.target.value as ArchiveFilterMode)}
            >
              <option value="active">仅正常</option>
              <option value="all">全部</option>
              <option value="archived">仅归档</option>
            </select>
          </div>

          <button
            type="button"
            className={`pill ${archiveCandidates.length > 0 ? "warn" : "disabled"}`}
            style={{ width: "272px" }}
            onClick={openArchiveDialog}
            disabled={archiveCandidates.length === 0}
          >
            {`一键归档下架（${archiveCandidates.length}）`}
          </button>
        </div>
        {archiveResult ? (
          <div className="muted">
            {archiveResult.archivedCount > 0
              ? `已归档 ${archiveResult.archivedCount} 项下架配置。`
              : "没有可归档的下架配置。"}
          </div>
        ) : null}
      </div>

      {countryGroups.length === 0 ? <div className="empty">没有匹配的配置。</div> : null}

      {countryGroups.map((country) => (
        <ProductCountrySection
          country={country}
          countriesById={countriesById}
          historyById={historyById}
          historyWindow={historyWindow}
          key={country.countryId}
          onOpenOrder={onOpenOrder}
          onRefreshPartition={onRefreshPartition}
          onToggle={onToggle}
          onTogglePartition={onTogglePartition}
          orderBaseUrl={orderBaseUrl}
          partitionRefreshStates={partitionRefreshStates}
        />
      ))}

      {archiveDialogOpen ? (
        <div
          className="ops-modal-backdrop"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) closeArchiveDialog();
          }}
          role="presentation"
        >
          <dialog className="ops-modal archive-modal" open aria-label="归档下架配置">
            <div className="ops-modal-title">归档全部下架配置</div>
            <div className="ops-modal-body">
              <div className="muted">
                {`将归档 ${archiveCandidates.length} 项“下架且未归档”配置，归档后默认视图会隐藏这些项。`}
              </div>
              {previewArchiveCandidates.length > 0 ? (
                <div className="archive-preview-grid">
                  {previewArchiveCandidates.map((cfg) => {
                    const country = countriesById.get(cfg.countryId)?.name ?? cfg.countryId;
                    const scope = cfg.regionId
                      ? (regionsById.get(cfg.regionId)?.name ?? cfg.regionId)
                      : country;
                    return (
                      <div className="archive-preview-card" key={cfg.id}>
                        <div className="archive-preview-title">{cfg.name}</div>
                        <div className="archive-preview-meta">{scope}</div>
                        <div className="archive-preview-meta">{formatMoney(cfg.price)}</div>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="muted">当前没有可归档的下架配置。</div>
              )}
              {archiveCandidates.length > previewArchiveCandidates.length ? (
                <div className="muted">
                  {`仅预览前 ${previewArchiveCandidates.length} 项，确认后会归档全部 ${archiveCandidates.length} 项。`}
                </div>
              ) : null}
              {archiveError ? <div className="error">{archiveError}</div> : null}
            </div>
            <div className="ops-modal-actions">
              <button type="button" className="pill" onClick={closeArchiveDialog}>
                取消
              </button>
              <button
                type="button"
                className="pill warn"
                onClick={() => void confirmArchiveDelisted()}
                disabled={archiveSubmitting || archiveCandidates.length === 0}
              >
                {archiveSubmitting ? "归档中..." : "确认归档"}
              </button>
            </div>
          </dialog>
        </div>
      ) : null}
    </div>
  );
}

export function MonitoringView({
  bootstrap,
  countriesById,
  regionsById,
  orderBaseUrl,
  nowMs,
  syncAlert,
  recentListed24h,
  onDismissSyncAlert,
  onOpenOrder,
}: {
  bootstrap: BootstrapResponse;
  countriesById: Map<string, Country>;
  regionsById: Map<string, Region>;
  orderBaseUrl: string;
  nowMs: number;
  syncAlert: string | null;
  recentListed24h: Config[];
  onDismissSyncAlert: () => void;
  onOpenOrder: (cfg: Config, orderLink: OrderLink) => void;
}) {
  const enabledAll = useMemo(
    () => bootstrap.catalog.configs.filter((c) => c.monitorEnabled),
    [bootstrap],
  );
  const enabled = useMemo(() => filterConfigsByArchiveMode(enabledAll, "active"), [enabledAll]);
  const recentListedFiltered = useMemo(
    () => filterConfigsByArchiveMode(recentListed24h, "active"),
    [recentListed24h],
  );

  const visibleIds = useMemo(() => enabled.map((c) => c.id), [enabled]);
  const { window: historyWindow, byId: historyById } = useInventoryHistory(
    visibleIds,
    bootstrap.catalog.fetchedAt,
  );

  const groups = useMemo(() => {
    const m = new Map<string, Config[]>();
    for (const cfg of enabled) {
      const k = groupKey(cfg);
      const list = m.get(k);
      if (list) list.push(cfg);
      else m.set(k, [cfg]);
    }
    return Array.from(m.entries());
  }, [enabled]);

  const groupNoticeByKey = useMemo(
    () => buildGroupNoticeByKey(bootstrap.catalog.regionNotices),
    [bootstrap.catalog.regionNotices],
  );
  const countriesWithTopologyRegions = useMemo(
    () => buildCountriesWithTopologyRegions(bootstrap.catalog.regions),
    [bootstrap.catalog.regions],
  );

  return (
    <div className="panel" data-testid="page-monitoring">
      {syncAlert ? (
        <div className="alert alert-error">
          <span className="sync-icon err" aria-hidden="true">
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path fill="currentColor" d="M1 21h22L12 2zm12-3h-2v-2h2zm0-4h-2v-4h2z" />
            </svg>
          </span>
          <div className="alert-message">{syncAlert}</div>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onDismissSyncAlert}>
            ✕
          </button>
        </div>
      ) : null}
      {recentListedFiltered.length > 0 ? (
        <div className="panel-section">
          <div className="panel-title">最近 24 小时上架</div>
          <div className="panel-subtitle">listed（含重新上架）</div>
          <div className="divider-bleed" />
          <div className="grid">
            {recentListedFiltered.slice(0, 12).map((cfg) => (
              <MonitoringCard
                cfg={cfg}
                countriesById={countriesById}
                key={cfg.id}
                nowMs={nowMs}
                orderLink={buildOrderLink(orderBaseUrl, cfg)}
                onOpenOrder={onOpenOrder}
              />
            ))}
          </div>
        </div>
      ) : null}
      {enabled.length === 0 ? (
        <div className="panel-section">
          <div className="empty">
            {enabledAll.length === 0
              ? "还没有启用监控的配置。去“全部产品”里点选需要监控的配置。"
              : "当前暂无可展示的监控配置（已归档下架项默认隐藏）。"}
          </div>
        </div>
      ) : null}
      {groups.map(([k, items]) => {
        const [countryId, regionId] = k.split("::");
        const countryName = countriesById.get(countryId)?.name;
        const title = countryName
          ? regionId
            ? `${countryName} / ${regionsById.get(regionId)?.name ?? regionId}`
            : countryName
          : "分组信息加载中…";
        return (
          <MonitoringSection
            key={k}
            collapseKey={`catnap:collapse:${k}`}
            title={title}
            groupNotice={resolveScopedGroupNotice(
              groupNoticeByKey,
              countriesWithTopologyRegions,
              countryId,
              regionId || null,
            )}
            items={items}
            countriesById={countriesById}
            orderBaseUrl={orderBaseUrl}
            nowMs={nowMs}
            onOpenOrder={onOpenOrder}
            historyWindow={historyWindow}
            historyById={historyById}
          />
        );
      })}
      <div className="panel-section">
        <div className="panel-title">提示</div>
        <div className="panel-subtitle">
          在“全部产品”中开启监控后，配置会出现在对应国家或可用区下。
        </div>
        <div className="muted">轮询频率与抖动在“系统设置”中配置；日志可追溯每次变更与通知。</div>
      </div>
    </div>
  );
}

function PartitionRefreshButton({
  regionTitle,
  testId,
  state,
  onRefresh,
}: {
  regionTitle: string;
  testId: string;
  state?: PartitionRefreshState;
  onRefresh: () => void;
}) {
  const isRunning = state?.kind === "running";
  const buttonClass = [
    "panel-title-link",
    "partition-refresh-btn",
    isRunning ? "running" : "",
    state?.kind === "success" ? "success" : "",
    state?.kind === "error" ? "error" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const label = isRunning
    ? `刷新 ${regionTitle} 中`
    : state?.kind === "success"
      ? `已刷新 ${regionTitle}`
      : state?.kind === "error"
        ? `刷新 ${regionTitle} 失败`
        : `刷新 ${regionTitle}`;

  return (
    <button
      type="button"
      className={buttonClass}
      data-testid={testId}
      aria-label={label}
      title={state?.kind === "error" ? state.message : label}
      onClick={onRefresh}
      disabled={isRunning}
    >
      <span
        className={`sync-icon ${isRunning ? "spin" : ""} ${state?.kind === "success" ? "ok" : ""} ${state?.kind === "error" ? "err" : ""}`.trim()}
        aria-hidden="true"
      >
        {isRunning ? (
          <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 4a8 8 0 0 1 7.9 6.7a1 1 0 1 1-2 .3A6 6 0 1 0 18 12a1 1 0 1 1 2 0a8 8 0 1 1-8-8"
            />
          </svg>
        ) : state?.kind === "success" ? (
          <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2m-1 14l-4-4l1.4-1.4L11 13.2l5.6-5.6L18 9z"
            />
          </svg>
        ) : state?.kind === "error" ? (
          <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
            <path fill="currentColor" d="M1 21h22L12 2zm12-3h-2v-2h2zm0-4h-2v-4h2z" />
          </svg>
        ) : (
          <svg viewBox="0 0 24 24" focusable="false" aria-hidden="true">
            <path fill="currentColor" d="M12 6V3L8 7l4 4V8a4 4 0 1 1-4 4H6a6 6 0 1 0 6-6" />
          </svg>
        )}
      </span>
    </button>
  );
}

function ProductCountrySection({
  country,
  countriesById,
  orderBaseUrl,
  onTogglePartition,
  onRefreshPartition,
  onToggle,
  onOpenOrder,
  partitionRefreshStates,
  historyWindow = null,
  historyById = EMPTY_HISTORY_BY_ID,
}: {
  country: ProductCountryGroup;
  countriesById: Map<string, Country>;
  orderBaseUrl: string;
  onTogglePartition: (countryId: string, regionId: string | null, enabled: boolean) => void;
  onRefreshPartition: (countryId: string, regionId: string) => Promise<void>;
  onToggle: (cfgId: string, enabled: boolean) => void;
  onOpenOrder: (cfg: Config, orderLink: OrderLink) => void;
  partitionRefreshStates: Record<string, PartitionRefreshState>;
  historyWindow?: InventoryHistoryResponse["window"] | null;
  historyById?: Map<string, InventoryHistoryPoint[]>;
}) {
  const collapseKey = `catnap:products:country-collapse:${country.countryId}`;
  const [collapsed, setCollapsed] = useState<boolean>(
    () => localStorage.getItem(collapseKey) === "1",
  );

  const countryCatalogLink = buildCountryCatalogLink(orderBaseUrl, country.countryId);
  const regionCount = country.groups.length;
  const configCount =
    country.directConfigs.length +
    country.groups.reduce((sum, group) => sum + group.configs.length, 0);
  const meta = [regionCount > 0 ? `${regionCount} 个可用区` : null, `${configCount} 个套餐`]
    .filter(Boolean)
    .join(" • ");

  return (
    <div className="panel-section product-country-section">
      <div className="product-country-header">
        <div className="panel-title-row product-country-title-row">
          <div className="product-country-heading">
            <div className="product-country-heading-copy">
              <div
                className="panel-title product-country-title"
                id={`products-country-heading-${country.countryId}`}
              >
                {country.countryName}
              </div>
              <div className="product-country-meta">{meta}</div>
            </div>
          </div>
          <div className="panel-title-actions">
            <button
              type="button"
              className="pill center collapse-btn"
              aria-label={collapsed ? `展开 ${country.countryName}` : `折叠 ${country.countryName}`}
              onClick={() => {
                const next = !collapsed;
                setCollapsed(next);
                localStorage.setItem(collapseKey, next ? "1" : "0");
              }}
            >
              {collapsed ? "展开" : "折叠"}
            </button>
            <MonitorToggle
              labelledBy={`products-country-heading-${country.countryId}`}
              onClick={() =>
                onTogglePartition(country.countryId, null, !country.countryMonitorEnabled)
              }
              state={country.countryMonitorEnabled ? "on" : "off"}
              testId={`country-monitor-${country.countryId}`}
            />
            {countryCatalogLink ? (
              <a
                className="panel-title-link"
                href={countryCatalogLink}
                target="_blank"
                rel="noopener noreferrer"
                aria-label={`打开上游分组页（新标签页）fid=${country.countryId}`}
                title={`打开上游分组页（新标签页）
${countryCatalogLink}`}
              >
                <Icon className="panel-title-link-icon" icon="mdi:open-in-new" aria-hidden="true" />
              </a>
            ) : null}
          </div>
        </div>
      </div>
      {collapsed ? null : (
        <>
          {country.countryNotice ? (
            <div className="panel-subtitle group-notice">{country.countryNotice}</div>
          ) : null}
          {country.directConfigs.length === 0 && country.groups.length === 0 ? (
            <div className="product-country-empty">当前暂无可用区与套餐。</div>
          ) : null}

          {country.directConfigs.length > 0 ? (
            <div className="grid product-country-direct-grid">
              {country.directConfigs.map((cfg) => (
                <ProductCard
                  cfg={cfg}
                  countriesById={countriesById}
                  key={cfg.id}
                  orderLink={buildOrderLink(orderBaseUrl, cfg)}
                  onOpenOrder={onOpenOrder}
                  onToggle={onToggle}
                  historyWindow={historyWindow}
                  historyPoints={historyById.get(cfg.id)}
                />
              ))}
            </div>
          ) : null}

          {country.groups.length > 0 ? (
            <div
              className={`product-region-list ${country.directConfigs.length > 0 ? "after-direct-configs" : ""}`}
            >
              {country.groups.map((group, index) => (
                <section
                  className={`product-region-block ${index === 0 ? "first" : ""}`}
                  data-testid={`products-region-${buildScopedRegionDomKey(country.countryId, group.regionId)}`}
                  key={group.key}
                >
                  <div className="product-region-header">
                    <div className="panel-title-row product-region-title-row">
                      <div className="product-region-heading">
                        <div className="product-region-heading-copy">
                          <div className="product-region-title-row-main">
                            <div
                              className="product-region-title"
                              id={`products-region-heading-${buildScopedRegionDomKey(
                                country.countryId,
                                group.regionId,
                              )}`}
                            >
                              {group.title}
                            </div>
                            {group.subtitle ? (
                              <div className="product-region-subtitle">{group.subtitle}</div>
                            ) : null}
                          </div>
                        </div>
                      </div>
                      <div className="panel-title-actions">
                        {group.refreshAvailable ? (
                          <PartitionRefreshButton
                            regionTitle={group.title}
                            testId={`region-refresh-${buildScopedRegionDomKey(
                              country.countryId,
                              group.regionId,
                            )}`}
                            state={
                              partitionRefreshStates[
                                buildPartitionKey(country.countryId, group.regionId)
                              ]
                            }
                            onRefresh={() =>
                              void onRefreshPartition(country.countryId, group.regionId)
                            }
                          />
                        ) : null}
                        <MonitorToggle
                          labelledBy={`products-region-heading-${buildScopedRegionDomKey(
                            country.countryId,
                            group.regionId,
                          )}`}
                          onClick={() =>
                            onTogglePartition(
                              country.countryId,
                              group.regionId,
                              !group.partitionEnabled,
                            )
                          }
                          state={group.partitionEnabled ? "on" : "off"}
                          testId={`region-monitor-${buildScopedRegionDomKey(
                            country.countryId,
                            group.regionId,
                          )}`}
                        />
                      </div>
                    </div>
                  </div>
                  <div className="product-region-content">
                    {partitionRefreshStates[buildPartitionKey(country.countryId, group.regionId)]
                      ?.kind === "success" ? (
                      <div className="product-region-refresh-feedback success">
                        {
                          partitionRefreshStates[
                            buildPartitionKey(country.countryId, group.regionId)
                          ]?.message
                        }
                      </div>
                    ) : null}
                    {partitionRefreshStates[buildPartitionKey(country.countryId, group.regionId)]
                      ?.kind === "error" ? (
                      <div className="product-region-refresh-feedback error">
                        {
                          partitionRefreshStates[
                            buildPartitionKey(country.countryId, group.regionId)
                          ]?.message
                        }
                      </div>
                    ) : null}
                    {group.notice ? (
                      <div className="panel-subtitle group-notice">{group.notice}</div>
                    ) : null}
                    <div className="divider-bleed product-region-divider" />
                    {group.configs.length === 0 ? (
                      <div className="product-region-empty">当前暂无套餐。</div>
                    ) : (
                      <div className="grid">
                        {group.configs.map((cfg) => (
                          <ProductCard
                            cfg={cfg}
                            countriesById={countriesById}
                            key={cfg.id}
                            orderLink={buildOrderLink(orderBaseUrl, cfg)}
                            onOpenOrder={onOpenOrder}
                            onToggle={onToggle}
                            historyWindow={historyWindow}
                            historyPoints={historyById.get(cfg.id)}
                          />
                        ))}
                      </div>
                    )}
                  </div>
                </section>
              ))}
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}

export function MonitoringSection({
  collapseKey,
  title,
  groupNotice = null,
  items,
  countriesById,
  orderBaseUrl,
  nowMs,
  onOpenOrder,
  historyWindow = null,
  historyById = EMPTY_HISTORY_BY_ID,
}: {
  collapseKey: string;
  title: string;
  groupNotice?: string | null;
  items: Config[];
  countriesById: Map<string, Country>;
  orderBaseUrl: string;
  nowMs: number;
  onOpenOrder: (cfg: Config, orderLink: OrderLink) => void;
  historyWindow?: InventoryHistoryResponse["window"] | null;
  historyById?: Map<string, InventoryHistoryPoint[]>;
}) {
  const [collapsed, setCollapsed] = useState<boolean>(
    () => localStorage.getItem(collapseKey) === "1",
  );

  const { restockCount, totalQty, recent } = useMemo(() => {
    let restock = 0;
    let sum = 0;
    let recentIso: string | null = null;
    let recentMs = Number.NEGATIVE_INFINITY;
    for (const cfg of items) {
      if (cfg.inventory.quantity > 0) restock += 1;
      sum += cfg.inventory.status === "unknown" ? 0 : cfg.inventory.quantity;
      const t = Date.parse(cfg.inventory.checkedAt);
      if (Number.isFinite(t) && t > recentMs) {
        recentMs = t;
        recentIso = cfg.inventory.checkedAt;
      }
    }
    return { restockCount: restock, totalQty: sum, recent: recentIso };
  }, [items]);

  const meta = `${items.length} 个配置 • ${collapsed ? "折叠" : "展开"} • 补货 ${restockCount} 台 • 余 ${totalQty} 台（最近 ${
    recent ? formatRelativeTime(recent, nowMs) : "—"
  }）`;

  return (
    <div className="panel-section">
      <div className="group-head">
        <div>
          <div className="panel-title">{title}</div>
          <div className="group-meta">{meta}</div>
          {groupNotice ? <div className="panel-subtitle group-notice">{groupNotice}</div> : null}
        </div>
        <button
          type="button"
          className="pill center collapse-btn"
          onClick={() => {
            const next = !collapsed;
            setCollapsed(next);
            localStorage.setItem(collapseKey, next ? "1" : "0");
          }}
        >
          {collapsed ? "展开" : "折叠"}
        </button>
      </div>
      {collapsed ? null : (
        <>
          <div className="divider-bleed" />
          <div className="grid">
            {items.map((cfg) => (
              <MonitoringCard
                cfg={cfg}
                countriesById={countriesById}
                key={cfg.id}
                nowMs={nowMs}
                orderLink={buildOrderLink(orderBaseUrl, cfg)}
                onOpenOrder={onOpenOrder}
                historyWindow={historyWindow}
                historyPoints={historyById.get(cfg.id)}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

export function MachinesView({
  bootstrap,
  onSync,
  fetchMachines = () => api<LazycatMachinesResponse>("/api/lazycat/machines"),
}: {
  bootstrap: BootstrapResponse;
  onSync: () => Promise<LazycatAccountView>;
  onRefreshAccount: () => Promise<LazycatAccountView>;
  fetchMachines?: () => Promise<LazycatMachinesResponse>;
}) {
  const [account, setAccount] = useState<LazycatAccountView>(() => bootstrap.lazycat);
  const [items, setItems] = useState<LazycatMachineView[]>([]);
  const [loading, setLoading] = useState<boolean>(bootstrap.lazycat.connected);
  const [refreshing, setRefreshing] = useState<boolean>(false);
  const [syncPending, setSyncPending] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedServiceId, setExpandedServiceId] = useState<number | null>(null);

  const loadMachines = useCallback(
    async (mode: "load" | "refresh" = "load") => {
      if (!bootstrap.lazycat.connected) {
        setAccount(bootstrap.lazycat);
        setItems([]);
        setLoading(false);
        setRefreshing(false);
        setError(null);
        setExpandedServiceId(null);
        return;
      }

      if (mode === "load") {
        setLoading(true);
      } else {
        setRefreshing(true);
      }
      setError(null);

      try {
        const response = await fetchMachines();
        setAccount(response.account);
        setItems(response.items);
        setExpandedServiceId((current) =>
          current != null && response.items.some((item) => item.serviceId === current)
            ? current
            : null,
        );
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (mode === "load") {
          setLoading(false);
        } else {
          setRefreshing(false);
        }
      }
    },
    [bootstrap.lazycat, fetchMachines],
  );

  useEffect(() => {
    setAccount(bootstrap.lazycat);
    void loadMachines("load");
  }, [bootstrap.lazycat, loadMachines]);

  useEffect(() => {
    if (!(account.state === "syncing" || account.state === "authenticating")) {
      return;
    }
    const id = window.setInterval(() => {
      void loadMachines("refresh");
    }, 3_000);
    return () => window.clearInterval(id);
  }, [account.state, loadMachines]);

  const staleCount = items.filter((item) => item.detailState === "stale").length;
  const errorCount = items.filter((item) => item.detailState === "error").length;
  const readyTrafficCount = items.filter((item) => (item.traffic?.history?.length ?? 0) > 0).length;

  return (
    <div className="panel" data-testid="page-machines">
      <div className="panel-section">
        <div className="panel-head">
          <div>
            <div className="panel-title">懒猫云机器资产</div>
            <div className="panel-subtitle">
              主站字段和面板字段统一缓存到本地；面板失败时保留 last-good 数据，不影响核心机器信息。
            </div>
          </div>
          <div className="panel-title-actions">
            {renderLazycatAccountBadge(account)}
            <button
              type="button"
              className="pill action-primary lazycat-sync-btn"
              disabled={!account.connected || syncPending}
              onClick={async () => {
                setSyncPending(true);
                setError(null);
                try {
                  const next = await onSync();
                  setAccount(next);
                  await loadMachines("refresh");
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setSyncPending(false);
                }
              }}
            >
              {syncPending ? (
                <>
                  <InlineSpinner />
                  <span>同步中</span>
                </>
              ) : (
                "立即同步"
              )}
            </button>
          </div>
        </div>

        {!account.connected ? (
          <div className="empty">
            还没有连接懒猫云账号。去 <a href="#settings">系统设置</a>{" "}
            中填写邮箱和密码后，服务端会自动抓取机器列表喵。
          </div>
        ) : (
          <div className="machines-summary-grid">
            <div className="machines-summary-card">
              <span className="machines-summary-label">机器数</span>
              <strong>{account.machineCount}</strong>
              <span className="machines-summary-meta">当前缓存条目</span>
            </div>
            <div className="machines-summary-card">
              <span className="machines-summary-label">流量历史</span>
              <strong>{readyTrafficCount}</strong>
              <span className="machines-summary-meta">已补全小时样本的机器</span>
            </div>
            <div className="machines-summary-card">
              <span className="machines-summary-label">异常/缓存</span>
              <strong>{`${errorCount}/${staleCount}`}</strong>
              <span className="machines-summary-meta">error / stale</span>
            </div>
            <div className="machines-summary-card">
              <span className="machines-summary-label">最近同步</span>
              <strong>{formatLocalTime(account.lastSiteSyncAt)}</strong>
              <span className="machines-summary-meta">
                {account.lastPanelSyncAt
                  ? `面板 ${formatLocalTime(account.lastPanelSyncAt)}`
                  : "面板尚未同步"}
              </span>
            </div>
          </div>
        )}

        {error ? <p className="error">{error}</p> : null}
        {account.lastError ? <p className="error">{account.lastError}</p> : null}
        {refreshing && !loading ? <p className="muted">正在刷新缓存视图…</p> : null}
      </div>

      {account.connected ? (
        <div className="panel-section">
          <div className="panel-title-row">
            <div className="panel-title">机器列表</div>
            <div className="panel-title-actions">
              <span className="pill sm">{`${items.length} 台`}</span>
            </div>
          </div>
          <div className="panel-subtitle">
            展开某台机器可查看端口映射、流量重置时间和明细同步状态。
          </div>

          {loading ? <div className="empty">正在读取机器缓存…</div> : null}
          {!loading && items.length === 0 ? (
            <div className="empty">
              账号已连接，但本地还没有机器缓存。稍等同步完成或手动触发一次同步。
            </div>
          ) : null}

          <div className="machines-list">
            {items.map((item) => {
              const expanded = expandedServiceId === item.serviceId;
              const trafficSnapshot = item.traffic ? buildLazycatTrafficCycle(item.traffic) : null;
              return (
                <section className="machines-card" key={item.serviceId}>
                  <div className="machines-card-head">
                    <div className="machines-card-title-row">
                      <div className="machines-card-title-wrap">
                        <div className="machines-card-title">{item.serviceName}</div>
                        <div className="machines-card-code mono">{item.serviceCode}</div>
                      </div>
                    </div>
                    <div className="machines-card-side">
                      <div className="machines-card-badges">
                        <span className={lazycatMachineStatusClass(item.status)}>
                          {item.status}
                        </span>
                        <span className={lazycatDetailClass(item.detailState)}>
                          {lazycatDetailLabel(item.detailState)}
                        </span>
                      </div>
                      <button
                        type="button"
                        className="pill machines-expand-btn"
                        onClick={() => {
                          setExpandedServiceId((current) =>
                            current === item.serviceId ? null : item.serviceId,
                          );
                        }}
                      >
                        {expanded ? "收起详情" : "展开详情"}
                      </button>
                    </div>
                  </div>

                  <div className="machines-card-body">
                    <div className="machines-card-grid">
                      <div>
                        <span className="machines-kv-label">主地址</span>
                        <div className="machines-kv-value mono">
                          {formatLazycatAddress(item.primaryAddress)}
                        </div>
                      </div>
                      <div>
                        <span className="machines-kv-label">到期时间</span>
                        <div className="machines-kv-value">{formatLocalTime(item.expiresAt)}</div>
                      </div>
                      <div>
                        <span className="machines-kv-label">续费价格</span>
                        <div className="machines-kv-value">{item.renewPrice ?? "—"}</div>
                      </div>
                      <div>
                        <span className="machines-kv-label">支付周期</span>
                        <div className="machines-kv-value">{item.billingCycle ?? "—"}</div>
                      </div>
                      <div>
                        <span className="machines-kv-label">附加地址</span>
                        <div className="machines-kv-value">
                          {item.extraAddresses.length > 0 ? item.extraAddresses.join(" · ") : "—"}
                        </div>
                      </div>
                    </div>
                    {trafficSnapshot ? (
                      <LazycatTrafficCycleChart
                        serviceId={item.serviceId}
                        snapshot={trafficSnapshot}
                      />
                    ) : (
                      <div className="machines-traffic-panel machines-traffic-panel--empty">
                        <div className="machines-traffic-panel-copy">
                          <span className="machines-traffic-panel-label">账期流量</span>
                          <strong className="machines-traffic-empty-title">
                            暂无可绘制的小时样本
                          </strong>
                        </div>
                        <div className="machines-traffic-empty-copy">
                          面板同步成功后，系统会按小时把流量写入历史；当前账期至少有一条样本后才显示图表。
                        </div>
                      </div>
                    )}
                  </div>

                  {expanded ? (
                    <div className="machines-detail">
                      <div className="machines-detail-grid">
                        <div>
                          <span className="machines-kv-label">系统</span>
                          <div className="machines-kv-value">{item.os ?? "—"}</div>
                        </div>
                        <div>
                          <span className="machines-kv-label">首购价格</span>
                          <div className="machines-kv-value">{item.firstPrice ?? "—"}</div>
                        </div>
                        <div>
                          <span className="machines-kv-label">最近主站同步</span>
                          <div className="machines-kv-value mono">
                            {formatLocalTime(item.lastSiteSyncAt)}
                          </div>
                        </div>
                        <div>
                          <span className="machines-kv-label">最近面板同步</span>
                          <div className="machines-kv-value mono">
                            {formatLocalTime(item.lastPanelSyncAt)}
                          </div>
                        </div>
                      </div>

                      {trafficSnapshot ? (
                        <div className="machines-detail-block">
                          <div className="machines-detail-title">流量账期</div>
                          <div className="machines-detail-grid">
                            <div>
                              <span className="machines-kv-label">账期范围</span>
                              <div className="machines-kv-value mono">
                                {trafficSnapshot.rangeLabel}
                              </div>
                            </div>
                            <div>
                              <span className="machines-kv-label">当前累计</span>
                              <div className="machines-kv-value">{trafficSnapshot.usageLabel}</div>
                            </div>
                            <div>
                              <span className="machines-kv-label">重置日</span>
                              <div className="machines-kv-value">{item.traffic?.resetDay} 日</div>
                            </div>
                            <div>
                              <span className="machines-kv-label">最近重置</span>
                              <div className="machines-kv-value mono">
                                {trafficSnapshot.lastResetLabel}
                              </div>
                            </div>
                            <div>
                              <span className="machines-kv-label">剩余额度</span>
                              <div className="machines-kv-value">
                                {trafficSnapshot.remainingLabel}
                              </div>
                            </div>
                            <div>
                              <span className="machines-kv-label">展示口径</span>
                              <div className="machines-kv-value">
                                {item.traffic?.display ?? "GB"}
                              </div>
                            </div>
                          </div>
                          <div className="machines-detail-note muted">
                            图表只绘制当前账期内的真实小时采样，横向虚线表示当前流量上限。
                          </div>
                        </div>
                      ) : null}

                      <div className="machines-detail-block">
                        <div className="machines-detail-title">端口映射</div>
                        {item.portMappings.length === 0 ? (
                          <div className="empty machines-inline-empty">
                            当前没有端口映射缓存，或者这台机器的面板/NAT 代理暂时不可达。
                          </div>
                        ) : (
                          <div className="machines-port-list">
                            {item.portMappings.map((mapping, index) => (
                              <div
                                className="machines-port-row"
                                key={`${item.serviceId}:${mapping.family}:${index}`}
                              >
                                <span className="pill sm">{mapping.family.toUpperCase()}</span>
                                <span className="mono">
                                  {`${formatLazycatAddress(mapping.publicIp)}:${formatPortRange(mapping.publicPort, mapping.publicPortEnd)}`}
                                </span>
                                <span className="machines-port-arrow">→</span>
                                <span className="mono">
                                  {`${formatLazycatAddress(mapping.privateIp)}:${formatPortRange(mapping.privatePort, mapping.privatePortEnd)}`}
                                </span>
                                {mapping.protocol ? (
                                  <span className="pill sm badge">
                                    {mapping.protocol.toUpperCase()}
                                  </span>
                                ) : null}
                                {mapping.status ? (
                                  <span className="pill sm badge">{mapping.status}</span>
                                ) : null}
                                {mapping.description ? (
                                  <span className="machines-port-description">
                                    {mapping.description}
                                  </span>
                                ) : null}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>

                      {item.detailError ? (
                        <div className="machines-detail-note error">{item.detailError}</div>
                      ) : (
                        <div className="machines-detail-note muted">
                          {item.detailState === "stale"
                            ? "当前展示的是最近一次成功同步的缓存。"
                            : "面板字段已按当前缓存展示。"}
                        </div>
                      )}
                    </div>
                  ) : null}
                </section>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

type SettingsFieldKey =
  | "intervalMinutes"
  | "jitterPct"
  | "siteBaseUrl"
  | "siteAutofill"
  | "partitionCatalogChangeEnabled"
  | "regionPartitionChangeEnabled"
  | "siteRegionChangeEnabled"
  | "tgEnabled"
  | "tgTargets"
  | "tgBotToken"
  | "tgTestAction"
  | "wpEnableAction"
  | "wpTestAction"
  | "wpEnabled";

type SettingsSaveState = {
  kind: "idle" | "saving" | "saved" | "error";
  message: string | null;
};

type SettingsDraft = {
  intervalMinutesInput: string;
  jitterPctInput: string;
  siteBaseUrlInput: string;
  partitionCatalogChangeEnabled: boolean;
  regionPartitionChangeEnabled: boolean;
  siteRegionChangeEnabled: boolean;
  tgEnabled: boolean;
  tgTargets: string[];
  tgBotTokenInput: string;
  wpEnabled: boolean;
};

type SettingsPersistSnapshot = {
  poll: { intervalMinutes: number; jitterPct: number };
  siteBaseUrl: string | null;
  catalogRefresh: { autoIntervalHours: number | null };
  monitoringEvents: {
    partitionCatalogChangeEnabled: boolean;
    regionPartitionChangeEnabled: boolean;
    siteRegionChangeEnabled: boolean;
  };
  notifications: {
    telegram: { enabled: boolean; targets: string[] };
    webPush: { enabled: boolean };
  };
  telegramBotToken: string | null;
};

function normalizeOptionalText(value: string): string | null {
  const next = value.trim();
  return next ? next : null;
}

function normalizeTelegramTargets(values: string[]): string[] {
  const out: string[] = [];
  for (const value of values) {
    const next = value.trim();
    if (!next || out.includes(next)) continue;
    out.push(next);
  }
  return out;
}

function sameStringList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index]);
}

function parseStrictInteger(raw: string): number | null {
  const trimmed = raw.trim();
  if (!trimmed || !/^-?\d+$/.test(trimmed)) return null;
  const value = Number(trimmed);
  if (!Number.isSafeInteger(value)) return null;
  return value;
}

function parseStrictNumber(raw: string): number | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return null;
  return value;
}

function buildPersistedSnapshotFromSettings(settings: SettingsView): SettingsPersistSnapshot {
  return {
    poll: {
      intervalMinutes: settings.poll.intervalMinutes,
      jitterPct: settings.poll.jitterPct,
    },
    siteBaseUrl: settings.siteBaseUrl,
    catalogRefresh: {
      autoIntervalHours: settings.catalogRefresh.autoIntervalHours,
    },
    monitoringEvents: {
      partitionCatalogChangeEnabled: settings.monitoringEvents.partitionCatalogChangeEnabled,
      regionPartitionChangeEnabled: settings.monitoringEvents.regionPartitionChangeEnabled,
      siteRegionChangeEnabled: settings.monitoringEvents.siteRegionChangeEnabled,
    },
    notifications: {
      telegram: {
        enabled: settings.notifications.telegram.enabled,
        targets: normalizeTelegramTargets(settings.notifications.telegram.targets ?? []),
      },
      webPush: {
        enabled: settings.notifications.webPush.enabled,
      },
    },
    telegramBotToken: null,
  };
}

function clonePersistedSnapshot(snapshot: SettingsPersistSnapshot): SettingsPersistSnapshot {
  return {
    poll: { ...snapshot.poll },
    siteBaseUrl: snapshot.siteBaseUrl,
    catalogRefresh: { ...snapshot.catalogRefresh },
    monitoringEvents: { ...snapshot.monitoringEvents },
    notifications: {
      telegram: {
        enabled: snapshot.notifications.telegram.enabled,
        targets: [...snapshot.notifications.telegram.targets],
      },
      webPush: { ...snapshot.notifications.webPush },
    },
    telegramBotToken: snapshot.telegramBotToken,
  };
}

function snapshotToOnSaveInput(
  snapshot: SettingsPersistSnapshot,
): SettingsView & { telegramBotToken?: string | null } {
  return {
    poll: {
      intervalMinutes: snapshot.poll.intervalMinutes,
      jitterPct: snapshot.poll.jitterPct,
    },
    siteBaseUrl: snapshot.siteBaseUrl,
    catalogRefresh: {
      autoIntervalHours: snapshot.catalogRefresh.autoIntervalHours,
    },
    monitoringEvents: {
      partitionCatalogChangeEnabled: snapshot.monitoringEvents.partitionCatalogChangeEnabled,
      regionPartitionChangeEnabled: snapshot.monitoringEvents.regionPartitionChangeEnabled,
      siteRegionChangeEnabled: snapshot.monitoringEvents.siteRegionChangeEnabled,
    },
    notifications: {
      telegram: {
        enabled: snapshot.notifications.telegram.enabled,
        configured: false,
        targets: [...snapshot.notifications.telegram.targets],
      },
      webPush: {
        enabled: snapshot.notifications.webPush.enabled,
      },
    },
    telegramBotToken: snapshot.telegramBotToken,
  };
}

function clearFeedbackTimer(timerRef: { current: number | null }) {
  if (timerRef.current !== null) {
    window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }
}

export function SettingsViewPanel({
  bootstrap,
  about,
  aboutLoading,
  aboutError,
  onCheckUpdate,
  onSave,
  onLazycatLogin,
  onLazycatDisconnect,
  onLazycatSync,
  onRefreshLazycatAccount,
}: {
  bootstrap: BootstrapResponse;
  about: AboutResponse | null;
  aboutLoading: boolean;
  aboutError: string | null;
  onCheckUpdate: () => Promise<void>;
  onSave: (next: SettingsView & { telegramBotToken?: string | null }) => Promise<SettingsView>;
  onLazycatLogin: (email: string, password: string) => Promise<LazycatAccountView>;
  onLazycatDisconnect: () => Promise<void>;
  onLazycatSync: () => Promise<LazycatAccountView>;
  onRefreshLazycatAccount: () => Promise<LazycatAccountView>;
}) {
  const [intervalMinutesInput, setIntervalMinutesInput] = useState<string>(
    String(bootstrap.settings.poll.intervalMinutes),
  );
  const [jitterPctInput, setJitterPctInput] = useState<string>(
    String(bootstrap.settings.poll.jitterPct),
  );
  const [siteBaseUrlInput, setSiteBaseUrlInput] = useState<string>(
    bootstrap.settings.siteBaseUrl ?? "",
  );
  const [partitionCatalogChangeEnabled, setPartitionCatalogChangeEnabled] = useState<boolean>(
    bootstrap.settings.monitoringEvents.partitionCatalogChangeEnabled,
  );
  const [regionPartitionChangeEnabled, setRegionPartitionChangeEnabled] = useState<boolean>(
    bootstrap.settings.monitoringEvents.regionPartitionChangeEnabled,
  );
  const [siteRegionChangeEnabled, setSiteRegionChangeEnabled] = useState<boolean>(
    bootstrap.settings.monitoringEvents.siteRegionChangeEnabled,
  );
  const [tgEnabled, setTgEnabled] = useState<boolean>(
    bootstrap.settings.notifications.telegram.enabled,
  );
  const [tgTargets, setTgTargets] = useState<string[]>(
    normalizeTelegramTargets(bootstrap.settings.notifications.telegram.targets ?? []),
  );
  const [tgTargetDraftInput, setTgTargetDraftInput] = useState<string>("");
  const [tgBotTokenInput, setTgBotTokenInput] = useState<string>("");
  const [tgTestPending, setTgTestPending] = useState<boolean>(false);
  const [tgTestResult, setTgTestResult] = useState<TelegramTestResponse | null>(null);
  const [wpEnabled, setWpEnabled] = useState<boolean>(
    bootstrap.settings.notifications.webPush.enabled,
  );
  const [lazycatAccount, setLazycatAccount] = useState<LazycatAccountView>(() => bootstrap.lazycat);
  const [lazycatEmailInput, setLazycatEmailInput] = useState<string>(bootstrap.lazycat.email ?? "");
  const [lazycatPasswordInput, setLazycatPasswordInput] = useState<string>("");
  const [lazycatLoginPending, setLazycatLoginPending] = useState<boolean>(false);
  const [lazycatSyncPending, setLazycatSyncPending] = useState<boolean>(false);
  const [lazycatDisconnectPending, setLazycatDisconnectPending] = useState<boolean>(false);
  const [lazycatActionError, setLazycatActionError] = useState<string | null>(null);
  const [wpStatus, setWpStatus] = useState<string | null>(null);
  const [wpTestPending, setWpTestPending] = useState<boolean>(false);
  const [wpTestStatus, setWpTestStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState<boolean>(false);
  const [saveState, setSaveState] = useState<SettingsSaveState>({ kind: "idle", message: null });
  const [fieldErrors, setFieldErrors] = useState<Partial<Record<SettingsFieldKey, string>>>({});
  const [lastPersisted, setLastPersisted] = useState<SettingsPersistSnapshot>(() =>
    buildPersistedSnapshotFromSettings(bootstrap.settings),
  );

  const autosaveTimerRef = useRef<number | null>(null);
  const tgTestSuccessTimerRef = useRef<number | null>(null);
  const wpTestSuccessTimerRef = useRef<number | null>(null);
  const tgTestButtonRef = useRef<HTMLButtonElement | null>(null);
  const wpEnableButtonRef = useRef<HTMLButtonElement | null>(null);
  const wpTestButtonRef = useRef<HTMLButtonElement | null>(null);
  const saveSeqRef = useRef(0);
  const lastPersistedRef = useRef(lastPersisted);

  useEffect(() => {
    lastPersistedRef.current = lastPersisted;
  }, [lastPersisted]);

  useEffect(
    () => () => {
      clearFeedbackTimer(autosaveTimerRef);
      clearFeedbackTimer(tgTestSuccessTimerRef);
      clearFeedbackTimer(wpTestSuccessTimerRef);
    },
    [],
  );

  useEffect(() => {
    setLazycatAccount(bootstrap.lazycat);
    setLazycatEmailInput((prev) => (prev.trim() ? prev : (bootstrap.lazycat.email ?? "")));
  }, [bootstrap.lazycat]);

  useEffect(() => {
    if (!lazycatAccount.connected) return;
    if (!(lazycatAccount.state === "syncing" || lazycatAccount.state === "authenticating")) {
      return;
    }
    let cancelled = false;
    const id = window.setInterval(() => {
      void onRefreshLazycatAccount()
        .then((account) => {
          if (!cancelled) setLazycatAccount(account);
        })
        .catch(() => {
          // Ignore polling errors; explicit actions surface the failure.
        });
    }, 3_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [lazycatAccount.connected, lazycatAccount.state, onRefreshLazycatAccount]);

  const clearTgTestStatus = useCallback(() => {
    clearFeedbackTimer(tgTestSuccessTimerRef);
    setTgTestResult(null);
  }, []);

  const showTgTestStatus = useCallback((result: TelegramTestResponse) => {
    clearFeedbackTimer(tgTestSuccessTimerRef);
    setTgTestResult(result);
    tgTestSuccessTimerRef.current = window.setTimeout(() => {
      tgTestSuccessTimerRef.current = null;
      setTgTestResult(null);
    }, SETTINGS_TEST_SUCCESS_BUBBLE_MS);
  }, []);

  const clearWpTestStatus = useCallback(() => {
    clearFeedbackTimer(wpTestSuccessTimerRef);
    setWpTestStatus(null);
  }, []);

  const showWpTestStatus = useCallback((message: string) => {
    clearFeedbackTimer(wpTestSuccessTimerRef);
    setWpTestStatus(message);
    wpTestSuccessTimerRef.current = window.setTimeout(() => {
      wpTestSuccessTimerRef.current = null;
      setWpTestStatus(null);
    }, SETTINGS_TEST_SUCCESS_BUBBLE_MS);
  }, []);

  const wpKey = bootstrap.settings.notifications.webPush.vapidPublicKey;
  const wpSupported = "serviceWorker" in navigator && "PushManager" in window;

  const aboutVersionDisplay = formatVersionDisplay(
    about?.version ?? import.meta.env.VITE_APP_VERSION,
  );
  const aboutWebDistBuildId = about?.webDistBuildId?.trim() ? about.webDistBuildId.trim() : null;
  const aboutRepoUrl = about?.repoUrl?.trim() ? about.repoUrl.trim() : null;
  const aboutRepoBaseUrl = aboutRepoUrl ? aboutRepoUrl.replace(/\/+$/, "") : null;
  const update = about?.update ?? null;
  const updateStatus: AboutUpdate["status"] = update?.status ?? "error";
  const updateBadgeClass =
    updateStatus === "ok"
      ? "pill badge on"
      : updateStatus === "disabled"
        ? "pill badge disabled"
        : "pill badge err";
  const updateCheckedAt = update?.checkedAt ?? null;
  const updateLatestVersion = update?.latestVersion ?? null;
  const updateLatestUrl = update?.latestUrl ?? null;
  const updateAvailable = update?.updateAvailable ?? false;
  const updateMessage = update?.message ?? aboutError;

  const topologyRefreshHours = lastPersisted.catalogRefresh.autoIntervalHours ?? 1;

  const buildDraft = useCallback(
    (overrides: Partial<SettingsDraft> = {}): SettingsDraft => ({
      intervalMinutesInput,
      jitterPctInput,
      siteBaseUrlInput,
      partitionCatalogChangeEnabled,
      regionPartitionChangeEnabled,
      siteRegionChangeEnabled,
      tgEnabled,
      tgTargets,
      tgBotTokenInput,
      wpEnabled,
      ...overrides,
    }),
    [
      intervalMinutesInput,
      jitterPctInput,
      siteBaseUrlInput,
      partitionCatalogChangeEnabled,
      regionPartitionChangeEnabled,
      siteRegionChangeEnabled,
      tgEnabled,
      tgTargets,
      tgBotTokenInput,
      wpEnabled,
    ],
  );

  const setFieldError = useCallback((field: SettingsFieldKey, message: string | null) => {
    setFieldErrors((prev) => {
      const prevMessage = prev[field] ?? null;
      if (prevMessage === message) return prev;
      const next = { ...prev };
      if (message) {
        next[field] = message;
      } else {
        delete next[field];
      }
      return next;
    });
  }, []);

  const validateDraftField = useCallback(
    (
      field: SettingsFieldKey,
      draft: SettingsDraft,
      _reportInvalid: boolean,
    ): { valid: true; value: number | string | null } | { valid: false; message: string } => {
      let message: string | null = null;
      let value: number | string | null = null;

      if (field === "intervalMinutes") {
        const parsed = parseStrictInteger(draft.intervalMinutesInput);
        if (parsed === null || parsed < 1) {
          message = "查询频率必须是 >= 1 的整数";
        } else {
          value = parsed;
        }
      } else if (field === "jitterPct") {
        const parsed = parseStrictNumber(draft.jitterPctInput);
        if (parsed === null || parsed < 0 || parsed > 1) {
          message = "抖动比例必须在 0..1 之间";
        } else {
          value = parsed;
        }
      } else if (field === "siteBaseUrl") {
        const trimmed = draft.siteBaseUrlInput.trim();
        if (!trimmed) {
          value = null;
        } else {
          try {
            const parsed = new URL(trimmed);
            const protocol = parsed.protocol.toLowerCase();
            if (protocol !== "http:" && protocol !== "https:") {
              message = "站点地址必须是 http 或 https";
            } else {
              value = trimmed;
            }
          } catch {
            message = "站点地址格式不正确";
          }
        }
      } else {
        value = null;
      }

      setFieldError(field, message);

      if (message) {
        return { valid: false, message };
      }
      return { valid: true, value };
    },
    [setFieldError],
  );

  const validateDraft = useCallback(
    (draft: SettingsDraft, reportInvalid: boolean) => {
      const intervalMinutes = validateDraftField("intervalMinutes", draft, reportInvalid);
      const jitterPct = validateDraftField("jitterPct", draft, reportInvalid);
      const siteBaseUrl = validateDraftField("siteBaseUrl", draft, reportInvalid);
      const computedInvalid: SettingsFieldKey[] = [];
      if (!intervalMinutes.valid) computedInvalid.push("intervalMinutes");
      if (!jitterPct.valid) computedInvalid.push("jitterPct");
      if (!siteBaseUrl.valid) computedInvalid.push("siteBaseUrl");

      return {
        intervalMinutes,
        jitterPct,
        siteBaseUrl,
        invalidFields: computedInvalid,
      };
    },
    [validateDraftField],
  );

  const persistDraft = useCallback(
    async (draft: SettingsDraft, reportInvalid: boolean, sourceField: SettingsFieldKey | null) => {
      const validated = validateDraft(draft, reportInvalid);
      const next = clonePersistedSnapshot(lastPersistedRef.current);
      let changed = false;

      if (validated.intervalMinutes.valid) {
        const value = validated.intervalMinutes.value as number;
        if (value !== next.poll.intervalMinutes) {
          next.poll.intervalMinutes = value;
          changed = true;
        }
      }

      if (validated.jitterPct.valid) {
        const value = validated.jitterPct.value as number;
        if (value !== next.poll.jitterPct) {
          next.poll.jitterPct = value;
          changed = true;
        }
      }

      if (validated.siteBaseUrl.valid) {
        const value = validated.siteBaseUrl.value as string | null;
        if (value !== next.siteBaseUrl) {
          next.siteBaseUrl = value;
          changed = true;
        }
      }

      if (
        draft.partitionCatalogChangeEnabled !== next.monitoringEvents.partitionCatalogChangeEnabled
      ) {
        next.monitoringEvents.partitionCatalogChangeEnabled = draft.partitionCatalogChangeEnabled;
        changed = true;
      }

      if (
        draft.regionPartitionChangeEnabled !== next.monitoringEvents.regionPartitionChangeEnabled
      ) {
        next.monitoringEvents.regionPartitionChangeEnabled = draft.regionPartitionChangeEnabled;
        changed = true;
      }

      if (draft.siteRegionChangeEnabled !== next.monitoringEvents.siteRegionChangeEnabled) {
        next.monitoringEvents.siteRegionChangeEnabled = draft.siteRegionChangeEnabled;
        changed = true;
      }

      if (draft.tgEnabled !== next.notifications.telegram.enabled) {
        next.notifications.telegram.enabled = draft.tgEnabled;
        changed = true;
      }

      const nextTargets = normalizeTelegramTargets(draft.tgTargets);
      if (!sameStringList(nextTargets, next.notifications.telegram.targets)) {
        next.notifications.telegram.targets = nextTargets;
        changed = true;
      }

      const nextBotToken = normalizeOptionalText(draft.tgBotTokenInput);
      if (nextBotToken !== next.telegramBotToken) {
        next.telegramBotToken = nextBotToken;
        changed = true;
      }

      if (draft.wpEnabled !== next.notifications.webPush.enabled) {
        next.notifications.webPush.enabled = draft.wpEnabled;
        changed = true;
      }

      if (!changed) {
        setSaveState((prev) => (prev.kind === "saving" ? prev : { kind: "idle", message: null }));
        return { requested: false, saved: false, invalidFields: validated.invalidFields };
      }

      const seq = saveSeqRef.current + 1;
      saveSeqRef.current = seq;
      if (sourceField) {
        setFieldError(sourceField, null);
      }
      setSaveState({ kind: "saving", message: "自动保存中…" });

      try {
        const updated = await onSave(snapshotToOnSaveInput(next));
        if (seq !== saveSeqRef.current) {
          return { requested: true, saved: false, invalidFields: validated.invalidFields };
        }

        const committed = buildPersistedSnapshotFromSettings(updated);
        committed.telegramBotToken = next.telegramBotToken;
        setLastPersisted(committed);

        if (sourceField) {
          setFieldError(sourceField, null);
        }

        if (validated.invalidFields.length > 0) {
          setSaveState({ kind: "saved", message: "已保存合法字段，仍有字段需要修正" });
        } else {
          setSaveState({ kind: "saved", message: "已自动保存" });
        }

        return { requested: true, saved: true, invalidFields: validated.invalidFields };
      } catch (e) {
        if (seq !== saveSeqRef.current) {
          return { requested: true, saved: false, invalidFields: validated.invalidFields };
        }
        const msg = e instanceof Error ? e.message : String(e);
        if (sourceField) {
          setFieldError(sourceField, msg);
        }
        setSaveState({ kind: "error", message: msg });
        return { requested: true, saved: false, invalidFields: validated.invalidFields };
      }
    },
    [onSave, setFieldError, validateDraft],
  );

  const scheduleAutosave = useCallback(
    (overrides: Partial<SettingsDraft> = {}, sourceField: SettingsFieldKey | null = null) => {
      if (autosaveTimerRef.current !== null) {
        window.clearTimeout(autosaveTimerRef.current);
      }
      const draft = buildDraft(overrides);
      autosaveTimerRef.current = window.setTimeout(() => {
        void persistDraft(draft, true, sourceField);
      }, 800);
    },
    [buildDraft, persistDraft],
  );

  const flushAutosaveImmediate = useCallback(
    async (overrides: Partial<SettingsDraft> = {}, sourceField: SettingsFieldKey | null = null) => {
      if (autosaveTimerRef.current !== null) {
        window.clearTimeout(autosaveTimerRef.current);
        autosaveTimerRef.current = null;
      }
      const draft = buildDraft(overrides);
      return persistDraft(draft, true, sourceField);
    },
    [buildDraft, persistDraft],
  );

  const renderFieldError = (
    field: SettingsFieldKey,
    inline = false,
    anchorRef?: RefObject<HTMLElement | null>,
    testId?: string,
  ) => (
    <SettingsFeedbackBubble
      anchorRef={anchorRef}
      inline={inline}
      message={fieldErrors[field] ?? null}
      onClose={() => setFieldError(field, null)}
      open={Boolean(fieldErrors[field])}
      testId={testId}
      tone="error"
    />
  );

  const renderActionFeedback = (
    field: SettingsFieldKey,
    successMessage: string | null,
    onCloseSuccess: () => void,
    successTestId?: string,
    inlineSuccess = false,
    successAnchorRef?: RefObject<HTMLElement | null>,
  ) => {
    const errorMessage = fieldErrors[field];
    const activeMessage = errorMessage ?? successMessage;
    const activeTone = errorMessage ? "error" : "success";

    return (
      <SettingsFeedbackBubble
        anchorRef={successAnchorRef}
        inline={inlineSuccess}
        message={activeMessage}
        onClose={errorMessage ? () => setFieldError(field, null) : onCloseSuccess}
        open={Boolean(activeMessage)}
        testId={successTestId}
        tone={activeTone}
      />
    );
  };

  const commitTelegramTargetDraft = useCallback(async (): Promise<string[]> => {
    const nextValue = tgTargetDraftInput.trim();
    setFieldError("tgTargets", null);
    if (!nextValue) return tgTargets;
    const nextTargets = normalizeTelegramTargets([...tgTargets, nextValue]);
    setTgTargetDraftInput("");
    setTgTargets(nextTargets);
    clearTgTestStatus();
    await flushAutosaveImmediate({ tgTargets: nextTargets }, "tgTargets");
    return nextTargets;
  }, [clearTgTestStatus, flushAutosaveImmediate, setFieldError, tgTargetDraftInput, tgTargets]);

  const removeTelegramTarget = useCallback(
    async (targetToRemove: string) => {
      const nextTargets = tgTargets.filter((target) => target !== targetToRemove);
      setFieldError("tgTargets", null);
      setTgTargets(nextTargets);
      clearTgTestStatus();
      await flushAutosaveImmediate({ tgTargets: nextTargets }, "tgTargets");
    },
    [clearTgTestStatus, flushAutosaveImmediate, setFieldError, tgTargets],
  );

  const renderTelegramTestFeedback = () => {
    const errorMessage = fieldErrors.tgTestAction;
    const tone =
      errorMessage || tgTestResult?.status === "error"
        ? "error"
        : tgTestResult?.status === "partial_success"
          ? "neutral"
          : "success";
    const children = tgTestResult ? (
      <div className="settings-feedback-content">
        <div className="settings-feedback-title">
          {tgTestResult.status === "success"
            ? "已全部发送"
            : tgTestResult.status === "partial_success"
              ? "部分目标发送成功"
              : "全部目标发送失败"}
        </div>
        {tgTestResult.results.map((result) => (
          <div className="settings-feedback-row" key={`${result.target}:${result.status}`}>
            <span className="settings-feedback-key">{result.target}</span>
            <span className={notificationStatusClass(result.status)}>
              {notificationStatusLabel(result.status)}
            </span>
            {result.error ? <span className="settings-feedback-line">{result.error}</span> : null}
          </div>
        ))}
      </div>
    ) : null;

    return (
      <SettingsFeedbackBubble
        anchorRef={tgTestButtonRef}
        inline
        message={errorMessage ?? null}
        onClose={errorMessage ? () => setFieldError("tgTestAction", null) : clearTgTestStatus}
        open={Boolean(errorMessage || children)}
        testId="settings-feedback-tg-test"
        tone={tone}
      >
        {children}
      </SettingsFeedbackBubble>
    );
  };

  const renderSaveStateMessage = () => {
    if (!saveState.message) return null;
    if (saveState.kind === "saving" || saveState.kind === "saved") {
      return (
        <div className="muted" style={{ marginTop: 12 }}>
          {saveState.message}
        </div>
      );
    }
    return null;
  };

  return (
    <div className="panel" data-testid="page-settings">
      <div className="panel-section">
        <div className="panel-title">轮询（Polling）</div>
        <div className="settings-grid">
          <div>查询频率（分钟）</div>
          <div className="settings-input-wrap">
            <div className="pill num" style={{ width: "120px" }}>
              <input
                type="number"
                min={1}
                value={intervalMinutesInput}
                onChange={(e) => {
                  const value = e.target.value;
                  setIntervalMinutesInput(value);
                  void validateDraftField(
                    "intervalMinutes",
                    buildDraft({ intervalMinutesInput: value }),
                    false,
                  );
                  scheduleAutosave({ intervalMinutesInput: value }, "intervalMinutes");
                }}
                onBlur={() => {
                  void validateDraftField("intervalMinutes", buildDraft(), true);
                }}
              />
            </div>
            {renderFieldError("intervalMinutes")}
          </div>
          <div className="hint">默认 1；最小 1</div>

          <div>抖动比例（0..1）</div>
          <div className="settings-input-wrap">
            <div className="pill num" style={{ width: "120px" }}>
              <input
                type="number"
                min={0}
                max={1}
                step={0.01}
                value={jitterPctInput}
                onChange={(e) => {
                  const value = e.target.value;
                  setJitterPctInput(value);
                  void validateDraftField(
                    "jitterPct",
                    buildDraft({ jitterPctInput: value }),
                    false,
                  );
                  scheduleAutosave({ jitterPctInput: value }, "jitterPct");
                }}
                onBlur={() => {
                  void validateDraftField("jitterPct", buildDraft(), true);
                }}
              />
            </div>
            {renderFieldError("jitterPct")}
          </div>
          <div className="hint">实际间隔 = interval × (1 ± jitter)</div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">站点地址（用于通知跳转链接）</div>
        <div className="panel-subtitle">默认值：window.location.origin（用户可修改）</div>
        <div className="controls settings-site-controls">
          <div className="settings-input-wrap settings-input-wide">
            <div className="pill" style={{ width: "100%" }}>
              <input
                placeholder={window.location.origin}
                value={siteBaseUrlInput}
                onChange={(e) => {
                  const value = e.target.value;
                  setSiteBaseUrlInput(value);
                  setFieldError("siteAutofill", null);
                  void validateDraftField(
                    "siteBaseUrl",
                    buildDraft({ siteBaseUrlInput: value }),
                    false,
                  );
                  scheduleAutosave({ siteBaseUrlInput: value }, "siteBaseUrl");
                }}
                onBlur={() => {
                  void validateDraftField("siteBaseUrl", buildDraft(), true);
                }}
              />
            </div>
            {renderFieldError("siteBaseUrl")}
          </div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className="pill warn center settings-site-autofill"
              style={{ width: "160px" }}
              onClick={() => {
                const value = window.location.origin;
                setSiteBaseUrlInput(value);
                setFieldError("siteAutofill", null);
                void validateDraftField(
                  "siteBaseUrl",
                  buildDraft({ siteBaseUrlInput: value }),
                  false,
                );
                void flushAutosaveImmediate({ siteBaseUrlInput: value }, "siteAutofill");
              }}
            >
              自动填充
            </button>
            {renderFieldError("siteAutofill")}
          </div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">目录拓扑复扫（Catalog topology refresh）</div>
        <div className="panel-subtitle">
          这项频率改为系统托管，不再单独设置；新区/新地域先做轻探测，再由正式复扫收敛
        </div>
        <div className="settings-grid">
          <div>新区轻探测</div>
          <div className="settings-action-wrap">
            <span className="pill sm center on" style={{ width: "108px" }}>
              {`${TOPOLOGY_PROBE_MINUTES} 分钟`}
            </span>
          </div>
          <div className="hint">
            只扫 root + fid，把新国家/新地域尽快纳入后续 5 分钟 discovery 范围
          </div>

          <div>正式拓扑复扫</div>
          <div className="settings-action-wrap">
            <span className="pill sm center on" style={{ width: "108px" }}>
              {`${topologyRefreshHours} 小时`}
            </span>
          </div>
          <div className="hint">
            用于保守收敛拓扑变化与移除目标；已知 URL 的新机发现仍走 5 分钟轻扫
          </div>

          <div>套餐变更</div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className={`pill sm center ${partitionCatalogChangeEnabled ? "on" : ""}`}
              style={{ width: "92px" }}
              onClick={() => {
                const next = !partitionCatalogChangeEnabled;
                setPartitionCatalogChangeEnabled(next);
                setFieldError("partitionCatalogChangeEnabled", null);
                void flushAutosaveImmediate(
                  { partitionCatalogChangeEnabled: next },
                  "partitionCatalogChangeEnabled",
                );
              }}
            >
              {partitionCatalogChangeEnabled ? "启用" : "关闭"}
            </button>
            {renderFieldError("partitionCatalogChangeEnabled")}
          </div>
          <div className="hint">
            启用后：仅通知已在 products
            中开启“国家监控”的国家直属套餐，以及已开启“可用区监控”的可用区套餐，关注套餐新增与删除。
          </div>

          <div>可用区变更</div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className={`pill sm center ${regionPartitionChangeEnabled ? "on" : ""}`}
              style={{ width: "92px" }}
              onClick={() => {
                const next = !regionPartitionChangeEnabled;
                setRegionPartitionChangeEnabled(next);
                setFieldError("regionPartitionChangeEnabled", null);
                void flushAutosaveImmediate(
                  { regionPartitionChangeEnabled: next },
                  "regionPartitionChangeEnabled",
                );
              }}
            >
              {regionPartitionChangeEnabled ? "启用" : "关闭"}
            </button>
            {renderFieldError("regionPartitionChangeEnabled")}
          </div>
          <div className="hint">
            启用后：仅通知已在 products 中开启“国家监控”的国家，关注可用区新增与删除。
          </div>

          <div>国家变更</div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className={`pill sm center ${siteRegionChangeEnabled ? "on" : ""}`}
              style={{ width: "92px" }}
              onClick={() => {
                const next = !siteRegionChangeEnabled;
                setSiteRegionChangeEnabled(next);
                setFieldError("siteRegionChangeEnabled", null);
                void flushAutosaveImmediate(
                  { siteRegionChangeEnabled: next },
                  "siteRegionChangeEnabled",
                );
              }}
            >
              {siteRegionChangeEnabled ? "启用" : "关闭"}
            </button>
            {renderFieldError("siteRegionChangeEnabled")}
          </div>
          <div className="hint">启用后：全站范围关注国家新增与删除。</div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">通知（Notifications）</div>

        {renderSaveStateMessage()}

        <div className="controls" style={{ marginTop: "16px" }}>
          <div className="panel-title" style={{ fontSize: "16px" }}>
            Telegram
          </div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className={`pill sm center ${tgEnabled ? "on" : ""}`}
              style={{ width: "92px" }}
              onClick={() => {
                const next = !tgEnabled;
                clearTgTestStatus();
                setTgEnabled(next);
                setFieldError("tgEnabled", null);
                void flushAutosaveImmediate({ tgEnabled: next }, "tgEnabled");
              }}
            >
              {tgEnabled ? "启用" : "关闭"}
            </button>
            {renderFieldError("tgEnabled")}
          </div>
        </div>

        <div className="settings-row">
          <div>Bot Token（不回显）</div>
          <div className="settings-input-wrap">
            <div className="pill">
              <input
                type="password"
                placeholder={
                  bootstrap.settings.notifications.telegram.configured ? "••••••••••••••••" : ""
                }
                value={tgBotTokenInput}
                onChange={(e) => {
                  const value = e.target.value;
                  clearTgTestStatus();
                  setTgBotTokenInput(value);
                  setFieldError("tgBotToken", null);
                  scheduleAutosave({ tgBotTokenInput: value }, "tgBotToken");
                }}
              />
            </div>
            {renderFieldError("tgBotToken")}
          </div>
        </div>

        <div className="settings-row" style={{ marginTop: "16px" }}>
          <div>Targets</div>
          <div className="settings-input-wrap">
            <div className="settings-tag-editor">
              <div className="settings-tag-field">
                {tgTargets.map((target) => (
                  <button
                    className="settings-tag-chip"
                    key={target}
                    onClick={() => {
                      void removeTelegramTarget(target);
                    }}
                    type="button"
                  >
                    <span>{target}</span>
                    <span className="settings-tag-chip-close" aria-hidden="true">
                      ×
                    </span>
                  </button>
                ))}
                <input
                  className="settings-tag-inline-input"
                  placeholder="@channel 或 -1001234567890"
                  value={tgTargetDraftInput}
                  onChange={(e) => {
                    clearTgTestStatus();
                    setTgTargetDraftInput(e.target.value);
                    setFieldError("tgTargets", null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === ",") {
                      e.preventDefault();
                      void commitTelegramTargetDraft();
                    }
                  }}
                  onBlur={() => {
                    void commitTelegramTargetDraft();
                  }}
                />
              </div>
              <div className="settings-tag-actions">
                <span className="hint">回车或逗号添加；点击目标可删除</span>
                <button
                  className="settings-tag-add-button"
                  onClick={() => {
                    void commitTelegramTargetDraft();
                  }}
                  type="button"
                >
                  添加目标
                </button>
              </div>
            </div>
            {renderFieldError("tgTargets")}
          </div>
        </div>

        <div className="settings-actions">
          <div className="settings-action-wrap settings-action-wrap-bubble-left settings-action-wrap-inline-feedback">
            <button
              ref={tgTestButtonRef}
              type="button"
              className="pill warn center btn"
              disabled={saving || tgTestPending}
              onClick={async () => {
                setTgTestPending(true);
                clearTgTestStatus();
                setFieldError("tgTestAction", null);
                try {
                  const nextTargets = await commitTelegramTargetDraft();
                  const res = await fetch("/api/notifications/telegram/test", {
                    method: "POST",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({
                      botToken: tgBotTokenInput.trim() ? tgBotTokenInput.trim() : null,
                      targets: nextTargets,
                      text: null,
                    }),
                  });
                  const bodyText = await res.text();
                  const parsed = parseJsonText(bodyText) as TelegramTestResponse | ApiError | null;
                  if (!res.ok) {
                    if (parsed && "results" in parsed) {
                      setTgTestResult(parsed as TelegramTestResponse);
                      setFieldError("tgTestAction", null);
                    } else {
                      throw new Error(formatResponseErrorMessage(res, bodyText, parsed));
                    }
                  } else {
                    if (!parsed || !("results" in parsed)) {
                      throw new Error("测试接口返回了无效响应");
                    }
                    showTgTestStatus(parsed as TelegramTestResponse);
                    setFieldError("tgTestAction", null);
                  }
                } catch (e) {
                  clearTgTestStatus();
                  setFieldError("tgTestAction", e instanceof Error ? e.message : String(e));
                } finally {
                  setTgTestPending(false);
                }
              }}
            >
              {tgTestPending ? "测试中…" : "测试 Telegram"}
            </button>
            {renderTelegramTestFeedback()}
          </div>
        </div>

        <div className="line-inner" />

        <div className="controls" style={{ marginTop: 0 }}>
          <div className="panel-title" style={{ fontSize: "16px" }}>
            Web Push（浏览器推送）
          </div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className={`pill sm center ${wpEnabled ? "on" : ""}`}
              style={{ width: "92px" }}
              onClick={() => {
                const next = !wpEnabled;
                clearWpTestStatus();
                setWpEnabled(next);
                setFieldError("wpEnabled", null);
                void flushAutosaveImmediate({ wpEnabled: next }, "wpEnabled");
              }}
            >
              {wpEnabled ? "启用" : "关闭"}
            </button>
            {renderFieldError("wpEnabled")}
          </div>
        </div>

        <div className="panel-subtitle" style={{ marginTop: "16px" }}>
          请求权限 → 注册 Service Worker → 上传 subscription
        </div>
        {wpEnabled ? (
          <div className="muted">
            {wpKey
              ? "需要 HTTPS（或 localhost）才能完成订阅。"
              : "缺少 VAPID public key：请在服务端设置 CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY。"}
          </div>
        ) : null}
        {wpStatus ? <div className="muted">{wpStatus}</div> : null}
        <div className="settings-actions">
          <div className="settings-action-wrap">
            <button
              ref={wpEnableButtonRef}
              type="button"
              className="pill warn center btn"
              disabled={saving || !wpKey || !wpSupported}
              onClick={async () => {
                setSaving(true);
                setWpStatus(null);
                setFieldError("wpEnableAction", null);
                try {
                  setWpEnabled(true);
                  const saveResult = await flushAutosaveImmediate({ wpEnabled: true }, "wpEnabled");
                  if (saveResult.requested && !saveResult.saved) {
                    throw new Error("设置保存失败，无法启用推送");
                  }

                  const perm = await Notification.requestPermission();
                  if (perm !== "granted") {
                    throw new Error("浏览器未授予通知权限");
                  }

                  await navigator.serviceWorker.register("/sw.js");
                  const ready = await navigator.serviceWorker.ready;
                  if (!wpKey) throw new Error("缺少 VAPID public key");

                  const sub = await ready.pushManager.subscribe({
                    userVisibleOnly: true,
                    applicationServerKey: urlBase64ToUint8Array(wpKey) as unknown as BufferSource,
                  });

                  const json = sub.toJSON() as {
                    endpoint?: string;
                    keys?: { p256dh?: string; auth?: string };
                  };
                  if (!json.endpoint || !json.keys?.p256dh || !json.keys?.auth) {
                    throw new Error("订阅信息不完整");
                  }

                  await api("/api/notifications/web-push/subscriptions", {
                    method: "POST",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({
                      subscription: {
                        endpoint: json.endpoint,
                        keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
                      },
                    }),
                  });

                  setWpStatus("订阅已上传。");
                  setFieldError("wpEnableAction", null);
                } catch (e) {
                  setFieldError("wpEnableAction", e instanceof Error ? e.message : String(e));
                } finally {
                  setSaving(false);
                }
              }}
            >
              启用推送
            </button>
            {renderFieldError(
              "wpEnableAction",
              true,
              wpEnableButtonRef,
              "settings-feedback-wp-enable-error",
            )}
          </div>

          <div className="settings-action-wrap settings-action-wrap-bubble-left settings-action-wrap-inline-feedback">
            <button
              ref={wpTestButtonRef}
              type="button"
              className="pill warn center btn"
              disabled={saving || wpTestPending}
              onClick={async () => {
                setWpTestPending(true);
                clearWpTestStatus();
                setFieldError("wpTestAction", null);
                try {
                  if (!wpSupported) throw new Error("当前浏览器不支持 Push");

                  const perm = await Notification.requestPermission();
                  if (perm !== "granted") {
                    throw new Error("浏览器未授予通知权限");
                  }

                  await navigator.serviceWorker.register("/sw.js");
                  const ready = await navigator.serviceWorker.ready;

                  let sub = await ready.pushManager.getSubscription();
                  if (!sub) {
                    if (!wpKey) throw new Error("缺少 VAPID public key");
                    sub = await ready.pushManager.subscribe({
                      userVisibleOnly: true,
                      applicationServerKey: urlBase64ToUint8Array(wpKey) as unknown as BufferSource,
                    });
                  }

                  const json = sub.toJSON() as {
                    endpoint?: string;
                    keys?: { p256dh?: string; auth?: string };
                  };
                  if (!json.endpoint || !json.keys?.p256dh || !json.keys?.auth) {
                    throw new Error("订阅信息不完整");
                  }

                  await api("/api/notifications/web-push/subscriptions", {
                    method: "POST",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({
                      subscription: {
                        endpoint: json.endpoint,
                        keys: { p256dh: json.keys.p256dh, auth: json.keys.auth },
                      },
                    }),
                  });

                  await api<{ ok: true }>("/api/notifications/web-push/test", {
                    method: "POST",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({}),
                  });

                  showWpTestStatus("已发送（如权限/订阅正常，应很快弹出通知）");
                  setFieldError("wpTestAction", null);
                } catch (e) {
                  clearWpTestStatus();
                  setFieldError("wpTestAction", e instanceof Error ? e.message : String(e));
                } finally {
                  setWpTestPending(false);
                }
              }}
            >
              {wpTestPending ? "测试中…" : "测试 Web Push"}
            </button>
            {renderActionFeedback(
              "wpTestAction",
              wpTestStatus,
              clearWpTestStatus,
              "settings-feedback-wp-test",
              true,
              wpTestButtonRef,
            )}
          </div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">懒猫云账号</div>
        <div className="panel-subtitle">
          显式连接/断开，不走自动保存；成功后服务端会自动续会话并同步机器缓存。
        </div>

        <div className="settings-grid" style={{ marginTop: "16px" }}>
          <div>连接状态</div>
          <div>{renderLazycatAccountBadge(lazycatAccount)}</div>
          <div className="hint">{`机器数：${lazycatAccount.machineCount}`}</div>

          <div>最近主站同步</div>
          <div className="mono">
            {lazycatAccount.lastSiteSyncAt ? formatLocalTime(lazycatAccount.lastSiteSyncAt) : "-"}
          </div>
          <div className="hint">机器列表、到期时间、续费价格等核心字段来源</div>

          <div>最近面板同步</div>
          <div className="mono">
            {lazycatAccount.lastPanelSyncAt ? formatLocalTime(lazycatAccount.lastPanelSyncAt) : "-"}
          </div>
          <div className="hint">流量与端口映射来源；失败时保留 last-good 缓存</div>
        </div>

        <div className="settings-row" style={{ marginTop: "16px" }}>
          <div>邮箱</div>
          <div className="settings-input-wrap">
            <div className="pill">
              <input
                type="email"
                placeholder="user@example.com"
                value={lazycatEmailInput}
                onChange={(e) => {
                  setLazycatEmailInput(e.target.value);
                  setLazycatActionError(null);
                }}
              />
            </div>
          </div>
        </div>

        <div className="settings-row" style={{ marginTop: "16px" }}>
          <div>密码</div>
          <div className="settings-input-wrap">
            <div className="pill">
              <input
                type="password"
                placeholder={lazycatAccount.connected ? "••••••••••••••••" : ""}
                value={lazycatPasswordInput}
                onChange={(e) => {
                  setLazycatPasswordInput(e.target.value);
                  setLazycatActionError(null);
                }}
              />
            </div>
          </div>
        </div>

        {lazycatAccount.lastError ? (
          <div className="error" style={{ marginTop: 12 }}>
            {lazycatAccount.lastError}
          </div>
        ) : null}
        {lazycatActionError ? (
          <div className="error" style={{ marginTop: 12 }}>
            {lazycatActionError}
          </div>
        ) : null}

        <div className="settings-actions">
          <div className="settings-action-wrap">
            <button
              type="button"
              className="pill action-primary lazycat-settings-btn"
              disabled={lazycatLoginPending || lazycatSyncPending || lazycatDisconnectPending}
              onClick={async () => {
                setLazycatLoginPending(true);
                setLazycatActionError(null);
                try {
                  const next = await onLazycatLogin(lazycatEmailInput, lazycatPasswordInput);
                  setLazycatAccount(next);
                  setLazycatPasswordInput("");
                } catch (e) {
                  setLazycatActionError(e instanceof Error ? e.message : String(e));
                } finally {
                  setLazycatLoginPending(false);
                }
              }}
            >
              {lazycatLoginPending ? (
                <>
                  <InlineSpinner />
                  <span>连接中</span>
                </>
              ) : lazycatAccount.connected ? (
                "更新凭据"
              ) : (
                "连接懒猫云"
              )}
            </button>
          </div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className="pill action-primary lazycat-settings-btn"
              disabled={
                !lazycatAccount.connected ||
                lazycatLoginPending ||
                lazycatSyncPending ||
                lazycatDisconnectPending
              }
              onClick={async () => {
                setLazycatSyncPending(true);
                setLazycatActionError(null);
                try {
                  const next = await onLazycatSync();
                  setLazycatAccount(next);
                } catch (e) {
                  setLazycatActionError(e instanceof Error ? e.message : String(e));
                } finally {
                  setLazycatSyncPending(false);
                }
              }}
            >
              {lazycatSyncPending ? (
                <>
                  <InlineSpinner />
                  <span>同步中</span>
                </>
              ) : (
                "立即同步"
              )}
            </button>
          </div>
          <div className="settings-action-wrap">
            <button
              type="button"
              className="pill err lazycat-settings-btn"
              disabled={
                !lazycatAccount.connected ||
                lazycatLoginPending ||
                lazycatSyncPending ||
                lazycatDisconnectPending
              }
              onClick={async () => {
                setLazycatDisconnectPending(true);
                setLazycatActionError(null);
                try {
                  await onLazycatDisconnect();
                  setLazycatAccount(emptyLazycatAccount());
                  setLazycatPasswordInput("");
                } catch (e) {
                  setLazycatActionError(e instanceof Error ? e.message : String(e));
                } finally {
                  setLazycatDisconnectPending(false);
                }
              }}
            >
              {lazycatDisconnectPending ? "断开中…" : "断开账号"}
            </button>
          </div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">关于（About）</div>
        <div className="panel-subtitle">
          版本号来自运行中服务；升级提示基于 GitHub Releases 的 stable latest（可关闭）。
        </div>

        <div className="settings-grid">
          <div>当前版本</div>
          <div className="mono">{aboutVersionDisplay}</div>
          <div className="hint">
            {aboutWebDistBuildId ? `webDist: ${aboutWebDistBuildId}` : "-"}
          </div>

          <div>仓库</div>
          <div className="mono">
            {aboutRepoBaseUrl ? (
              <a href={aboutRepoBaseUrl} target="_blank" rel="noopener noreferrer">
                {aboutRepoBaseUrl}
              </a>
            ) : (
              "-"
            )}
          </div>
          <div className="hint">CATNAP_REPO_URL 可覆盖</div>

          <div>更新检查</div>
          <div>
            <span className={updateBadgeClass}>{updateStatus}</span>
            {updateAvailable ? (
              <span className="pill badge warn" style={{ marginLeft: 8 }}>
                update
              </span>
            ) : null}
          </div>
          <div className="hint">
            {updateCheckedAt ? `checked: ${formatLocalTime(updateCheckedAt)}` : "-"}
          </div>

          <div>最新版本</div>
          <div className="mono">
            {updateLatestVersion ? formatVersionDisplay(updateLatestVersion) : "-"}
          </div>
          <div className="hint">
            {updateLatestUrl ? (
              <a href={updateLatestUrl} target="_blank" rel="noopener noreferrer">
                打开 Release
              </a>
            ) : (
              "-"
            )}
          </div>
        </div>

        {updateMessage ? (
          <div className="muted" style={{ marginTop: 12 }}>
            {updateMessage}
          </div>
        ) : null}

        <div className="settings-actions">
          <button
            type="button"
            className="pill warn center btn"
            disabled={aboutLoading || (about !== null && update?.enabled === false)}
            onClick={async () => {
              try {
                await onCheckUpdate();
              } catch {
                // Ignore; errors surface via aboutError/updateMessage.
              }
            }}
          >
            {aboutLoading ? "检查中…" : "检查更新"}
          </button>
        </div>
      </div>
    </div>
  );
}

type NotificationsViewProps = {
  fetchRecords?: (params: {
    cursor: string | null;
    limit: number;
  }) => Promise<NotificationRecordsResponse>;
  fetchRecord?: (id: string) => Promise<NotificationRecord>;
  targetRecordId?: string | null;
  nowMs?: number;
  onTargetHandled?: () => void;
};

function NotificationSnapshotCard({
  item,
  nowMs,
}: {
  item: NotificationRecordItem;
  nowMs: number;
}) {
  const flagIcon = resolveCountryFlagWatermarkIcon(item.countryName ?? null);
  const capTone =
    item.inventory.status === "unknown" || item.inventory.quantity === 0 ? "warn" : "";
  const capText =
    item.inventory.status === "unknown"
      ? "?"
      : item.inventory.quantity > 10
        ? "10+"
        : String(item.inventory.quantity);
  const rawSpecCells = buildSpecCells(item.specs, SPEC_CARD_MAX_CELLS);
  const specCells: SpecSlotCell[] = SPEC_SLOTS.slice(0, SPEC_CARD_MAX_CELLS).map((key, i) => {
    const c = rawSpecCells[i];
    return c ? { key, ...c } : { key, empty: true };
  });
  const lifecycleLabel = item.lifecycle.state === "delisted" ? "已下架" : "活跃";
  const metaParts = [
    item.partitionLabel?.trim() || null,
    `更新：${formatRelativeTime(item.inventory.checkedAt, nowMs)}`,
    `状态：${lifecycleLabel}`,
  ].filter(Boolean);

  return (
    <article className="notification-item-card">
      {flagIcon ? (
        <span className="card-flag-watermark" aria-hidden="true">
          <Icon className="card-flag-icon" icon={flagIcon} />
        </span>
      ) : null}
      <div className={`mon-cap ${capTone}`}>{capText}</div>
      <div className="card-content">
        <div className="notification-item-title">
          <span className="title-text">{item.name}</span>
          {item.lifecycle.state === "delisted" ? <span className="pill sm err">下架</span> : null}
        </div>
        <div className="notification-item-specs" aria-label="通知快照规格">
          {specCells.map((c) =>
            "empty" in c ? (
              <div className="spec-cell empty" key={c.key}>
                <span className="spec-k"> </span>
                <span className="spec-v"> </span>
              </div>
            ) : (
              <div className="spec-cell" key={c.key}>
                <span className="spec-k">{c.label}</span>
                <span className="spec-v">{c.value}</span>
              </div>
            ),
          )}
        </div>
        <div className="notification-item-price">{formatMoney(item.price)}</div>
        <div className="notification-item-meta">{metaParts.join(" · ")}</div>
      </div>
    </article>
  );
}

export function NotificationsView({
  fetchRecords,
  fetchRecord,
  targetRecordId = null,
  nowMs = Date.now(),
  onTargetHandled,
}: NotificationsViewProps = {}) {
  const limit = 20;
  const [items, setItems] = useState<NotificationRecord[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [initialLoading, setInitialLoading] = useState<boolean>(false);
  const [loadingMore, setLoadingMore] = useState<boolean>(false);
  const [loadingTarget, setLoadingTarget] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [targetError, setTargetError] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const recordRefs = useRef<Map<string, HTMLElement>>(new Map());
  const handledTargetIdRef = useRef<string | null>(null);

  const fetchRecordsImpl = useCallback(
    async (params: { cursor: string | null; limit: number }) => {
      if (fetchRecords) return fetchRecords(params);
      const q = new URLSearchParams();
      if (params.cursor) q.set("cursor", params.cursor);
      q.set("limit", String(params.limit));
      return api<NotificationRecordsResponse>(`/api/notifications/records?${q.toString()}`);
    },
    [fetchRecords],
  );

  const fetchRecordImpl = useCallback(
    async (id: string) => {
      if (fetchRecord) return fetchRecord(id);
      return api<NotificationRecord>(`/api/notifications/records/${encodeURIComponent(id)}`);
    },
    [fetchRecord],
  );

  const loadFirstPage = useCallback(async () => {
    setInitialLoading(true);
    setError(null);
    try {
      const res = await fetchRecordsImpl({ cursor: null, limit });
      setItems((prev) => mergeNotificationRecordLists(prev, res.items));
      setNextCursor(res.nextCursor);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInitialLoading(false);
    }
  }, [fetchRecordsImpl]);

  const loadMore = useCallback(async () => {
    if (!nextCursor || initialLoading || loadingMore) return;
    setLoadingMore(true);
    try {
      const res = await fetchRecordsImpl({ cursor: nextCursor, limit });
      setItems((prev) => mergeNotificationRecordLists(prev, res.items));
      setNextCursor(res.nextCursor);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [fetchRecordsImpl, initialLoading, loadingMore, nextCursor]);

  useEffect(() => {
    handledTargetIdRef.current = null;
    setItems([]);
    setNextCursor(null);
    setTargetError(null);
    setHighlightedId(null);
    void loadFirstPage();
  }, [loadFirstPage]);

  useEffect(() => {
    if (!targetRecordId) {
      handledTargetIdRef.current = null;
      return;
    }
    if (handledTargetIdRef.current === targetRecordId) return;

    const existing = items.find((item) => item.id === targetRecordId);
    if (existing) {
      handledTargetIdRef.current = targetRecordId;
      setTargetError(null);
      setHighlightedId(existing.id);
      onTargetHandled?.();
      return;
    }

    handledTargetIdRef.current = targetRecordId;
    let cancelled = false;
    setLoadingTarget(true);
    setTargetError(null);
    void fetchRecordImpl(targetRecordId)
      .then((record) => {
        if (cancelled) return;
        setItems((prev) => mergeNotificationRecordLists(prev, [record]));
        setHighlightedId(record.id);
        onTargetHandled?.();
      })
      .catch((e) => {
        if (cancelled) return;
        if (e instanceof ApiHttpError && e.status === 404) {
          setTargetError("记录不存在或已过期");
          return;
        }
        handledTargetIdRef.current = null;
        setTargetError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (cancelled) return;
        setLoadingTarget(false);
      });

    return () => {
      cancelled = true;
    };
  }, [fetchRecordImpl, items, onTargetHandled, targetRecordId]);

  useEffect(() => {
    if (!highlightedId) return;
    const el = recordRefs.current.get(highlightedId);
    if (!el) return;
    const raf = window.requestAnimationFrame(() => {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    });
    const timer = window.setTimeout(() => {
      setHighlightedId((current) => (current === highlightedId ? null : current));
    }, 4_000);
    return () => {
      window.cancelAnimationFrame(raf);
      window.clearTimeout(timer);
    };
  }, [highlightedId]);

  useEffect(() => {
    const node = sentinelRef.current;
    if (!node || !nextCursor) return;
    if (typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          void loadMore();
        }
      },
      { rootMargin: "280px 0px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [loadMore, nextCursor]);

  return (
    <div className="panel" data-testid="page-notifications">
      <div className="panel-section">
        <div className="panel-title-row">
          <div className="panel-title">通知记录</div>
          <div className="notifications-meta-row">
            <span className="pill sm">{`${items.length} 组`}</span>
            {loadingTarget ? <span className="pill sm warn">定位中…</span> : null}
          </div>
        </div>
        <div className="panel-subtitle">
          每条通知都是一个组；组里展示该通知生成时关联到的机子快照，不随当前目录变化漂移。
        </div>
        {error ? <p className="error">{error}</p> : null}
        {targetError ? <p className="error">{targetError}</p> : null}
      </div>

      <div className="panel-section notifications-panel-section">
        {initialLoading && items.length === 0 ? <p className="muted">加载通知记录中…</p> : null}
        {!initialLoading && items.length === 0 && !error ? (
          <div className="notifications-empty">还没有通知记录。</div>
        ) : null}

        <div className="notifications-stack">
          {items.map((record) => (
            <section
              className={`notification-group${highlightedId === record.id ? " is-highlighted" : ""}`}
              key={record.id}
              ref={(node) => {
                if (node) {
                  recordRefs.current.set(record.id, node);
                } else {
                  recordRefs.current.delete(record.id);
                }
              }}
              data-record-id={record.id}
            >
              <header className="notification-group-head">
                <div className="notification-group-main">
                  <div className="notification-group-topline">
                    <span className="pill sm badge on">{notificationKindLabel(record.kind)}</span>
                    <span className="notification-group-time" title={record.createdAt}>
                      {formatAbsoluteTime(record.createdAt)}
                    </span>
                  </div>
                  <div className="notification-group-title">{record.title}</div>
                  <div className="notification-group-summary">{record.summary}</div>
                  {record.partitionLabel ? (
                    <div className="notification-group-partition">{record.partitionLabel}</div>
                  ) : null}
                </div>

                <div className="notification-group-statuses">
                  <div className="notification-group-status-block">
                    <span className="muted">Telegram</span>
                    <span className={notificationStatusClass(record.telegramStatus)}>
                      {notificationStatusLabel(record.telegramStatus)}
                    </span>
                  </div>
                  <div className="notification-group-status-block">
                    <span className="muted">Web Push</span>
                    <span className={notificationStatusClass(record.webPushStatus)}>
                      {notificationStatusLabel(record.webPushStatus)}
                    </span>
                  </div>
                </div>
              </header>

              {record.telegramDeliveries && record.telegramDeliveries.length > 0 ? (
                <div className="notification-delivery-list">
                  {record.telegramDeliveries.map((delivery) => (
                    <div
                      className="notification-delivery-row"
                      key={`${record.id}:${delivery.target}:${delivery.status}`}
                    >
                      <span className="notification-delivery-target">{delivery.target}</span>
                      <span className={notificationStatusClass(delivery.status)}>
                        {notificationStatusLabel(delivery.status)}
                      </span>
                      {delivery.error ? (
                        <span className="notification-delivery-error">{delivery.error}</span>
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : null}

              <div className="notification-group-items">
                {record.items.map((item, index) => (
                  <NotificationSnapshotCard
                    item={item}
                    key={`${record.id}:${item.configId ?? index}`}
                    nowMs={nowMs}
                  />
                ))}
              </div>
            </section>
          ))}
        </div>

        <div className="notifications-tail" ref={sentinelRef} aria-hidden="true" />
        {loadingMore ? <p className="muted">正在加载更多通知…</p> : null}
        {!nextCursor && items.length > 0 ? <p className="muted">已经到底啦。</p> : null}
      </div>
    </div>
  );
}

export type LogsViewProps = {
  fetchLogs?: (params: {
    level: string;
    cursor: string | null;
    limit: number;
  }) => Promise<LogsResponse>;
};

export function LogsView({ fetchLogs }: LogsViewProps = {}) {
  const [level, setLevel] = useState<string>("info");
  const [keyword, setKeyword] = useState<string>("");
  const [limit, setLimit] = useState<number>(50);
  const [items, setItems] = useState<LogsResponse["items"]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const fetchLogsImpl = useCallback(
    async (params: { level: string; cursor: string | null; limit: number }) => {
      if (fetchLogs) return fetchLogs(params);
      const q = new URLSearchParams();
      q.set("level", params.level);
      if (params.cursor) q.set("cursor", params.cursor);
      q.set("limit", String(params.limit));
      return api<LogsResponse>(`/api/logs?${q.toString()}`);
    },
    [fetchLogs],
  );

  function formatClockTime(iso: string): string {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return iso;
    return new Intl.DateTimeFormat(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(new Date(t));
  }

  const filteredItems = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    if (!q) return items;
    return items.filter((it) => {
      if (it.scope.toLowerCase().includes(q)) return true;
      return it.message.toLowerCase().includes(q);
    });
  }, [items, keyword]);

  function levelBadgeClass(lvl: LogsResponse["items"][number]["level"]): string {
    if (lvl === "info") return "pill sm center logs-badge on";
    if (lvl === "warn") return "pill sm center logs-badge warn";
    if (lvl === "error") return "pill sm center logs-badge err";
    return "pill sm center logs-badge";
  }

  async function load(cursor: string | null) {
    setLoading(true);
    setError(null);
    try {
      const res = await fetchLogsImpl({ level, cursor, limit });
      setItems(res.items);
      setNextCursor(res.nextCursor);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    async function run() {
      setLoading(true);
      setError(null);
      try {
        const res = await fetchLogsImpl({ level, cursor: null, limit });
        if (!cancelled) {
          setItems(res.items);
          setNextCursor(res.nextCursor);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void run();
    return () => {
      cancelled = true;
    };
  }, [fetchLogsImpl, level, limit]);

  return (
    <div className="panel" data-testid="page-logs">
      <div className="panel-section">
        <div className="panel-title">过滤</div>
        <div className="logs-controls">
          <div className="pill select w-168">
            <span className="pill-prefix">Level：</span>
            <select value={level} onChange={(e) => setLevel(e.target.value)}>
              <option value="debug">debug</option>
              <option value="info">info</option>
              <option value="warn">warn</option>
              <option value="error">error</option>
            </select>
          </div>

          <div className="pill search w-520">
            <span className="pill-prefix">关键字：</span>
            <input
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              placeholder="scope/message…"
            />
          </div>

          <div className="pill w-144 num">
            <span className="pill-prefix">limit：</span>
            <input
              type="number"
              min={1}
              max={200}
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
            />
          </div>

          <button
            type="button"
            className="pill warn center w-128"
            disabled={loading}
            onClick={() => void load(null)}
          >
            刷新
          </button>
        </div>
        {error ? <p className="error">{error}</p> : null}
      </div>

      <div className="panel-section">
        <div className="panel-title">列表</div>
        <div className="divider-bleed" />

        <div className="logs-grid-head">
          <div className="muted">时间</div>
          <div className="muted">级别</div>
          <div className="muted">范围</div>
          <div className="muted">消息</div>
        </div>
        <div className="divider-bleed" style={{ marginTop: 0 }} />

        <div className="logs-rows">
          {filteredItems.map((it) => (
            <div className="logs-row" key={it.id}>
              <div className="muted">{formatClockTime(it.ts)}</div>
              <div>
                <span className={levelBadgeClass(it.level)}>{it.level.toUpperCase()}</span>
              </div>
              <div className="mono">{it.scope}</div>
              <div>{it.message}</div>
            </div>
          ))}
        </div>

        <div className="logs-footer">
          <div className="pill cursor">
            <input readOnly value={`cursor: nextCursor=${nextCursor ?? "null"}（null 表示到底）`} />
          </div>
          <button
            type="button"
            className="pill next center"
            disabled={loading || !nextCursor}
            onClick={() => void load(nextCursor)}
          >
            下一页
          </button>
        </div>
      </div>
    </div>
  );
}

type OpsViewProps = {
  fetchState?: (range: OpsRange) => Promise<OpsStateResponse>;
  createEventSource?: (url: string) => EventSource;
  range?: OpsRange;
  onRangeChange?: (range: OpsRange) => void;
  follow?: boolean;
  onFollowChange?: (follow: boolean) => void;
  helpOpen?: boolean;
  onHelpOpenChange?: (open: boolean) => void;
  onSseUiChange?: (next: OpsSseUi) => void;
};

async function fetchOpsState(range: OpsRange): Promise<OpsStateResponse> {
  return api<OpsStateResponse>(`/api/ops/state?range=${encodeURIComponent(range)}`);
}

function defaultCreateEventSource(url: string): EventSource {
  return new EventSource(url);
}

function opsRangeLabel(range: OpsRange): string {
  if (range === "24h") return "24小时";
  if (range === "7d") return "7天";
  return "30天";
}

function OpsRangePill({ range, onChange }: { range: OpsRange; onChange: (r: OpsRange) => void }) {
  return (
    <div className="pill select ops-range-pill" style={{ width: 168 }}>
      <span className="pill-prefix">口径：</span>
      <select value={range} onChange={(e) => onChange(e.target.value as OpsRange)}>
        <option value="24h">24小时</option>
        <option value="7d">7天</option>
        <option value="30d">30天</option>
      </select>
    </div>
  );
}

function OpsSseIndicator({ sse }: { sse: OpsSseUi }) {
  const anchorRef = useRef<HTMLButtonElement | null>(null);
  const [tooltipOpen, setTooltipOpen] = useState(false);
  const dotClass =
    sse.status === "connected"
      ? "ops-dot ok"
      : sse.status === "reset"
        ? "ops-dot err"
        : "ops-dot warn";
  const statusText =
    sse.status === "connected"
      ? "状态：已连接"
      : sse.status === "reset"
        ? "状态：已重置"
        : "状态：重连中";
  const resetText = sse.lastReset
    ? `${sse.lastReset.reason}${sse.lastReset.details ? ` (${sse.lastReset.details})` : ""}`
    : "无";

  return (
    <button
      aria-label="查看 SSE 连接状态"
      className="ops-sse"
      type="button"
      onBlur={(event) => {
        const nextTarget = event.relatedTarget;
        if (!(nextTarget instanceof Node) || !event.currentTarget.contains(nextTarget)) {
          setTooltipOpen(false);
        }
      }}
      onFocus={() => setTooltipOpen(true)}
      onMouseEnter={() => setTooltipOpen(true)}
      onMouseLeave={() => setTooltipOpen(false)}
      ref={anchorRef}
    >
      <span className="ops-dot-ring" aria-hidden="true">
        <span className={dotClass} />
      </span>
      <span className="ops-sse-label">SSE</span>
      <SettingsFeedbackBubble
        anchorRef={anchorRef}
        dismissible={false}
        inline
        message={null}
        open={tooltipOpen}
        placement="bottom-end"
        role="tooltip"
        showIcon={false}
        tone="neutral"
      >
        <div className="settings-feedback-title">SSE 连接状态</div>
        <div className="settings-feedback-row">
          <span className="ops-dot-ring sm" aria-hidden="true">
            <span className={dotClass} />
          </span>
          <span className="settings-feedback-key">{statusText}</span>
        </div>
        <div className="settings-feedback-line">{`回放窗口：${sse.replayWindowSeconds ? `${Math.round(sse.replayWindowSeconds / 60)}分钟` : "—"}`}</div>
        <div className="settings-feedback-line">
          Last-Event-ID：<span className="mono">{sse.lastEventId ?? "—"}</span>
        </div>
        <div className="settings-feedback-line">{`最近 reset：${resetText}`}</div>
      </SettingsFeedbackBubble>
    </button>
  );
}

function formatCompactCount(n: number): string {
  if (!Number.isFinite(n)) return "—";
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\\.0$/, "")}m`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\\.0$/, "")}k`;
  return String(Math.round(n));
}

function sparkPath(values: number[], width: number, height: number): string | null {
  if (!values.length) return null;
  const n = values.length;
  if (n === 1) return `M0 ${height / 2} L${width} ${height / 2}`;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const pts = values.map((v, i) => {
    const x = (i / (n - 1)) * width;
    const y = height - ((v - min) / span) * height;
    return { x, y };
  });
  return pts.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(2)} ${p.y.toFixed(2)}`).join(" ");
}

function Sparkline({
  values,
  stroke,
}: {
  values: number[];
  stroke: string;
}) {
  const w = 220;
  const h = 28;
  const d = sparkPath(values, w, h);
  return (
    <svg className="ops-spark" viewBox={`0 0 ${w} ${h}`} aria-hidden="true">
      {d ? (
        <>
          <path className="ops-spark-fade" d={d} stroke={stroke} />
          <path className="ops-spark-line" d={d} stroke={stroke} />
        </>
      ) : (
        <line x1="0" y1={h / 2} x2={w} y2={h / 2} stroke="var(--trend-empty)" strokeWidth="2" />
      )}
    </svg>
  );
}

export function OpsView({
  fetchState = fetchOpsState,
  createEventSource = defaultCreateEventSource,
  range: rangeProp,
  onRangeChange,
  follow: followProp,
  onFollowChange,
  helpOpen = false,
  onHelpOpenChange,
  onSseUiChange,
}: OpsViewProps = {}) {
  const [rangeInternal, setRangeInternal] = useState<OpsRange>("24h");
  const range = rangeProp ?? rangeInternal;
  const setRange = onRangeChange ?? setRangeInternal;

  const [followInternal, setFollowInternal] = useState<boolean>(true);
  const follow = followProp ?? followInternal;
  const setFollow = onFollowChange ?? setFollowInternal;

  const [snap, setSnap] = useState<OpsStateResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [err, setErr] = useState<string | null>(null);
  const [sseEpoch, setSseEpoch] = useState<number>(0);
  const [search, setSearch] = useState<string>("");
  const [sseUi, setSseUi] = useState<OpsSseUi>({
    status: "reconnecting",
    replayWindowSeconds: null,
    lastEventId: null,
    lastReset: null,
  });
  const [toast, setToast] = useState<{ text: string; tone: "warn" | "err" | "ok" } | null>(null);
  const logRef = useRef<HTMLDivElement | null>(null);

  const formatClock = useCallback((iso: string) => {
    const ts = Date.parse(iso);
    if (!Number.isFinite(ts)) return iso;
    return new Date(ts).toLocaleTimeString("zh-CN", { hour12: false });
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const next = await fetchState(range);
      setSnap(next);
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  }, [fetchState, range]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    onSseUiChange?.(sseUi);
  }, [onSseUiChange, sseUi]);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 6_000);
    return () => window.clearTimeout(id);
  }, [toast]);

  useEffect(() => {
    const url = `/api/ops/stream?range=${encodeURIComponent(range)}&epoch=${encodeURIComponent(
      String(sseEpoch),
    )}`;
    const es = createEventSource(url);
    let closed = false;

    setSseUi((prev) => ({ ...prev, status: "reconnecting" }));

    const noteEventId = (ev: MessageEvent) => {
      const eventId = Number(ev.lastEventId || 0);
      if (!Number.isFinite(eventId) || eventId <= 0) return;
      setSseUi((prev) => ({
        ...prev,
        lastEventId: prev.lastEventId ? Math.max(prev.lastEventId, eventId) : eventId,
      }));
    };

    es.onopen = () => {
      if (!closed) setSseUi((prev) => ({ ...prev, status: "connected" }));
    };
    es.onerror = () => {
      if (!closed)
        setSseUi((prev) => (prev.status === "reset" ? prev : { ...prev, status: "reconnecting" }));
    };

    const onHello = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const data = JSON.parse(ev.data) as { replayWindowSeconds?: number };
        const replayWindowSeconds = data.replayWindowSeconds;
        if (typeof replayWindowSeconds === "number") {
          setSseUi((prev) => ({ ...prev, replayWindowSeconds }));
        }
      } catch {
        // ignore
      }
    };

    const onMetrics = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const data = JSON.parse(ev.data) as { stats?: OpsStateResponse["stats"] };
        const stats = data.stats;
        if (!stats) return;
        setSnap((prev) => (prev ? { ...prev, stats } : prev));
      } catch {
        // ignore
      }
    };

    const onQueue = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const data = JSON.parse(ev.data) as { queue?: OpsStateResponse["queue"] };
        const queue = data.queue;
        if (!queue) return;
        setSnap((prev) => (prev ? { ...prev, queue } : prev));
      } catch {
        // ignore
      }
    };

    const onWorkers = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const data = JSON.parse(ev.data) as { workers?: OpsStateResponse["workers"] };
        const workers = data.workers;
        if (!workers) return;
        setSnap((prev) => (prev ? { ...prev, workers } : prev));
      } catch {
        // ignore
      }
    };

    const onTask = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const data = JSON.parse(ev.data) as {
          phase?: "enqueued" | "started" | "finished";
          key?: { fid: string; gid: string | null };
          reasonCounts?: Record<string, number> | null;
          run?: { runId: number; endedAt?: string | null; ok?: boolean | null } | null;
        };
        const phase = data.phase;
        const key = data.key;
        if (!phase || !key) return;
        const keyStr = `${key.fid}:${key.gid ?? ""}`;
        setSnap((prev) => {
          if (!prev) return prev;
          const byKey = new Map<string, OpsStateResponse["tasks"][number]>();
          for (const t of prev.tasks) byKey.set(`${t.key.fid}:${t.key.gid ?? ""}`, t);
          const existing = byKey.get(keyStr);

          if (phase === "finished") {
            byKey.delete(keyStr);
            return { ...prev, tasks: Array.from(byKey.values()) };
          }

          const next: OpsStateResponse["tasks"][number] = {
            key: { fid: key.fid, gid: key.gid },
            state: phase === "started" ? "running" : (existing?.state ?? "pending"),
            enqueuedAt: existing?.enqueuedAt ?? prev.serverTime,
            reasonCounts: (data.reasonCounts ?? existing?.reasonCounts ?? {}) as Record<
              string,
              number
            >,
            lastRun: existing?.lastRun ?? null,
          };
          byKey.set(keyStr, next);
          return { ...prev, tasks: Array.from(byKey.values()) };
        });
      } catch {
        // ignore
      }
    };

    const onLog = (ev: MessageEvent) => {
      noteEventId(ev);
      const eventId = Number(ev.lastEventId || 0);
      try {
        const data = JSON.parse(ev.data) as {
          ts: string;
          level: "debug" | "info" | "warn" | "error";
          scope: string;
          message: string;
          meta?: unknown;
        };
        setSnap((prev) => {
          if (!prev) return prev;
          const next = {
            ...prev,
            logTail: [...prev.logTail, { eventId, ...data }].slice(-500),
          };
          return next;
        });
      } catch {
        // ignore
      }
    };

    const onReset = (ev: MessageEvent) => {
      noteEventId(ev);
      try {
        const payload = JSON.parse(ev.data) as {
          serverTime: string;
          reason: string;
          details?: string | null;
        };
        setSseUi((prev) => ({
          ...prev,
          status: "reset",
          lastReset: {
            serverTime: payload.serverTime,
            reason: payload.reason,
            details: payload.details ?? null,
          },
        }));
        setToast({
          tone: "warn",
          text: `SSE 重置：${payload.reason} → 重新加载 snapshot…`,
        });
      } catch {
        setSseUi((prev) => ({ ...prev, status: "reset" }));
        setToast({ tone: "warn", text: "SSE 重置：重新加载 snapshot…" });
      }
      es.close();
      void refresh();
      setSseEpoch((v) => v + 1);
    };

    es.addEventListener("ops.hello", onHello as EventListener);
    es.addEventListener("ops.metrics", onMetrics as EventListener);
    es.addEventListener("ops.queue", onQueue as EventListener);
    es.addEventListener("ops.worker", onWorkers as EventListener);
    es.addEventListener("ops.task", onTask as EventListener);
    es.addEventListener("ops.log", onLog as EventListener);
    es.addEventListener("ops.reset", onReset as EventListener);

    return () => {
      closed = true;
      es.close();
    };
  }, [range, refresh, sseEpoch, createEventSource]);

  const setFollowNext = useCallback(
    (next: boolean) => {
      setFollow(next);
    },
    [setFollow],
  );

  const logLen = snap?.logTail.length ?? 0;
  useEffect(() => {
    if (!follow) return;
    if (!logLen) return;
    const el = logRef.current;
    if (!el) return;
    requestAnimationFrame(() => {
      el.scrollTop = el.scrollHeight;
    });
  }, [follow, logLen]);

  const filteredLogTail = useMemo(() => {
    const items = snap?.logTail ?? [];
    const q = search.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (it) => it.scope.toLowerCase().includes(q) || it.message.toLowerCase().includes(q),
    );
  }, [search, snap?.logTail]);

  const recentNotifyFailure = useMemo(() => {
    const items = snap?.logTail ?? [];
    for (let i = items.length - 1; i >= 0; i -= 1) {
      const it = items[i];
      if ((it.level === "warn" || it.level === "error") && it.scope.startsWith("notify.")) {
        return it.message;
      }
    }
    return null;
  }, [snap?.logTail]);

  const workerConcurrency = snap?.workers.length ?? 0;
  const rangeText = opsRangeLabel(range);

  return (
    <div className="panel ops-panel" data-testid="page-ops">
      <div className="panel-section ops-surface">
        {err ? <p className="error">{err}</p> : null}

        {!snap ? (
          <p className="muted">Loading…</p>
        ) : (
          <div className="ops-layout">
            <div className="ops-kpi-grid">
              <div
                className="ops-kpi-card"
                style={{ "--ops-accent": "var(--ops-green)" } as CSSProperties}
              >
                <div className="ops-kpi-head">
                  <span className="ops-dot-ring" aria-hidden="true">
                    <span className="ops-dot ok" />
                  </span>
                  <span className="ops-kpi-label">队列</span>
                </div>
                <div className="ops-kpi-value-row">
                  <span className="ops-kpi-value">{formatCompactCount(snap.queue.pending)}</span>
                  <span className="ops-kpi-unit">待处理</span>
                </div>
                <div className="ops-kpi-sub">{`运行中：${snap.queue.running} • 合并：${snap.queue.deduped}`}</div>
                <div className="ops-kpi-meta">{`最老等待：${snap.queue.oldestWaitSeconds ?? 0}s • 更新：${formatClock(snap.serverTime)}${loading ? "（刷新中）" : ""}`}</div>
                <Sparkline values={snap.sparks.volume} stroke="var(--ops-green)" />
              </div>

              <div
                className="ops-kpi-card"
                style={{ "--ops-accent": "var(--ops-blue)" } as CSSProperties}
              >
                <div className="ops-kpi-head">
                  <span className="ops-dot-ring" aria-hidden="true">
                    <span className="ops-dot blue" />
                  </span>
                  <span className="ops-kpi-label">采集成功率</span>
                </div>
                <div className="ops-kpi-value-row">
                  <span className="ops-kpi-value">{`${snap.stats.collection.successRatePct.toFixed(1)}%`}</span>
                </div>
                <div className="ops-kpi-sub">{`成功：${snap.stats.collection.success} • 失败：${snap.stats.collection.failure}`}</div>
                <div className="ops-kpi-meta">{`cache hit：${snap.stats.collection.cacheHits} • 口径：${rangeText}`}</div>
                <Sparkline values={snap.sparks.collectionSuccessRatePct} stroke="var(--ops-blue)" />
              </div>

              <div
                className="ops-kpi-card"
                style={{ "--ops-accent": "var(--ops-purple)" } as CSSProperties}
              >
                <div className="ops-kpi-head">
                  <span className="ops-dot-ring" aria-hidden="true">
                    <span className="ops-dot purple" />
                  </span>
                  <span className="ops-kpi-label">通知成功率</span>
                </div>
                <div className="ops-kpi-value-row">
                  <span className="ops-kpi-value">
                    {snap.stats.notify.telegram?.total
                      ? `${snap.stats.notify.telegram.successRatePct.toFixed(1)}%`
                      : "—"}
                  </span>
                  <span className="ops-kpi-unit">Telegram</span>
                </div>
                <div className="ops-kpi-sub">
                  {`Web Push：${
                    snap.stats.notify.webPush?.total
                      ? `${snap.stats.notify.webPush.successRatePct.toFixed(1)}%`
                      : "—"
                  }`}
                </div>
                <div className="ops-kpi-meta">{`最近失败：${recentNotifyFailure ?? "—"}`}</div>
                <Sparkline
                  values={snap.sparks.notifyTelegramSuccessRatePct}
                  stroke="var(--ops-purple)"
                />
              </div>

              <div
                className="ops-kpi-card"
                style={{ "--ops-accent": "var(--ops-orange)" } as CSSProperties}
              >
                <div className="ops-kpi-head">
                  <span className="ops-dot-ring" aria-hidden="true">
                    <span className="ops-dot orange" />
                  </span>
                  <span className="ops-kpi-label">目录拓扑</span>
                </div>
                <div className="ops-kpi-value-row">
                  <span className="ops-kpi-value">{snap.topology.status || "idle"}</span>
                  <span className="ops-kpi-unit">状态</span>
                </div>
                <div className="ops-kpi-sub">{`请求：${snap.topology.requestCount} • 最近：${snap.topology.refreshedAt ? formatClock(snap.topology.refreshedAt) : "—"}`}</div>
                <div className="ops-kpi-meta">{snap.topology.message ?? `口径：${rangeText}`}</div>
                <Sparkline values={snap.sparks.volume} stroke="var(--ops-orange)" />
              </div>
            </div>

            <div className="ops-block-grid">
              <section className="ops-block">
                <div className="ops-block-head">
                  <div className="ops-block-title">{`工作者（并发=${workerConcurrency}）`}</div>
                </div>
                <div className="ops-block-divider" />
                <div className="ops-workers">
                  {snap.workers.map((w) => {
                    const idx = Number(w.workerId.replace(/^w/, ""));
                    const name = Number.isFinite(idx) ? `工作者-${idx}` : w.workerId;
                    const startedAtMs = w.startedAt ? Date.parse(w.startedAt) : Number.NaN;
                    const nowMs = Date.now();
                    const elapsedMs = Number.isFinite(startedAtMs)
                      ? Math.max(0, nowMs - startedAtMs)
                      : 0;
                    const elapsedText =
                      w.state === "running" && elapsedMs
                        ? `耗时：${(elapsedMs / 1000).toFixed(1)}s`
                        : null;

                    const dotClass =
                      w.state === "running"
                        ? "ops-dot ok"
                        : w.state === "error"
                          ? "ops-dot err"
                          : "ops-dot idle";

                    return (
                      <div className="ops-worker" key={w.workerId}>
                        <div className="ops-worker-line1">
                          <span className="ops-dot-ring" aria-hidden="true">
                            <span className={dotClass} />
                          </span>
                          <span className="muted">{name}</span>
                        </div>
                        <div className="ops-worker-line2">
                          <span className="muted">当前：</span>
                          {w.task ? (
                            <span className="mono">{`key=(fid=${w.task.fid}, gid=${w.task.gid ?? "-"})`}</span>
                          ) : (
                            <span className="muted">-</span>
                          )}
                          <span className="muted ops-worker-spacer" />
                          {elapsedText ? <span className="muted">{elapsedText}</span> : null}
                          {w.state !== "running" ? (
                            <span className="muted">{`最近错误：${w.lastError?.message ?? "-"}`}</span>
                          ) : null}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>

              <section className="ops-block">
                <div className="ops-block-head">
                  <div className="ops-block-title">队列任务</div>
                  <div className="ops-block-subtitle muted">{`discovery=${snap.queue.reasonCounts.discovery_due ?? 0} • poller=${snap.queue.reasonCounts.poller_due ?? 0} • manual=${snap.queue.reasonCounts.manual_refresh ?? 0}`}</div>
                </div>
                <div className="ops-block-divider" />
                <div className="ops-tasks">
                  <div className="ops-tasks-head muted">
                    <div>状态</div>
                    <div>键</div>
                    <div>原因</div>
                    <div className="ops-tasks-right">最近结果</div>
                  </div>
                  <div className="ops-block-divider thin" />
                  {snap.tasks.length ? (
                    snap.tasks.map((t) => {
                      const dotClass = t.state === "running" ? "ops-dot ok" : "ops-dot pend";
                      const reasons = Object.entries(t.reasonCounts)
                        .map(([k, v]) => {
                          const short =
                            k === "manual_refresh" || k === "manual_ops"
                              ? "manual"
                              : k === "poller_due"
                                ? "poller_due"
                                : k;
                          return `${short}=${v}`;
                        })
                        .join(", ");
                      const lastText = t.lastRun
                        ? `${t.lastRun.ok ? "成功" : "失败"} ${formatClock(t.lastRun.endedAt)}`
                        : "—";
                      return (
                        <div className="ops-task" key={`${t.key.fid}:${t.key.gid ?? ""}`}>
                          <div className="ops-task-state">
                            <span className="ops-dot-ring" aria-hidden="true">
                              <span className={dotClass} />
                            </span>
                          </div>
                          <div className="mono">{`${t.key.fid} / ${t.key.gid ?? "-"}`}</div>
                          <div className="mono" title={reasons || "—"}>
                            {reasons || "—"}
                          </div>
                          <div className="muted ops-tasks-right">{lastText}</div>
                        </div>
                      );
                    })
                  ) : (
                    <div className="muted ops-empty">当前无 pending/running 任务</div>
                  )}
                </div>
              </section>
            </div>

            <section className="ops-block ops-log">
              <div className="ops-block-head ops-log-headbar">
                <div className="ops-block-title">{`实时日志（N=${filteredLogTail.length}）`}</div>
                <button
                  type="button"
                  className={`pill sm ${follow ? "on" : ""}`}
                  onClick={() => setFollowNext(!follow)}
                >
                  {follow ? "跟随：开" : "跟随：关"}
                </button>
                <div className="pill search sm ops-log-search" style={{ width: 240 }}>
                  <span className="pill-prefix">搜索：</span>
                  <input
                    value={search}
                    placeholder="关键字…"
                    onChange={(e) => setSearch(e.target.value)}
                  />
                </div>
                <button
                  type="button"
                  className="pill warn sm"
                  onClick={() => {
                    setSearch("");
                    setSnap((prev) => (prev ? { ...prev, logTail: [] } : prev));
                    setFollowNext(true);
                  }}
                >
                  清空
                </button>
              </div>
              <div className="ops-block-divider" />

              {toast ? (
                <div className={`ops-toast ${toast.tone}`}>
                  <span className="mono">{toast.text}</span>
                </div>
              ) : null}

              {!follow ? (
                <div className="ops-follow-paused">
                  <span className="muted">已暂停跟随（你已上滚）</span>
                  <div className="ops-follow-actions">
                    <button
                      type="button"
                      className="pill sm"
                      onClick={() => {
                        const el = logRef.current;
                        if (el) el.scrollTop = el.scrollHeight;
                      }}
                    >
                      跳到底部
                    </button>
                    <button
                      type="button"
                      className="pill sm on"
                      onClick={() => setFollowNext(true)}
                    >
                      恢复跟随
                    </button>
                  </div>
                </div>
              ) : null}

              <div
                className="ops-logbox"
                ref={logRef}
                onScroll={(e) => {
                  const el = e.currentTarget;
                  const nearBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 8;
                  setFollowNext(nearBottom);
                }}
              >
                {filteredLogTail.map((it) => (
                  <div className={`ops-log-row lvl-${it.level}`} key={it.eventId}>
                    <div className="mono">{formatClock(it.ts)}</div>
                    <div className="mono">{it.scope}</div>
                    <div className="ops-log-msg">{it.message}</div>
                  </div>
                ))}
              </div>
            </section>
          </div>
        )}

        {helpOpen ? (
          <div
            className="ops-modal-backdrop"
            onMouseDown={(e) => {
              if (e.target === e.currentTarget) onHelpOpenChange?.(false);
            }}
            role="presentation"
          >
            <div className="ops-modal">
              <div className="ops-modal-title">帮助</div>
              <div className="ops-modal-body">
                <div className="muted">- 先拉 snapshot，再用 SSE 实时更新（断线自动重连）。</div>
                <div className="muted">
                  - 若携带的 <span className="mono">Last-Event-ID</span> 过旧/非法，会收到{" "}
                  <span className="mono">ops.reset</span> 并自动重拉 snapshot。
                </div>
                <div className="muted">- “跟随”只影响日志 tail 的自动滚动。</div>
                <div className="muted">- “口径”切换会刷新成功率与推送成功率统计。</div>
              </div>
              <div className="ops-modal-actions">
                <button type="button" className="pill" onClick={() => onHelpOpenChange?.(false)}>
                  关闭
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
