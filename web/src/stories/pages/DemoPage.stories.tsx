import type { Meta, StoryObj } from "@storybook/react";

function DemoPage() {
  return (
    <div className="panel" style={{ padding: 24 }}>
      <div className="panel-section">
        <div className="panel-title">Demo page</div>
        <div className="panel-subtitle">
          Storybook 里不跑真实 API：页面级组件用 stub 展示结构与状态。
        </div>
        <div className="divider" />
        <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
          <span className="pill">正常</span>
          <span className="pill on">已开启</span>
          <span className="pill warn">警告</span>
          <span className="pill err">错误</span>
          <span className="pill disabled">禁用</span>
        </div>
      </div>
    </div>
  );
}

const meta = {
  title: "Pages/DemoPage",
  component: DemoPage,
} satisfies Meta<typeof DemoPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
