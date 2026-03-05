import type { Meta, StoryObj } from "@storybook/react";
import { LogsView } from "../../App";
import { demoLogsResponse } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

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
    <ResponsivePageStory
      route="logs"
      title="Catnap • 日志"
      subtitle="使用顶部 Viewport 选择断点进行验收"
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
    await expectResponsiveBreakpoints(canvasElement, "page-logs");
  },
};
