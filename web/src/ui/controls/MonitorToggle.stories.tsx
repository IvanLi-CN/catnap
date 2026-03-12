import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { expect, userEvent, within } from "storybook/test";
import { MonitorToggle, type MonitorToggleState } from "./MonitorToggle";

function MonitorToggleDemo({ initialState }: { initialState: MonitorToggleState }) {
  const [state, setState] = useState<MonitorToggleState>(initialState);

  return (
    <div
      style={{
        minHeight: 220,
        display: "grid",
        placeItems: "center",
        padding: 40,
      }}
    >
      <div style={{ display: "grid", gap: 16, justifyItems: "start" }}>
        <div id="monitor-toggle-demo-heading" className="panel-title">
          东京
        </div>
        <MonitorToggle
          labelledBy="monitor-toggle-demo-heading"
          onClick={() => {
            if (state === "disabled") return;
            setState((prev) => (prev === "on" ? "off" : "on"));
          }}
          state={state}
          testId="monitor-toggle-story"
        />
      </div>
    </div>
  );
}

const meta = {
  title: "Components/MonitorToggle",
  component: MonitorToggle,
  tags: ["autodocs"],
  parameters: {
    layout: "padded",
    docs: {
      description: {
        component:
          "统一的监控开关组件。文案固定为监控状态，具体作用域由所在上下文表达；支持通用 on/off/disabled 三种状态。",
      },
    },
  },
  argTypes: {
    labelledBy: { control: false, table: { disable: true } },
    testId: { control: false, table: { disable: true } },
    onClick: { control: false, table: { disable: true } },
    onKeyDown: { control: false, table: { disable: true } },
    className: { control: false, table: { disable: true } },
    state: {
      control: "inline-radio",
      options: ["off", "on", "disabled"],
    },
  },
} satisfies Meta<typeof MonitorToggle>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Off: Story = {
  args: { state: "off" },
  render: () => <MonitorToggleDemo initialState="off" />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByTestId("monitor-toggle-story");
    expect(toggle).toHaveTextContent("监控：关");
    await userEvent.click(toggle);
    expect(toggle).toHaveTextContent("监控：开");
  },
};

export const On: Story = {
  args: { state: "on" },
  render: () => <MonitorToggleDemo initialState="on" />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByTestId("monitor-toggle-story");
    expect(toggle).toHaveTextContent("监控：开");
    await userEvent.click(toggle);
    expect(toggle).toHaveTextContent("监控：关");
  },
};

export const Disabled: Story = {
  args: { state: "disabled" },
  render: () => <MonitorToggleDemo initialState="disabled" />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByTestId("monitor-toggle-story");
    expect(toggle).toHaveTextContent("监控：禁用");
    expect(toggle).toBeDisabled();
  },
};
