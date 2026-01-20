import type { Meta, StoryObj } from "@storybook/react";
import { AppShell } from "../../ui/layout/AppShell";

function AppShellDemo() {
  return (
    <AppShell
      title="Catnap • Layout"
      subtitle="A minimal shell for pages and components"
      actions={<span className="pill">Right area</span>}
      sidebar={
        <>
          <div className="sidebar-title">导航</div>
          <div className="nav-item active">当前</div>
          <div className="nav-item">其它</div>
        </>
      }
    >
      <div className="panel">
        <div className="panel-section">
          <div className="panel-title">Content</div>
          <div className="panel-subtitle">Use the shared tokens and layout primitives.</div>
        </div>
      </div>
    </AppShell>
  );
}

const meta = {
  title: "Layout/AppShell",
  component: AppShellDemo,
} satisfies Meta<typeof AppShellDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
