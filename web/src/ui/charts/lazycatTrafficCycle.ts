export type LazycatTrafficSnapshotInput = {
  usedGb: number;
  limitGb: number;
  resetDay: number;
  lastResetAt?: string | null;
  display?: string | null;
};

export type LazycatTrafficCyclePoint = {
  kind: "start" | "current" | "end";
  ts: number;
  usedGb: number | null;
};

export type LazycatTrafficCycleSnapshot = {
  currentAt: number;
  currentLabel: string;
  displayUnit: string;
  endAt: number;
  endLabel: string;
  lastResetLabel: string;
  limitGb: number;
  limitLabel: string;
  points: LazycatTrafficCyclePoint[];
  rangeLabel: string;
  remainingGb: number;
  remainingLabel: string;
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

function createMonthlyAnchor(base: Date, monthOffset: number, resetDay: number): Date {
  const day = clampNumber(Math.trunc(resetDay) || 1, 1, 31);
  const candidate = new Date(base);
  candidate.setMonth(candidate.getMonth() + monthOffset, 1);
  const lastDay = new Date(candidate.getFullYear(), candidate.getMonth() + 1, 0).getDate();
  candidate.setDate(Math.min(day, lastDay));
  return candidate;
}

function inferCycleStart(resetDay: number, now: Date): Date {
  const currentMonthAnchor = createMonthlyAnchor(now, 0, resetDay);
  if (currentMonthAnchor <= now) return currentMonthAnchor;
  return createMonthlyAnchor(now, -1, resetDay);
}

function computeCycleStart(traffic: LazycatTrafficSnapshotInput, now: Date): Date {
  const parsedStart = parseDate(traffic.lastResetAt);
  if (isValidDate(parsedStart)) {
    const parsedEnd = createMonthlyAnchor(parsedStart, 1, traffic.resetDay);
    if (parsedStart <= now && parsedEnd > now) return parsedStart;
  }
  return inferCycleStart(traffic.resetDay, now);
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
  now = new Date(),
): LazycatTrafficCycleSnapshot {
  const cycleStart = computeCycleStart(traffic, now);
  const cycleEnd = createMonthlyAnchor(cycleStart, 1, traffic.resetDay);

  const safeCurrentAt = clampNumber(
    now.getTime(),
    cycleStart.getTime() + 60_000,
    cycleEnd.getTime() - 60_000,
  );

  const displayUnit = traffic.display?.trim() || "GB";
  const usagePct = traffic.limitGb > 0 ? (traffic.usedGb / traffic.limitGb) * 100 : 0;
  const remainingGb = traffic.limitGb - traffic.usedGb;
  const usageTop = Math.max(traffic.limitGb, traffic.usedGb, 1);
  const points: LazycatTrafficCyclePoint[] = [
    { kind: "start", ts: cycleStart.getTime(), usedGb: 0 },
    { kind: "current", ts: safeCurrentAt, usedGb: traffic.usedGb },
    { kind: "end", ts: cycleEnd.getTime(), usedGb: null },
  ];

  return {
    currentAt: safeCurrentAt,
    currentLabel: formatDateTime(safeCurrentAt),
    displayUnit,
    endAt: cycleEnd.getTime(),
    endLabel: formatDateTime(cycleEnd.getTime()),
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
    startAt: cycleStart.getTime(),
    startLabel: formatDateTime(cycleStart.getTime()),
    ticks: buildTicks(cycleStart.getTime(), safeCurrentAt, cycleEnd.getTime()),
    yTicks: buildTrafficYTicks(usageTop),
    usageLabel: `${formatTrafficValue(traffic.usedGb)} / ${formatTrafficValue(traffic.limitGb)} ${displayUnit}`,
    usagePct,
    usedGb: traffic.usedGb,
    yDomainMax: usageTop,
  };
}
