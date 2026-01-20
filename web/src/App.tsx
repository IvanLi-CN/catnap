import { useCallback, useEffect, useMemo, useState } from "react";
import { AppShell } from "./ui/layout/AppShell";
import { ThemeMenu } from "./ui/nav/ThemeMenu";
import "./app.css";

type ApiError = {
  error: { code: string; message: string };
};

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
export type Region = { id: string; countryId: string; name: string; locationName?: string };

export type Spec = { key: string; value: string };
export type Money = { amount: number; currency: string; period: string };
export type Inventory = {
  status: "unknown" | "available" | "unavailable";
  quantity: number;
  checkedAt: string;
};

export type Config = {
  id: string;
  countryId: string;
  regionId: string | null;
  name: string;
  specs: Spec[];
  price: Money;
  inventory: Inventory;
  digest: string;
  monitorSupported: boolean;
  monitorEnabled: boolean;
};

export type SettingsView = {
  poll: { intervalMinutes: number; jitterPct: number };
  siteBaseUrl: string | null;
  notifications: {
    telegram: { enabled: boolean; configured: boolean; target?: string };
    webPush: { enabled: boolean; vapidPublicKey?: string };
  };
};

export type BootstrapResponse = {
  user: UserView;
  catalog: {
    countries: Country[];
    regions: Region[];
    configs: Config[];
    fetchedAt: string;
    source: { url: string };
  };
  monitoring: { enabledConfigIds: string[]; poll: { intervalSeconds: number; jitterPct: number } };
  settings: SettingsView;
};

export type ProductsResponse = {
  configs: Config[];
  fetchedAt: string;
};

