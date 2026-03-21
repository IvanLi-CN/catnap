import { describe, expect, it } from "vitest";
import { buildLazycatTrafficCycle } from "./lazycatTrafficCycle";

describe("buildLazycatTrafficCycle", () => {
  it("handles missing history by returning an empty snapshot", () => {
    const snapshot = buildLazycatTrafficCycle({
      usedGb: 702,
      limitGb: 800,
      resetDay: 11,
      cycleStartAt: "2026-03-11T00:00:00Z",
      cycleEndAt: "2026-04-11T00:00:00Z",
      display: "702.00 GB / 800 GB",
    });

    expect(snapshot).not.toBeNull();
    expect(snapshot?.hasSamples).toBe(false);
    expect(snapshot?.points).toEqual([]);
  });

  it("converts gb counters into the provider display unit", () => {
    const snapshot = buildLazycatTrafficCycle({
      usedGb: 1024,
      limitGb: 2048,
      resetDay: 11,
      cycleStartAt: "2026-03-11T00:00:00Z",
      cycleEndAt: "2026-04-11T00:00:00Z",
      history: [
        {
          sampledAt: "2026-03-18T00:20:00Z",
          usedGb: 1024,
          limitGb: 2048,
        },
      ],
      display: "0.98 TiB / 2 TiB",
    });

    expect(snapshot?.displayUnit).toBe("TiB");
    expect(snapshot?.usedValue).toBeCloseTo(1, 6);
    expect(snapshot?.limitValue).toBeCloseTo(2, 6);
    expect(snapshot?.points[0]?.usedValue).toBeCloseTo(1, 6);
    expect(snapshot?.usageLabel).toContain("TiB");
    expect(snapshot?.usageLabel).toContain("1");
    expect(snapshot?.limitLabel).toContain("2");
  });
});
