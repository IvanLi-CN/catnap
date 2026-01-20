import type { Meta, StoryObj } from "@storybook/react";
import { expect } from "storybook/test";

function ThemeTokensDemo() {
  return (
    <div className="panel" style={{ padding: 24 }}>
      <div className="panel-section">
        <div className="panel-title">Theme tokens</div>
        <div className="panel-subtitle">
          <code>system / dark / light</code> 通过 <code>{"<html data-theme>"}</code> 驱动
        </div>
        <div className="divider" />
        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
          }}
        >
          <Swatch name="bg" varName="--color-bg" />
          <Swatch name="surface-1" varName="--color-surface-1" />
          <Swatch name="surface-2" varName="--color-surface-2" />
          <Swatch name="border" varName="--color-border" />
          <Swatch name="accent" varName="--color-accent" />
          <Swatch name="danger" varName="--color-danger" />
        </div>
      </div>
    </div>
  );
}

function Swatch({ name, varName }: { name: string; varName: string }) {
  return (
    <div
      className="panel-section"
      style={{
        padding: 16,
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <div style={{ fontWeight: 800 }}>{name}</div>
      <div
        style={{
          height: 44,
          borderRadius: 10,
          border: "1px solid var(--line)",
          background: `var(${varName})`,
        }}
      />
      <div className="mono">{varName}</div>
    </div>
  );
}

const meta = {
  title: "Foundations/Theme",
  component: ThemeTokensDemo,
} satisfies Meta<typeof ThemeTokensDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const System: Story = {
  globals: { theme: "system" },
  play: async () => {
    expect(document.documentElement.getAttribute("data-theme")).toBeNull();
  },
};

export const Dark: Story = {
  globals: { theme: "dark" },
  play: async () => {
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  },
};

export const Light: Story = {
  globals: { theme: "light" },
  play: async () => {
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  },
};
