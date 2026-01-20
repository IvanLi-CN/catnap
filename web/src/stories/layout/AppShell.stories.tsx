import type { Meta, StoryObj } from "@storybook/react";

function AppShellDemo() {
  return (
    <div className="app">
      <div className="shell">
        <header className="topbar">
          <div className="topbar-left">
            <div className="topbar-title">Catnap • Layout</div>
            <div className="topbar-subtitle">A minimal shell for pages and components</div>
          </div>
          <div className="topbar-right">
            <span className="pill">Right area</span>
          </div>
        </header>

        <div className="layout">
          <nav className="sidebar">
            <div className="sidebar-title">导航</div>
            <div className="nav-item active">当前</div>
            <div className="nav-item">其它</div>
          </nav>

          <main className="content">
            <div className="panel">
              <div className="panel-section">
                <div className="panel-title">Content</div>
                <div className="panel-subtitle">Use the shared tokens and layout primitives.</div>
              </div>
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}

const meta = {
  title: "Layout/AppShell",
  component: AppShellDemo,
} satisfies Meta<typeof AppShellDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
