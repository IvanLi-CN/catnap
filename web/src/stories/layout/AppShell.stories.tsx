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

          <div className="sidebar-meta">
            <div className="sidebar-meta-divider" aria-hidden="true" />
            <div className="sidebar-meta-top">
              <a
                className="sidebar-meta-version"
                href="https://github.com/IvanLi-CN/catnap/releases/tag/v0.1.0"
                target="_blank"
                rel="noopener noreferrer"
                title="Version: v0.1.0"
              >
                <span className="mono">v0.1.0</span>
              </a>
              <a
                className="sidebar-meta-repo"
                href="https://github.com/IvanLi-CN/catnap"
                target="_blank"
                rel="noopener noreferrer"
                title="https://github.com/IvanLi-CN/catnap"
              >
                <span className="mono">GitHub</span>
              </a>
            </div>
          </div>
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
