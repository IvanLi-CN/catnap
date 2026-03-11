import type { Meta, StoryObj } from "@storybook/react";
import { NotificationsView } from "../../App";
import { demoNotificationRecords, demoNotificationRecordsResponse } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

async function demoFetchRecord(id: string) {
  const found = demoNotificationRecords.find((item) => item.id === id);
  if (!found) throw new Error("not found");
  return found;
}

const meta = {
  title: "Pages/NotificationsView",
  component: NotificationsView,
  render: () => (
    <NotificationsView
      fetchRecords={async (params) => demoNotificationRecordsResponse(params)}
      fetchRecord={demoFetchRecord}
      nowMs={Date.now()}
    />
  ),
} satisfies Meta<typeof NotificationsView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const DeepLinkedRecord: Story = {
  render: () => (
    <NotificationsView
      fetchRecords={async (params) => demoNotificationRecordsResponse(params)}
      fetchRecord={demoFetchRecord}
      targetRecordId="nr-1"
      nowMs={Date.now()}
    />
  ),
};

export const MissingTarget: Story = {
  render: () => (
    <NotificationsView
      fetchRecords={async (params) => demoNotificationRecordsResponse(params)}
      fetchRecord={async () => {
        throw new Error("记录不存在或已过期");
      }}
      targetRecordId="nr-missing"
      nowMs={Date.now()}
    />
  ),
};

export const ResponsiveAllBreakpoints: Story = {
  render: () => (
    <ResponsivePageStory
      route="notifications"
      title="Catnap • 通知记录"
      subtitle="使用顶部 Viewport 选择断点进行验收"
      actions={<span className="pill sm">theme</span>}
      renderPage={() => (
        <NotificationsView
          fetchRecords={async (params) => demoNotificationRecordsResponse(params)}
          fetchRecord={demoFetchRecord}
          nowMs={Date.now()}
        />
      )}
    />
  ),
  play: async ({ canvasElement }) => {
    await expectResponsiveBreakpoints(canvasElement, "page-notifications");
  },
};