export type RefreshStatusResponse = {
  state: "idle" | "syncing" | "success" | "error";
  done: number;
  total: number;
  message?: string;
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

export type Route = "monitoring" | "products" | "settings" | "logs";

function getRoute(): Route {
  const raw = window.location.hash.replace(/^#/, "");
  if (raw === "products" || raw === "settings" || raw === "logs") return raw;
  return "monitoring";
}

function routeTitle(route: Route): string {
  if (route === "products") return "全部产品";
  if (route === "settings") return "系统设置";
  if (route === "logs") return "日志";
  return "库存监控";
}

function routeSubtitle(route: Route): string {
  if (route === "products") return "分组：国家地区 → 可用区域 → 配置 • 点击切换监控（用户隔离）";
  if (route === "settings") return "按用户隔离 • 保存后立即生效（下次轮询使用新频率 + 抖动）";
  if (route === "logs") return "按用户隔离 • 支持过滤与分页（cursor）";
  return "按国家地区 / 可用区分组展示；支持折叠，默认展开（折叠状态可记忆）";
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { cache: "no-store", ...init });
  const text = await res.text();
  const tryJson = () => {
    if (!text) return null;
    try {
      return JSON.parse(text) as unknown;
    } catch {
      return null;
    }
  };

  if (!res.ok) {
    const body = tryJson() as ApiError | null;
    const msg = body?.error?.message ?? `HTTP ${res.status}`;
    throw new ApiHttpError(res.status, res.statusText, msg);
  }

  const body = tryJson() as T | null;
  if (body === null) throw new Error("Invalid JSON response");
  return body;
}

function formatMoney(m: Money): string {
  if (m.currency === "CNY") {
    const period = m.period === "month" ? "月" : m.period;
    return `¥${m.amount.toFixed(2)} / ${period}`;
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
  const [bootstrap, setBootstrap] = useState<BootstrapResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [syncState, setSyncState] = useState<RefreshStatusResponse["state"]>("idle");
  const [syncDone, setSyncDone] = useState<number>(0);
  const [syncTotal, setSyncTotal] = useState<number>(0);
  const [syncAlert, setSyncAlert] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());

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
    const onHash = () => setRoute(getRoute());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 10_000);
    return () => window.clearInterval(id);
  }, []);

  const refreshBootstrapSilently = useCallback(async () => {
    try {
      const json = await api<BootstrapResponse>("/api/bootstrap");
      setBootstrap(json);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

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

  async function reloadBootstrap() {
    setLoading(true);
    setError(null);
    try {
      const json = await api<BootstrapResponse>("/api/bootstrap");
      setBootstrap(json);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function startSync() {
    if (syncState === "syncing") return;

    setSyncAlert(null);
    setSyncDone(0);
    setSyncTotal(0);
    setSyncState("syncing");

    try {
      const st = await api<RefreshStatusResponse>("/api/refresh", { method: "POST" });
      setSyncDone(st.done);
      setSyncTotal(st.total);
      setSyncState(st.state);
      if (st.state === "error") {
        setSyncAlert(st.message ?? "同步失败");
      }
    } catch (e) {
      setSyncState("error");
      if (e instanceof ApiHttpError && e.status === 405) {
        setSyncAlert("后端未启用“重新同步”接口（/api/refresh）。请重启后端到最新版本后再试。");
      } else {
        setSyncAlert(e instanceof Error ? e.message : String(e));
      }
    }
  }

  useEffect(() => {
    if (syncState !== "syncing") return;

    let cancelled = false;
    async function poll() {
      try {
        const st = await api<RefreshStatusResponse>("/api/refresh/status");
        if (cancelled) return;
        if (st.state === "idle") return;

        setSyncDone(st.done);
        setSyncTotal(st.total);
        setSyncState(st.state);

        if (st.state === "success") {
          await refreshBootstrapSilently();
        } else if (st.state === "error") {
          setSyncAlert(st.message ?? "同步失败");
        }
      } catch {
        // Ignore poll errors.
      }
    }

    const id = window.setInterval(() => void poll(), 800);
    void poll();
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [refreshBootstrapSilently, syncState]);

  useEffect(() => {
    if (syncState !== "success") return;
    const id = window.setTimeout(() => setSyncState("idle"), 2000);
    return () => window.clearTimeout(id);
  }, [syncState]);

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

  async function saveSettings(next: SettingsView & { telegramBotToken?: string | null }) {
    if (!bootstrap) return;
    const updated = await api<SettingsView>("/api/settings", {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        poll: next.poll,
        siteBaseUrl: next.siteBaseUrl,
        notifications: {
          telegram: {
            enabled: next.notifications.telegram.enabled,
            botToken: next.telegramBotToken ?? null,
            target: next.notifications.telegram.target ?? null,
          },
          webPush: { enabled: next.notifications.webPush.enabled },
        },
      }),
    });
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
  }

  const title = `Catnap • ${routeTitle(route)}`;
  const subtitle = route === "monitoring" ? null : routeSubtitle(route);
  const sidebar = (
    <>
      <div className="sidebar-title">导航</div>
      <a className={route === "monitoring" ? "nav-item active" : "nav-item"} href="#monitoring">
        库存监控
      </a>
      <a className={route === "products" ? "nav-item active" : "nav-item"} href="#products">
        全部产品
      </a>
      <a className={route === "settings" ? "nav-item active" : "nav-item"} href="#settings">
        系统设置
      </a>
      <a className={route === "logs" ? "nav-item active" : "nav-item"} href="#logs">
        日志
      </a>
    </>
  );

  const actions = (
    <>
      {route === "products" ? (
        <button type="button" className="pill" disabled={loading} onClick={reloadBootstrap}>
          刷新：手动
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
              disabled={syncState === "syncing"}
              title="强制抓取上游并刷新（30s 限流）"
              onClick={() => void startSync()}
            >
              <span
                className={
                  syncState === "syncing"
                    ? "sync-icon spin"
                    : syncState === "error"
                      ? "sync-icon err"
                      : syncState === "success"
                        ? "sync-icon ok"
                        : "sync-icon"
                }
                aria-hidden="true"
              >
                {syncState === "syncing" ? (
                  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                    <path
                      fill="currentColor"
                      d="M12 4a8 8 0 0 1 7.9 6.7a1 1 0 1 1-2 .3A6 6 0 1 0 18 12a1 1 0 1 1 2 0a8 8 0 1 1-8-8"
                    />
                  </svg>
                ) : syncState === "error" ? (
                  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="M1 21h22L12 2zm12-3h-2v-2h2zm0-4h-2v-4h2z" />
                  </svg>
                ) : syncState === "success" ? (
                  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                    <path
                      fill="currentColor"
                      d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2m-1 14l-4-4l1.4-1.4L11 13.2l5.6-5.6L18 9z"
                    />
                  </svg>
                ) : (
                  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                    <path fill="currentColor" d="M12 6V3L8 7l4 4V8a4 4 0 1 1-4 4H6a6 6 0 1 0 6-6" />
                  </svg>
                )}
              </span>
              {syncState === "syncing"
                ? `同步中（${syncDone}/${syncTotal || "?"}）`
                : syncState === "success"
                  ? "同步完成"
                  : "重新同步"}
            </button>
          </>
        ) : null
      ) : null}
      <ThemeMenu />
    </>
  );

  return (
    <AppShell title={title} subtitle={subtitle} actions={actions} sidebar={sidebar}>
      {loading ? <p className="muted">Loading...</p> : null}
      {error ? <p className="error">{error}</p> : null}

      {bootstrap ? (
        route === "products" ? (
          <ProductsView
            bootstrap={bootstrap}
            countriesById={countriesById}
            regionsById={regionsById}
            onToggle={toggleMonitoring}
          />
        ) : route === "settings" ? (
          <SettingsViewPanel bootstrap={bootstrap} onSave={saveSettings} />
        ) : route === "logs" ? (
          <LogsView />
        ) : (
          <MonitoringView
            bootstrap={bootstrap}
            countriesById={countriesById}
            regionsById={regionsById}
            nowMs={nowMs}
            syncAlert={syncAlert}
            onDismissSyncAlert={() => {
              setSyncAlert(null);
              setSyncDone(0);
              setSyncTotal(0);
              setSyncState("idle");
            }}
          />
        )
      ) : null}
    </AppShell>
  );
}

function groupKey(c: Config): string {
  return `${c.countryId}::${c.regionId ?? ""}`;
}

function columnsForCount(count: number): string {
  if (count <= 2) return "repeat(auto-fit, minmax(280px, 504px))";
  return "repeat(auto-fit, minmax(280px, 328px))";
}

function summarizeSpecs(specs: Spec[], max: number): string {
  const parts: string[] = [];
  for (const s of specs) {
    const key = s.key.trim();
    const value = s.value.trim();
    if (!key && !value) continue;
    parts.push(key ? `${key} ${value}`.trim() : value);
    if (parts.length >= max) break;
  }
  return parts.join(" • ");
}

export function ProductCard({
  cfg,
  onToggle,
}: {
  cfg: Config;
  onToggle: (configId: string, enabled: boolean) => void;
}) {
  const isCloud = !cfg.monitorSupported;
  const invTone = isCloud
    ? "on"
    : cfg.inventory.status === "unknown" || cfg.inventory.quantity === 0
      ? "warn"
      : "on";
  const invText = isCloud
    ? "状态：有货"
    : cfg.inventory.status === "unknown"
      ? "库存 ?"
      : cfg.inventory.quantity > 0
        ? `库存 ${cfg.inventory.quantity}`
        : "库存 0";
  const invWidth = isCloud ? 120 : 96;
  const monitorTone = isCloud ? "disabled" : cfg.monitorEnabled ? "on" : "";
  const monitorText = isCloud ? "监控：禁用" : cfg.monitorEnabled ? "监控：开" : "监控：关";
  const foot = isCloud ? null : cfg.monitorEnabled ? "变化检测：补货 / 价格 / 配置" : null;

  return (
    <div className={`cfg-card ${isCloud ? "cloud" : "product"}`}>
      <div className="cfg-title">{cfg.name}</div>
      {isCloud ? null : <div className="cfg-sub">{summarizeSpecs(cfg.specs, 3) || "—"}</div>}
      <div className="cfg-price">{formatMoney(cfg.price)}</div>
      <div className="cfg-pills">
        <div className={`pill ${invTone} center`} style={{ width: `${invWidth}px` }}>
          {invText}
        </div>
        <button
          type="button"
          className={`pill ${monitorTone} center`}
          style={{ width: "96px" }}
          disabled={!cfg.monitorSupported}
          onClick={() => onToggle(cfg.id, !cfg.monitorEnabled)}
        >
          {monitorText}
        </button>
      </div>
      {foot ? <div className="cfg-foot">{foot}</div> : null}
    </div>
  );
}

export function MonitoringCard({ cfg, nowMs }: { cfg: Config; nowMs: number }) {
  const invTone =
    cfg.inventory.status === "unknown" || cfg.inventory.quantity === 0 ? "warn" : "on";
  const invText =
    cfg.inventory.status === "unknown"
      ? "库存：未知"
      : cfg.inventory.quantity > 0
        ? `库存 ${cfg.inventory.quantity}`
        : "库存 0";
  return (
    <div className="mon-card">
      <div className="mon-title">{cfg.name}</div>
      <div className="mon-sub">{summarizeSpecs(cfg.specs, 2) || "—"}</div>
      <div className="mon-price">{formatMoney(cfg.price)}</div>
      <div className="mon-pills">
        <div className={`pill ${invTone}`} style={{ width: "220px" }}>
          {invText}
        </div>
        <div
          className="pill"
          style={{ width: "220px" }}
        >{`更新：${formatRelativeTime(cfg.inventory.checkedAt, nowMs)}`}</div>
      </div>
    </div>
  );
}

export function ProductsView({
  bootstrap,
  countriesById,
  regionsById,
  onToggle,
}: {
  bootstrap: BootstrapResponse;
  countriesById: Map<string, Country>;
  regionsById: Map<string, Region>;
  onToggle: (configId: string, enabled: boolean) => void;
}) {
  const [countryFilter, setCountryFilter] = useState<string>("all");
  const [regionFilter, setRegionFilter] = useState<string>("all");
  const [search, setSearch] = useState<string>("");
  const [onlyMonitored, setOnlyMonitored] = useState<boolean>(false);

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

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return bootstrap.catalog.configs.filter((cfg) => {
      if (onlyMonitored && !cfg.monitorEnabled) return false;
      if (countryFilter !== "all" && cfg.countryId !== countryFilter) return false;
      if (regionFilter !== "all" && (cfg.regionId ?? "") !== regionFilter) return false;
      if (!q) return true;
      if (cfg.name.toLowerCase().includes(q)) return true;
      const specText = cfg.specs
        .map((s) => `${s.key} ${s.value}`.trim())
        .join(" ")
        .toLowerCase();
      return specText.includes(q);
    });
  }, [bootstrap, onlyMonitored, countryFilter, regionFilter, search]);

  const groups = useMemo(() => {
    const m = new Map<string, Config[]>();
    for (const cfg of filtered) {
      const k = groupKey(cfg);
      const list = m.get(k);
      if (list) list.push(cfg);
      else m.set(k, [cfg]);
    }
    const entries = Array.from(m.entries());
    entries.sort((a, b) => {
      const aCloud = countriesById.get(a[1][0]?.countryId ?? "")?.name.includes("云服务器");
      const bCloud = countriesById.get(b[1][0]?.countryId ?? "")?.name.includes("云服务器");
      if (aCloud && !bCloud) return 1;
      if (!aCloud && bCloud) return -1;
      return a[0].localeCompare(b[0]);
    });
    return entries;
  }, [filtered, countriesById]);

  return (
    <div className="panel">
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
        </div>
      </div>

      {groups.length === 0 ? <div className="empty">没有匹配的配置。</div> : null}

      {groups.map(([k, items]) => {
        const [countryId, regionId] = k.split("::");
        const country = countriesById.get(countryId)?.name ?? countryId;
        const isCloud = country.includes("云服务器");
        const title = isCloud
          ? country
          : `${country} / ${regionId ? (regionsById.get(regionId)?.name ?? regionId) : "默认"}`;
        const subtitle = isCloud
          ? "长期有货：不提供库存监控开关"
          : "配置卡片：规格 / 价格 / 库存 / 监控开关";
        return (
          <div className="panel-section" key={k}>
            <div className="panel-title">{title}</div>
            <div className="panel-subtitle">{subtitle}</div>
            <div className="divider-bleed" />
            <div className="grid" style={{ gridTemplateColumns: columnsForCount(items.length) }}>
              {items.map((cfg) => (
                <ProductCard cfg={cfg} key={cfg.id} onToggle={onToggle} />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

export function MonitoringView({
  bootstrap,
  countriesById,
  regionsById,
  nowMs,
  syncAlert,
  onDismissSyncAlert,
}: {
  bootstrap: BootstrapResponse;
  countriesById: Map<string, Country>;
  regionsById: Map<string, Region>;
  nowMs: number;
  syncAlert: string | null;
  onDismissSyncAlert: () => void;
}) {
  const enabled = useMemo(
    () => bootstrap.catalog.configs.filter((c) => c.monitorEnabled),
    [bootstrap],
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

  return (
    <div className="panel">
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
      {enabled.length === 0 ? (
        <div className="panel-section">
          <div className="empty">还没有启用监控的配置。去“全部产品”里点选需要监控的配置。</div>
        </div>
      ) : null}
      {groups.map(([k, items]) => {
        const [countryId, regionId] = k.split("::");
        const country = countriesById.get(countryId)?.name ?? countryId;
        const region = regionId ? (regionsById.get(regionId)?.name ?? regionId) : "默认";
        return (
          <MonitoringSection
            key={k}
            collapseKey={`catnap:collapse:${k}`}
            title={`${country} / ${region}`}
            items={items}
            nowMs={nowMs}
          />
        );
      })}
      <div className="panel-section">
        <div className="panel-title">提示</div>
        <div className="panel-subtitle">
          在“全部产品”中开启监控后，配置会出现在对应可用区行的网格里。
        </div>
        <div className="muted">轮询频率与抖动在“系统设置”中配置；日志可追溯每次变更与通知。</div>
      </div>
    </div>
  );
}

export function MonitoringSection({
  collapseKey,
  title,
  items,
  nowMs,
}: {
  collapseKey: string;
  title: string;
  items: Config[];
  nowMs: number;
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
          <div className="grid" style={{ gridTemplateColumns: columnsForCount(items.length) }}>
            {items.map((cfg) => (
              <MonitoringCard cfg={cfg} key={cfg.id} nowMs={nowMs} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

export function SettingsViewPanel({
  bootstrap,
  onSave,
}: {
  bootstrap: BootstrapResponse;
  onSave: (next: SettingsView & { telegramBotToken?: string | null }) => Promise<void>;
}) {
  const [intervalMinutes, setIntervalMinutes] = useState<number>(
    bootstrap.settings.poll.intervalMinutes,
  );
  const [jitterPct, setJitterPct] = useState<number>(bootstrap.settings.poll.jitterPct);
  const [siteBaseUrl, setSiteBaseUrl] = useState<string>(bootstrap.settings.siteBaseUrl ?? "");
  const [tgEnabled, setTgEnabled] = useState<boolean>(
    bootstrap.settings.notifications.telegram.enabled,
  );
  const [tgTarget, setTgTarget] = useState<string>(
    bootstrap.settings.notifications.telegram.target ?? "",
  );
  const [tgBotToken, setTgBotToken] = useState<string>("");
  const [wpEnabled, setWpEnabled] = useState<boolean>(
    bootstrap.settings.notifications.webPush.enabled,
  );
  const [wpStatus, setWpStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState<boolean>(false);

  const wpKey = bootstrap.settings.notifications.webPush.vapidPublicKey;
  const wpSupported = "serviceWorker" in navigator && "PushManager" in window;

  return (
    <div className="panel">
      <div className="panel-section">
        <div className="panel-title">轮询（Polling）</div>
        <div className="settings-grid">
          <div>查询频率（分钟）</div>
          <div className="pill" style={{ width: "120px" }}>
            <input
              type="number"
              min={1}
              value={intervalMinutes}
              onChange={(e) => setIntervalMinutes(Number(e.target.value))}
            />
          </div>
          <div className="hint">默认 1；最小 1</div>

          <div>抖动比例（0..1）</div>
          <div className="pill" style={{ width: "120px" }}>
            <input
              type="number"
              min={0}
              max={1}
              step={0.01}
              value={jitterPct}
              onChange={(e) => setJitterPct(Number(e.target.value))}
            />
          </div>
          <div className="hint">实际间隔 = interval × (1 ± jitter)</div>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">站点地址（用于通知跳转链接）</div>
        <div className="panel-subtitle">默认值：window.location.origin（用户可修改）</div>
        <div className="controls">
          <div className="pill" style={{ width: "848px" }}>
            <input
              placeholder={window.location.origin}
              value={siteBaseUrl}
              onChange={(e) => setSiteBaseUrl(e.target.value)}
            />
          </div>
          <button
            type="button"
            className="pill warn center"
            style={{ width: "160px" }}
            onClick={() => setSiteBaseUrl(window.location.origin)}
          >
            自动填充
          </button>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-title">通知（Notifications）</div>

        <div className="controls" style={{ marginTop: "16px" }}>
          <div className="panel-title" style={{ fontSize: "16px" }}>
            Telegram
          </div>
          <button
            type="button"
            className={`pill sm center ${tgEnabled ? "on" : ""}`}
            style={{ width: "92px" }}
            onClick={() => setTgEnabled((v) => !v)}
          >
            {tgEnabled ? "启用" : "关闭"}
          </button>
        </div>

        <div className="settings-row">
          <div>Bot Token（不回显）</div>
          <div className="pill">
            <input
              type="password"
              placeholder={
                bootstrap.settings.notifications.telegram.configured ? "••••••••••••••••" : ""
              }
              value={tgBotToken}
              onChange={(e) => setTgBotToken(e.target.value)}
            />
          </div>
        </div>

        <div className="settings-row" style={{ marginTop: "16px" }}>
          <div>Target（chat id 或频道）</div>
          <div className="pill">
            <input value={tgTarget} onChange={(e) => setTgTarget(e.target.value)} />
          </div>
        </div>

        <div className="line-inner" />

        <div className="controls" style={{ marginTop: 0 }}>
          <div className="panel-title" style={{ fontSize: "16px" }}>
            Web Push（浏览器推送）
          </div>
          <button
            type="button"
            className={`pill sm center ${wpEnabled ? "on" : ""}`}
            style={{ width: "92px" }}
            onClick={() => setWpEnabled((v) => !v)}
          >
            {wpEnabled ? "启用" : "关闭"}
          </button>
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
          <button
            type="button"
            className="pill warn center btn"
            disabled={saving || !wpKey || !wpSupported}
            onClick={async () => {
              setSaving(true);
              setWpStatus(null);
              try {
                setWpEnabled(true);
                await onSave({
                  poll: { intervalMinutes, jitterPct },
                  siteBaseUrl: siteBaseUrl.trim() ? siteBaseUrl.trim() : null,
                  notifications: {
                    telegram: {
                      enabled: tgEnabled,
                      configured: false,
                      target: tgTarget.trim() || undefined,
                    },
                    webPush: { enabled: true },
                  },
                  telegramBotToken: tgBotToken.trim() ? tgBotToken.trim() : null,
                });
                setTgBotToken("");

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
              } catch (e) {
                setWpStatus(e instanceof Error ? e.message : String(e));
              } finally {
                setSaving(false);
              }
            }}
          >
            启用推送
          </button>

          <button
            type="button"
            className="pill on center btn"
            disabled={saving}
            onClick={async () => {
              setSaving(true);
              try {
                await onSave({
                  poll: { intervalMinutes, jitterPct },
                  siteBaseUrl: siteBaseUrl.trim() ? siteBaseUrl.trim() : null,
                  notifications: {
                    telegram: {
                      enabled: tgEnabled,
                      configured: false,
                      target: tgTarget.trim() || undefined,
                    },
                    webPush: { enabled: wpEnabled },
                  },
                  telegramBotToken: tgBotToken.trim() ? tgBotToken.trim() : null,
                });
                setTgBotToken("");
              } finally {
                setSaving(false);
              }
            }}
          >
            保存设置
          </button>
        </div>
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
    <div className="panel">
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

          <div className="pill w-144">
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
