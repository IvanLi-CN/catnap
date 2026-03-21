export type LazycatTrafficSnapshotInput = {
  usedGb: number;
  limitGb: number;
  resetDay: number;
  cycleStartAt: string;
  cycleEndAt: string;
  history?: Array<{
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
  usedValue: number;
  limitValue: number;
};

export type LazycatTrafficCycleSnapshot = {
  currentAt: number;
  currentLabel: string;
  displayUnit: string;
  endAt: number;
  endLabel: string;
  hasSamples: boolean;
  lastResetLabel: string | null;
  limitValue: number;
  limitLabel: string;
  points: LazycatTrafficCyclePoint[];
  rangeLabel: string;
  remainingValue: number;
  remainingLabel: string;
  sampleCountLabel: string;
  startAt: number;
  startLabel: string;
  ticks: number[];
  yTicks: number[];
  usageLabel: string;
  usagePct: number;
  usedValue: number;
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

function buildTrafficYTicks(limitValue: number): number[] {
  if (!Number.isFinite(limitValue) || limitValue <= 0) return [0];
  const mid = limitValue / 2;
  return [0, mid, limitValue];
}

function deriveTrafficDisplayUnit(display: string | null | undefined): string {
  const unitTokens = (display ?? "")
    .match(/[A-Za-z]{1,8}/g)
    ?.map((token) => token.trim())
    .filter((token) => token.length > 0);
  if (!unitTokens || unitTokens.length === 0) {
    return "GB";
  }
  const distinctUnits = Array.from(new Set(unitTokens));
  return distinctUnits.length === 1 ? distinctUnits[0] : unitTokens[unitTokens.length - 1];
}

function parseTrafficDisplayPart(part: string): { value?: number; unit?: string } {
  const valueMatch = part.match(/-?\d+(?:\.\d+)?/);
  const unitMatch = part.match(/[A-Za-z]{1,8}(?=[^A-Za-z]*$)/);
  return {
    value: valueMatch ? Number(valueMatch[0]) : undefined,
    unit: unitMatch?.[0],
  };
}

function normalizeTrafficUnit(unit: string): string {
  return unit.trim().toLowerCase();
}

function getCanonicalGbPerUnit(unit: string): number {
  switch (normalizeTrafficUnit(unit)) {
    case "kb":
      return 1 / 1_000_000;
    case "mb":
      return 1 / 1_000;
    case "gb":
    case "gib":
      return 1;
    case "tb":
      return 1_000;
    case "tib":
      return 1_024;
    case "mib":
      return 1 / 1_024;
    case "kib":
      return 1 / (1_024 * 1_024);
    default:
      return 1;
  }
}

function deriveTrafficScale(traffic: LazycatTrafficSnapshotInput): {
  displayUnit: string;
  gbPerUnit: number;
} {
  const displayUnit = deriveTrafficDisplayUnit(traffic.display);
  const [usedPart = "", limitPart = ""] = (traffic.display ?? "").split("/");
  const parsedUsed = parseTrafficDisplayPart(usedPart);
  const parsedLimit = parseTrafficDisplayPart(limitPart);
  const normalizedUnit = normalizeTrafficUnit(displayUnit);

  const candidateGbPerUnit = [
    parsedLimit.unit && normalizeTrafficUnit(parsedLimit.unit) === normalizedUnit
      ? traffic.limitGb / (parsedLimit.value ?? Number.NaN)
      : Number.NaN,
    getCanonicalGbPerUnit(displayUnit),
    parsedUsed.unit && normalizeTrafficUnit(parsedUsed.unit) === normalizedUnit
      ? traffic.usedGb / (parsedUsed.value ?? Number.NaN)
      : Number.NaN,
  ].find((value) => Number.isFinite(value) && value > 0);

  return {
    displayUnit,
    gbPerUnit: candidateGbPerUnit ?? 1,
  };
}

function convertGbToDisplayValue(valueGb: number, gbPerUnit: number): number {
  if (!Number.isFinite(valueGb) || !Number.isFinite(gbPerUnit) || gbPerUnit <= 0) {
    return valueGb;
  }
  return valueGb / gbPerUnit;
}

export function buildLazycatTrafficCycle(
  traffic: LazycatTrafficSnapshotInput,
): LazycatTrafficCycleSnapshot | null {
  const cycleStart = parseDate(traffic.cycleStartAt);
  const cycleEnd = parseDate(traffic.cycleEndAt);
  if (!isValidDate(cycleStart) || !isValidDate(cycleEnd) || cycleEnd <= cycleStart) {
    return null;
  }

  const { displayUnit, gbPerUnit } = deriveTrafficScale(traffic);
  const points = (traffic.history ?? [])
    .map((point): LazycatTrafficCyclePoint | null => {
      const sampledAt = parseDate(point.sampledAt);
      if (!isValidDate(sampledAt)) return null;
      return {
        kind: "sample" as const,
        ts: sampledAt.getTime(),
        usedValue: convertGbToDisplayValue(point.usedGb, gbPerUnit),
        limitValue: convertGbToDisplayValue(point.limitGb, gbPerUnit),
      };
    })
    .filter((point): point is LazycatTrafficCyclePoint => {
      return Boolean(
        point &&
          Number.isFinite(point.usedValue) &&
          Number.isFinite(point.limitValue) &&
          point.ts >= cycleStart.getTime() &&
          point.ts <= cycleEnd.getTime(),
      );
    })
    .sort((left, right) => left.ts - right.ts);

  const usagePct = traffic.limitGb > 0 ? (traffic.usedGb / traffic.limitGb) * 100 : 0;
  const usedValue = convertGbToDisplayValue(traffic.usedGb, gbPerUnit);
  const limitValue = convertGbToDisplayValue(traffic.limitGb, gbPerUnit);
  const remainingValue = convertGbToDisplayValue(traffic.limitGb - traffic.usedGb, gbPerUnit);
  const hasSamples = points.length > 0;
  const lastResetAt = parseDate(traffic.lastResetAt)?.getTime();
  const currentAt = clampNumber(
    points[points.length - 1]?.ts ?? cycleStart.getTime(),
    cycleStart.getTime(),
    cycleEnd.getTime(),
  );
  const usageTop = Math.max(limitValue, usedValue, ...points.map((point) => point.usedValue), 1);

  return {
    currentAt,
    currentLabel: hasSamples ? formatDateTime(currentAt) : "暂无小时样本",
    displayUnit,
    endAt: cycleEnd.getTime(),
    endLabel: formatDateTime(cycleEnd.getTime()),
    hasSamples,
    lastResetLabel: lastResetAt ? formatDateTime(lastResetAt) : null,
    limitValue,
    limitLabel: `${formatTrafficValue(limitValue)} ${displayUnit}`,
    points,
    rangeLabel: `${formatDateTime(cycleStart.getTime())} - ${formatDateTime(cycleEnd.getTime())}`,
    remainingValue,
    remainingLabel:
      remainingValue >= 0
        ? `${formatTrafficValue(remainingValue)} ${displayUnit} 剩余`
        : `超出 ${formatTrafficValue(Math.abs(remainingValue))} ${displayUnit}`,
    sampleCountLabel: hasSamples ? `${points.length} 个小时样本` : "0 个小时样本（显示缓存摘要）",
    startAt: cycleStart.getTime(),
    startLabel: formatDateTime(cycleStart.getTime()),
    ticks: buildTicks(cycleStart.getTime(), currentAt, cycleEnd.getTime()),
    yTicks: buildTrafficYTicks(usageTop),
    usageLabel: `${formatTrafficValue(usedValue)} / ${formatTrafficValue(limitValue)} ${displayUnit}`,
    usagePct,
    usedValue,
    yDomainMax: usageTop,
  };
}
