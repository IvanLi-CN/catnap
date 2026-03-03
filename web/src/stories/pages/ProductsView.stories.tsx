import type { Meta, StoryObj } from "@storybook/react";
import { useMemo, useState } from "react";
import {
  type ArchiveFilterMode,
  type BootstrapResponse,
  type Config,
  ProductsView,
} from "../../App";
import { AppShell } from "../../ui/layout/AppShell";
import { countriesById, demoBootstrap, regionsById } from "../fixtures";

function ProductsViewDemo() {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(demoBootstrap);
  const [archiveFilterMode, setArchiveFilterMode] = useState<ArchiveFilterMode>("active");
  const countries = useMemo(() => countriesById(), []);
  const regions = useMemo(() => regionsById(), []);

  return (
    <ProductsView
      bootstrap={bootstrap}
      countriesById={countries}
      regionsById={regions}
      orderBaseUrl={bootstrap.catalog.source.url}
      archiveFilterMode={archiveFilterMode}
      onArchiveFilterModeChange={setArchiveFilterMode}
      onArchiveDelisted={async () => {
        const now = new Date().toISOString();
        const archivedIds = bootstrap.catalog.configs
          .filter((cfg) => cfg.lifecycle.state === "delisted" && !cfg.lifecycle.cleanupAt)
          .map((cfg) => cfg.id);
        setBootstrap((prev) => ({
          ...prev,
          catalog: {
            ...prev.catalog,
            configs: prev.catalog.configs.map((cfg) =>
              archivedIds.includes(cfg.id)
                ? ({
                    ...cfg,
                    lifecycle: {
                      ...cfg.lifecycle,
                      cleanupAt: now,
                    },
                  } satisfies Config)
                : cfg,
            ),
          },
        }));
        return { archivedCount: archivedIds.length, archivedAt: now, archivedIds };
      }}
      onToggle={(configId, enabled) => {
        setBootstrap((prev) => {
          const nextConfigs = prev.catalog.configs.map((c) =>
            c.id === configId ? ({ ...c, monitorEnabled: enabled } satisfies Config) : c,
          );
          return {
            ...prev,
            catalog: { ...prev.catalog, configs: nextConfigs },
            monitoring: {
              ...prev.monitoring,
              enabledConfigIds: nextConfigs.filter((c) => c.monitorEnabled).map((c) => c.id),
            },
          };
        });
      }}
      onOpenOrder={() => {}}
    />
  );
}

function ProductsShell({ width }: { width: number }) {
  return (
    <div style={{ width, height: 760, borderRadius: 18, overflow: "hidden" }}>
      <AppShell
        title="Catnap • 全部产品"
        subtitle="Storybook 响应式布局预览"
        actions={<span className="pill">右侧动作</span>}
        sidebar={
          <>
            <div className="sidebar-title">导航</div>
            <div className="nav-item active">全部产品</div>
            <div className="nav-item">库存监控</div>
            <div className="nav-item">系统设置</div>
            <div className="nav-item">日志</div>
          </>
        }
      >
        <ProductsViewDemo />
      </AppShell>
    </div>
  );
}

function LabeledShell({ width }: { width: number }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div className="pill sm" style={{ width: "fit-content" }}>
        {`${width}px`}
      </div>
      <ProductsShell width={width} />
    </div>
  );
}

const meta = {
  title: "Pages/ProductsView",
  component: ProductsViewDemo,
} satisfies Meta<typeof ProductsViewDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const ResponsiveMatrix: Story = {
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 28 }}>
      <LabeledShell width={820} />
      <LabeledShell width={1180} />
      <LabeledShell width={1440} />
      <LabeledShell width={1680} />
    </div>
  ),
};

export const Narrow: Story = {
  render: () => <ProductsShell width={920} />,
};

export const Medium: Story = {
  render: () => <ProductsShell width={1180} />,
};

export const Wide: Story = {
  render: () => <ProductsShell width={1440} />,
};

export const Wider: Story = {
  render: () => <ProductsShell width={1680} />,
};
