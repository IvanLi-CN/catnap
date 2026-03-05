export type ResponsiveBreakpoint = {
  id: string;
  label: string;
  range: string;
  width: number;
  height: number;
};

export const RESPONSIVE_BREAKPOINTS: ResponsiveBreakpoint[] = [
  { id: "bp-360", label: "360 / small phone", range: "360-479", width: 360, height: 780 },
  { id: "bp-480", label: "480 / large phone", range: "480-767", width: 480, height: 900 },
  { id: "bp-768", label: "768 / tablet portrait", range: "768-1023", width: 768, height: 1024 },
  {
    id: "bp-1024",
    label: "1024 / tablet landscape",
    range: "1024-1219",
    width: 1024,
    height: 1366,
  },
  { id: "bp-1220", label: "1220 / laptop", range: "1220-1439", width: 1220, height: 1600 },
  { id: "bp-1440", label: "1440 / desktop", range: "1440-1680", width: 1440, height: 1800 },
  { id: "bp-1680", label: "1680 / wide desktop", range: ">=1680", width: 1680, height: 2100 },
];

export const RESPONSIVE_BREAKPOINTS_BY_ID = new Map(
  RESPONSIVE_BREAKPOINTS.map((item) => [item.id, item] as const),
);
