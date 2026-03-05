import type { Meta, StoryObj } from "@storybook/react";
import { RESPONSIVE_BREAKPOINTS } from "../breakpoints";

function ViewportBreakpoints() {
  return (
    <div className="panel" style={{ padding: 24 }}>
      <div className="panel-section">
        <div className="panel-title">响应式断点合同</div>
        <div className="panel-subtitle">
          支持范围：360px - 1680px（大于 1680px 复用 1680px 版式约束）
        </div>
        <div className="divider" />
        <div style={{ display: "grid", gap: 10 }}>
          {RESPONSIVE_BREAKPOINTS.map((bp) => (
            <div
              key={bp.id}
              className="pill"
              style={{ height: 40, justifyContent: "space-between", padding: "0 14px" }}
            >
              <span>{bp.label}</span>
              <span className="mono">{`${bp.width} x ${bp.height} (${bp.range})`}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

const meta = {
  title: "Foundations/ViewportBreakpoints",
  component: ViewportBreakpoints,
} satisfies Meta<typeof ViewportBreakpoints>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
