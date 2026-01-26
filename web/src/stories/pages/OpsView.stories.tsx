import type { Meta } from "@storybook/react";
import { type OpsStateResponse, OpsView } from "../../App";

const demo: OpsStateResponse = {
  serverTime: "2026-01-26T00:00:00Z",
  range: "24h",
  replayWindowSeconds: 3600,
  queue: { pending: 3, running: 1, deduped: 5 },
  workers: [
    {
      workerId: "w1",
      state: "running",
      task: { fid: "7", gid: "40" },
      startedAt: "2026-01-26T00:00:00Z",
      lastError: null,
    },
    {
      workerId: "w2",
      state: "idle",
      task: null,
      startedAt: null,
      lastError: null,
    },
  ],
  tasks: [
    {
      key: { fid: "7", gid: "40" },
      state: "running",
      enqueuedAt: "2026-01-26T00:00:00Z",
      reasonCounts: { poller_due: 2, manual_refresh: 1 },
      lastRun: { endedAt: "2026-01-25T23:58:00Z", ok: true },
    },
    {
      key: { fid: "2", gid: "56" },
      state: "pending",
      enqueuedAt: "2026-01-26T00:00:00Z",
      reasonCounts: { manual_refresh: 1 },
      lastRun: null,
    },
  ],
  stats: {
    collection: { total: 42, success: 40, failure: 2, successRatePct: 95.2 },
    notify: {
      telegram: { total: 12, success: 11, failure: 1, successRatePct: 91.7 },
      webPush: { total: 6, success: 6, failure: 0, successRatePct: 100.0 },
    },
  },
  sparks: {
    bucketSeconds: 3600,
    volume: [3, 4, 2, 6, 8, 7, 5, 6],
    collectionSuccessRatePct: [80, 86, 90, 92, 94, 96, 95, 97],
    notifyTelegramSuccessRatePct: [88, 90, 91, 92, 90, 93, 94, 95],
    notifyWebPushSuccessRatePct: [92, 92, 93, 94, 95, 96, 96, 97],
  },
  logTail: [
    {
      eventId: 1001,
      ts: "2026-01-26T00:00:00Z",
      level: "info",
      scope: "ops.task",
      message: "task ok: fid=7 gid=40",
      meta: { runId: 123 },
    },
    {
      eventId: 1002,
      ts: "2026-01-26T00:00:01Z",
      level: "warn",
      scope: "notify.telegram",
      message: "notify telegram: error (telegram http 500)",
      meta: { runId: 123 },
    },
  ],
};

function NoopOpsView() {
  return (
    <OpsView
      fetchState={async () => demo}
      createEventSource={() => ({ close() {}, addEventListener() {} }) as unknown as EventSource}
    />
  );
}

export default {
  title: "Pages/OpsView",
  component: NoopOpsView,
} satisfies Meta<typeof NoopOpsView>;

export const Default = { render: () => <NoopOpsView /> };
