import { type CSSProperties, useId } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  type TooltipContentProps,
  XAxis,
  YAxis,
} from "recharts";
import {
  type LazycatTrafficCycleSnapshot,
  formatTrafficPercent,
  formatTrafficTick,
  formatTrafficValue,
  getTrafficUsageTone,
} from "./lazycatTrafficCycle";

type LazycatTrafficCycleChartProps = {
  serviceId: number;
  snapshot: LazycatTrafficCycleSnapshot;
};

type TrafficTooltipProps = TooltipContentProps & {
  snapshot: LazycatTrafficCycleSnapshot;
};

type TonePalette = {
  accent: string;
  accentSoft: string;
  dash: string;
  fillStart: string;
  fillStop: string;
};

const PALETTES: Record<ReturnType<typeof getTrafficUsageTone>, TonePalette> = {
  ok: {
    accent: "#4fd1c5",
    accentSoft: "rgba(79, 209, 197, 0.2)",
    dash: "#7dd3fc",
    fillStart: "rgba(79, 209, 197, 0.34)",
    fillStop: "rgba(79, 209, 197, 0.03)",
  },
  warn: {
    accent: "#fbbf24",
    accentSoft: "rgba(251, 191, 36, 0.18)",
    dash: "#fcd34d",
    fillStart: "rgba(251, 191, 36, 0.3)",
    fillStop: "rgba(251, 191, 36, 0.03)",
  },
  danger: {
    accent: "#fb7185",
    accentSoft: "rgba(251, 113, 133, 0.18)",
    dash: "#fdba74",
    fillStart: "rgba(251, 113, 133, 0.32)",
    fillStop: "rgba(251, 113, 133, 0.04)",
  },
};

function TrafficTooltip({ active, payload, snapshot }: TrafficTooltipProps) {
  if (!active || !payload?.length) return null;
  const point = payload[0]?.payload as
    | {
        kind?: "start" | "current" | "end";
        ts?: number;
        usedGb?: number;
      }
    | undefined;

  if (!point) return null;

  const title =
    point.kind === "start" ? "账期开始" : point.kind === "end" ? "下次重置" : "当前累计";
  const timeLabel =
    point.kind === "start"
      ? snapshot.startLabel
      : point.kind === "end"
        ? snapshot.endLabel
        : snapshot.currentLabel;

  return (
    <div className="machines-traffic-tooltip">
      <div className="machines-traffic-tooltip-label">{title}</div>
      <div className="machines-traffic-tooltip-time">{timeLabel}</div>
      <div className="machines-traffic-tooltip-value">
        {`${formatTrafficValue(point.usedGb ?? 0)} ${snapshot.displayUnit}`}
      </div>
      {point.kind === "current" ? (
        <div className="machines-traffic-tooltip-meta">
          {`${snapshot.remainingLabel} · ${formatTrafficPercent(snapshot.usagePct)}%`}
        </div>
      ) : null}
    </div>
  );
}

export function LazycatTrafficCycleChart({ serviceId, snapshot }: LazycatTrafficCycleChartProps) {
  const tone = getTrafficUsageTone(snapshot.usagePct);
  const palette = PALETTES[tone];
  const reactId = useId().replace(/:/g, "");
  const gradientId = `machines-traffic-${serviceId}-${reactId}`;
  const chartStyle = {
    "--machines-traffic-accent": palette.accent,
    "--machines-traffic-accent-soft": palette.accentSoft,
    "--machines-traffic-dash": palette.dash,
    "--machines-traffic-fill-start": palette.fillStart,
    "--machines-traffic-fill-stop": palette.fillStop,
  } as CSSProperties;

  return (
    <div className={`machines-traffic-panel machines-traffic-panel--${tone}`} style={chartStyle}>
      <div className="machines-traffic-panel-head">
        <div className="machines-traffic-panel-copy">
          <span className="machines-traffic-panel-label">账期流量</span>
          <strong>{snapshot.usageLabel}</strong>
        </div>
        <div className="machines-traffic-panel-stats">
          <span className="machines-traffic-chip">{snapshot.remainingLabel}</span>
          <span className="machines-traffic-ratio">{`${formatTrafficPercent(snapshot.usagePct)}%`}</span>
        </div>
      </div>

      <div className="machines-traffic-panel-range">{snapshot.rangeLabel}</div>

      <div
        className="machines-traffic-chart"
        aria-label={`当前账期流量 ${snapshot.usageLabel}，可用周期 ${snapshot.rangeLabel}`}
        role="img"
      >
        <ResponsiveContainer width="100%" height="100%" minWidth={0} minHeight={208}>
          <AreaChart
            data={snapshot.points}
            margin={{ top: 12, right: 8, bottom: 0, left: -18 }}
            title={`Traffic cycle ${snapshot.usageLabel}, range ${snapshot.rangeLabel}`}
          >
            <defs>
              <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="var(--machines-traffic-fill-start)" stopOpacity={1} />
                <stop offset="95%" stopColor="var(--machines-traffic-fill-stop)" stopOpacity={1} />
              </linearGradient>
            </defs>

            <CartesianGrid
              stroke="color-mix(in oklab, var(--line) 76%, transparent)"
              strokeDasharray="4 6"
              vertical={false}
            />
            <XAxis
              axisLine={false}
              dataKey="ts"
              domain={[snapshot.startAt, snapshot.endAt]}
              minTickGap={18}
              tick={{ fill: "var(--muted)", fontSize: 12 }}
              tickFormatter={formatTrafficTick}
              tickLine={false}
              ticks={snapshot.ticks}
              type="number"
            />
            <YAxis
              axisLine={false}
              domain={[0, snapshot.yDomainMax]}
              tick={{ fill: "var(--muted)", fontSize: 12 }}
              tickFormatter={(value) => formatTrafficValue(Number(value))}
              tickLine={false}
              width={54}
            />
            <Tooltip
              content={(props) => <TrafficTooltip {...props} snapshot={snapshot} />}
              cursor={{
                stroke: "var(--machines-traffic-accent-soft)",
                strokeDasharray: "3 5",
                strokeWidth: 1,
              }}
            />
            <Area
              activeDot={{
                fill: "var(--machines-traffic-accent)",
                r: 4,
                stroke: "var(--surface)",
                strokeWidth: 2,
              }}
              dataKey="usedGb"
              fill={`url(#${gradientId})`}
              fillOpacity={1}
              isAnimationActive={false}
              stroke="var(--machines-traffic-accent)"
              strokeWidth={3}
              type="linear"
            />
            <ReferenceLine
              ifOverflow="extendDomain"
              segment={[
                { x: snapshot.currentAt, y: snapshot.usedGb },
                { x: snapshot.currentAt, y: snapshot.limitGb },
              ]}
              stroke="var(--machines-traffic-dash)"
              strokeDasharray="6 5"
              strokeLinecap="round"
              strokeWidth={2}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>

      <div className="machines-traffic-panel-foot">
        <span>{`上限 ${snapshot.limitLabel}`}</span>
        <span>{`当前点 ${snapshot.currentLabel}`}</span>
        <span>虚线段 = 当前累计到上限</span>
      </div>
    </div>
  );
}
