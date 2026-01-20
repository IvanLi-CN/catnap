import type { Meta, StoryObj } from "@storybook/react";
import { LogsView } from "../../App";
import { demoLogsResponse } from "../fixtures";

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
