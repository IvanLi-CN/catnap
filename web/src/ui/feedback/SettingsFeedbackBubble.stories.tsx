import type { Meta, StoryObj } from "@storybook/react-vite";
import { useRef, useState } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";
import { SettingsFeedbackBubble, type SettingsFeedbackTone } from "./SettingsFeedbackBubble";

type InlineDemoProps = {
  message: string | null;
  placement?: "top" | "bottom" | "left" | "right";
  tone: SettingsFeedbackTone;
};

function InlineFeedbackBubbleDemo({ message, placement = "top", tone }: InlineDemoProps) {
  const anchorRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(true);

  return (
    <div
      style={{
        minHeight: 240,
        display: "grid",
        placeItems: "center",
        padding: 48,
      }}
    >
      <div className="settings-action-wrap settings-action-wrap-inline-feedback">
        <button
          className="pill warn center btn"
          onClick={() => setOpen(true)}
          ref={anchorRef}
          style={{ width: 160 }}
          type="button"
        >
          测试通知
        </button>
        {open ? (
          <SettingsFeedbackBubble
            anchorRef={anchorRef}
            inline
            message={message}
            onClose={() => setOpen(false)}
            placement={placement}
            testId="settings-feedback-bubble-story"
            tone={tone}
          />
        ) : null}
      </div>
    </div>
  );
}

function StaticFeedbackBubbleDemo({
  message,
  tone,
}: { message: string | null; tone: SettingsFeedbackTone }) {
  const [open, setOpen] = useState(true);

  return (
    <div
      style={{
        minHeight: 220,
        padding: "88px 48px 48px",
      }}
    >
      <div className="settings-action-wrap" style={{ position: "relative" }}>
        <button className="pill warn center btn" style={{ width: 160 }} type="button">
          字段操作
        </button>
        {open ? (
          <SettingsFeedbackBubble
            message={message}
            onClose={() => setOpen(false)}
            testId="settings-feedback-bubble-static-story"
            tone={tone}
          />
        ) : null}
      </div>
    </div>
  );
}

function NeutralTooltipDemo() {
  const anchorRef = useRef<HTMLButtonElement | null>(null);

  return (
    <div
      style={{
        minHeight: 240,
        display: "grid",
        placeItems: "center",
        padding: 48,
      }}
    >
      <div className="settings-action-wrap settings-action-wrap-inline-feedback">
        <button
          className="pill warn center btn"
          ref={anchorRef}
          style={{ width: 160 }}
          type="button"
        >
          SSE 状态
        </button>
        <SettingsFeedbackBubble
          anchorRef={anchorRef}
          inline
          message={null}
          open
          placement="bottom-end"
          role="tooltip"
          showIcon={false}
          tone="neutral"
        >
          <div className="settings-feedback-title">SSE 连接状态</div>
          <div className="settings-feedback-row">
            <span className="ops-dot-ring sm" aria-hidden="true">
              <span className="ops-dot ok" />
            </span>
            <span className="settings-feedback-key">状态：已连接</span>
          </div>
          <div className="settings-feedback-line">回放窗口：5分钟</div>
          <div className="settings-feedback-line">Last-Event-ID：evt_1024</div>
          <div className="settings-feedback-line">最近 reset：无</div>
        </SettingsFeedbackBubble>
      </div>
    </div>
  );
}

function ToneComparisonDemo() {
  return (
    <div
      style={{
        minHeight: 280,
        display: "grid",
        gap: 28,
        padding: 40,
        alignContent: "start",
      }}
    >
      <InlineFeedbackBubbleDemo message="已发送" tone="success" />
      <InlineFeedbackBubbleDemo message="HTTP 404" tone="error" />
    </div>
  );
}

const meta = {
  title: "Components/SettingsFeedbackBubble",
  component: SettingsFeedbackBubble,
  tags: ["autodocs"],
  args: {
    message: "已发送",
    onClose: () => {},
    placement: "top",
    tone: "success",
  },
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "统一的反馈气泡组件：success / error / neutral 共享同一套结构、箭头、动画与定位行为，仅通过 tone token 和可选内容布局区分语义。inline 模式使用 Floating UI 做锚定，非 inline 模式用于字段错误泡泡。",
      },
    },
  },
  argTypes: {
    anchorRef: { control: false, table: { disable: true } },
    inline: { control: false, table: { disable: true } },
    onClose: { control: false, table: { disable: true } },
    testId: { control: false, table: { disable: true } },
    tone: {
      control: "inline-radio",
      options: ["success", "error", "neutral"],
    },
    placement: {
      control: "inline-radio",
      options: ["top", "bottom", "left", "right"],
    },
  },
} satisfies Meta<typeof SettingsFeedbackBubble>;

export default meta;
type Story = StoryObj<typeof meta>;

export const InlineSuccess: Story = {
  args: {
    message: "已发送",
    placement: "top",
    tone: "success",
  },
  render: (args) => (
    <InlineFeedbackBubbleDemo
      message={args.message}
      placement={args.placement as InlineDemoProps["placement"]}
      tone={args.tone}
    />
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const bubble = await canvas.findByTestId("settings-feedback-bubble-story");
    expect(bubble).toHaveAttribute("role", "status");
    expect(bubble).toHaveTextContent("已发送");
  },
};

export const InlineError: Story = {
  args: {
    message: "HTTP 404",
    placement: "top",
    tone: "error",
  },
  render: (args) => (
    <InlineFeedbackBubbleDemo
      message={args.message}
      placement={args.placement as InlineDemoProps["placement"]}
      tone={args.tone}
    />
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const bubble = await canvas.findByTestId("settings-feedback-bubble-story");
    expect(bubble).toHaveAttribute("role", "alert");
    expect(bubble).toHaveTextContent("HTTP 404");
  },
};

export const StaticFieldError: Story = {
  args: {
    message: "请输入有效地址",
    tone: "error",
  },
  render: (args) => <StaticFeedbackBubbleDemo message={args.message} tone={args.tone} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const bubble = await canvas.findByTestId("settings-feedback-bubble-static-story");
    await waitFor(() => {
      expect(bubble).toBeVisible();
    });
    expect(bubble).toHaveAttribute("role", "alert");
    expect(bubble).toHaveTextContent("请输入有效地址");
  },
};

export const StaticFieldErrorDismissible: Story = {
  args: {
    message: "请输入有效地址",
    tone: "error",
  },
  render: (args) => <StaticFeedbackBubbleDemo message={args.message} tone={args.tone} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const bubble = await canvas.findByTestId("settings-feedback-bubble-static-story");
    await userEvent.click(within(bubble).getByRole("button", { name: "关闭提示" }));
    await waitFor(() => {
      expect(canvas.queryByTestId("settings-feedback-bubble-static-story")).toBeNull();
    });
  },
};

export const NeutralTooltip: Story = {
  args: {
    message: null,
    tone: "neutral",
  },
  render: () => <NeutralTooltipDemo />,
};

export const ToneComparison: Story = {
  render: () => <ToneComparisonDemo />,
};
