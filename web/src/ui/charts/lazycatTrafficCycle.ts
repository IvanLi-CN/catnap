export type LazycatTrafficSnapshotInput = {
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

export type LazycatTrafficCyclePoint = {
  kind: "sample";
  ts: number;
  usedGb: number;
  limitGb: number;
};

export type LazycatTrafficCycleSnapshot = {
  currentAt: number;
  currentLabel: string;
  displayUnit: string;
  endAt: number;
  endLabel: string;
  hasSamples: boolean;
  lastResetLabel: string;
  limitGb: number;
  limitLabel: string;
  points: LazycatTrafficCyclePoint[];
  rangeLabel: string;
  remainingGb: number;
  remainingLabel: string;
  sampleCountLabel: string;
  startAt: number;
  startLabel: string;
  ticks: number[];
  yTicks: number[];
  usageLabel: string;
  usagePct: number;
  usedGb: number;
  yDomainMax: number;
};

function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function formatDateTick(ts: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(ts));
}

function formatDateTime(ts: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(ts));
}

function isValidDate(value: Date | null): value is Date {
  return Boolean(value && Number.isFinite(value.getTime()));
}

function parseDate(value: string | null | undefined): Date | null {
  if (!value) return null;
  const parsed = new Date(value);
  return isValidDate(parsed) ? parsed : null;
}

function buildTicks(startAt: number, currentAt: number, endAt: number): number[] {
  const gap = Math.max((endAt - startAt) * 0.14, 18 * 60 * 60 * 1000);
  const ticks = [startAt];
  if (currentAt - startAt > gap && endAt - currentAt > gap) {
    ticks.push(currentAt);
  }
  ticks.push(endAt);
  return ticks;
}

export function formatTrafficValue(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return new Intl.NumberFormat(undefined, {
    minimumFractionDigits: value >= 100 ? 0 : 1,
    maximumFractionDigits: value >= 100 ? 1 : 2,
  }).format(value);
}

export function formatTrafficPercent(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return new Intl.NumberFormat(undefined, {
    minimumFractionDigits: value >= 100 ? 0 : 1,
    maximumFractionDigits: value >= 100 ? 0 : 1,
  }).format(value);
}

export function getTrafficUsageTone(usagePct: number): "ok" | "warn" | "danger" {
  if (usagePct >= 90) return "danger";
  if (usagePct >= 75) return "warn";
  return "ok";
}

export function formatTrafficTick(ts: number): string {
  return formatDateTick(ts);
}

function buildTrafficYTicks(limitGb: number): number[] {
  if (!Number.isFinite(limitGb) || limitGb <= 0) return [0];
  const mid = limitGb / 2;
  return [0, mid, limitGb];
}

export function buildLazycatTrafficCycle(
  traffic: LazycatTrafficSnapshotInput,
): LazycatTrafficCycleSnapshot | null {
  const cycleStart = parseDate(traffic.cycleStartAt);
  const cycleEnd = parseDate(traffic.cycleEndAt);
  if (!isValidDate(cycleStart) || !isValidDate(cycleEnd) || cycleEnd <= cycleStart) {
    return null;
  }

  const points = traffic.history
    .map((point) => {
      const sampledAt = parseDate(point.sampledAt);
      if (!isValidDate(sampledAt)) return null;
      return {
        kind: "sample" as const,
        ts: sampledAt.getTime(),
        usedGb: point.usedGb,
        limitGb: point.limitGb,
      };
    })
    .filter((point): point is LazycatTrafficCyclePoint => {
      return Boolean(
        point &&
          Number.isFinite(point.usedGb) &&
          Number.isFinite(point.limitGb) &&
          point.ts >= cycleStart.getTime() &&
          point.ts <= cycleEnd.getTime(),
      );
    })
    .sort((left, right) => left.ts - right.ts);

  const displayUnit = traffic.display?.trim() || "GB";
  const usagePct = traffic.limitGb > 0 ? (traffic.usedGb / traffic.limitGb) * 100 : 0;
  const remainingGb = traffic.limitGb - traffic.usedGb;
  const hasSamples = points.length > 0;
  const currentAt = clampNumber(
    points[points.length - 1]?.ts ?? cycleStart.getTime(),
    cycleStart.getTime(),
    cycleEnd.getTime(),
  );
  const usageTop = Math.max(
    traffic.limitGb,
    traffic.usedGb,
    ...points.map((point) => point.usedGb),
    1,
  );

  return {
    currentAt,
    currentLabel: hasSamples ? formatDateTime(currentAt) : "暂无小时样本",
    displayUnit,
    endAt: cycleEnd.getTime(),
    endLabel: formatDateTime(cycleEnd.getTime()),
    hasSamples,
    lastResetLabel: formatDateTime(
      parseDate(traffic.lastResetAt)?.getTime() ?? cycleStart.getTime(),
    ),
    limitGb: traffic.limitGb,
    limitLabel: `${formatTrafficValue(traffic.limitGb)} ${displayUnit}`,
    points,
    rangeLabel: `${formatDateTime(cycleStart.getTime())} - ${formatDateTime(cycleEnd.getTime())}`,
    remainingGb,
    remainingLabel:
      remainingGb >= 0
        ? `${formatTrafficValue(remainingGb)} ${displayUnit} 剩余`
        : `超出 ${formatTrafficValue(Math.abs(remainingGb))} ${displayUnit}`,
    sampleCountLabel: hasSamples
      ? `${points.length} 个小时样本`
      : "0 个小时样本（显示缓存摘要）",
    startAt: cycleStart.getTime(),
    startLabel: formatDateTime(cycleStart.getTime()),
    ticks: buildTicks(cycleStart.getTime(), currentAt, cycleEnd.getTime()),
    yTicks: buildTrafficYTicks(usageTop),
    usageLabel: `${formatTrafficValue(traffic.usedGb)} / ${formatTrafficValue(traffic.limitGb)} ${displayUnit}`,
    usagePct,
    usedGb: traffic.usedGb,
    yDomainMax: usageTop,
  };
}
