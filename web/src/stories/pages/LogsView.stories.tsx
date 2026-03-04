import type { Meta, StoryObj } from "@storybook/react";
import { LogsView } from "../../App";
import { demoLogsResponse } from "../fixtures";
import { ResponsivePageMatrix, expectResponsivePageCases } from "./responsivePageHelpers";

const meta = {
  title: "Pages/LogsView",
  component: LogsView,
  render: () => (
    <LogsView
      fetchLogs={async (params) => {
        return demoLogsResponse(params);
      }}
    />
  ),
} satisfies Meta<typeof LogsView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const ResponsiveAllBreakpoints: Story = {
  render: () => (
    <ResponsivePageMatrix
      route="logs"
      title="Catnap • 日志"
      subtitle="响应式断点 DOM 验收矩阵"
      actions={<span className="pill sm">theme</span>}
      renderPage={() => (
        <LogsView
          fetchLogs={async (params) => {
            return demoLogsResponse(params);
          }}
        />
      )}
    />
  ),
  play: async ({ canvasElement }) => {
    await expectResponsivePageCases(canvasElement, "page-logs");
  },
};
