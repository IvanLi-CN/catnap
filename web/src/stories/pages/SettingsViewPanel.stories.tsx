import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { type AboutResponse, type BootstrapResponse, SettingsViewPanel } from "../../App";
import { demoBootstrap } from "../fixtures";
import { ResponsivePageStory, expectResponsiveBreakpoints } from "./responsivePageHelpers";

type DemoProps = {
  about: AboutResponse | null;
};

const aboutOk: AboutResponse = {
  version: "0.1.0",
  webDistBuildId: "demo-webdist-abcdef",
  repoUrl: "https://github.com/IvanLi-CN/catnap",
  update: {
    enabled: true,
    status: "ok",
    checkedAt: "2026-02-17T00:00:00Z",
    latestVersion: "0.1.0",
    latestUrl: "https://github.com/IvanLi-CN/catnap/releases/tag/v0.1.0",
    updateAvailable: false,
  },
};

const aboutUpdateAvailable: AboutResponse = {
  ...aboutOk,
  update: {
    enabled: true,
    status: "ok",
    checkedAt: "2026-02-17T00:00:00Z",
    latestVersion: "0.1.9",
    latestUrl: "https://github.com/IvanLi-CN/catnap/releases/tag/v0.1.9",
    updateAvailable: true,
  },
};

function SettingsViewPanelDemo({ about }: DemoProps) {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(demoBootstrap);

  return (
    <SettingsViewPanel
      bootstrap={bootstrap}
      about={about}
      aboutLoading={false}
      aboutError={null}
      onCheckUpdate={async () => {}}
      onSave={async (next) => {
        const { telegramBotToken: _telegramBotToken, ...settings } = next;
        setBootstrap((prev) => ({ ...prev, settings: { ...prev.settings, ...settings } }));
        return settings;
      }}
    />
  );
}

const meta = {
  title: "Pages/SettingsViewPanel",
  component: SettingsViewPanelDemo,
} satisfies Meta<typeof SettingsViewPanelDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { about: null },
};

export const AboutOk: Story = {
  args: { about: aboutOk },
};

export const UpdateAvailable: Story = {
  args: { about: aboutUpdateAvailable },
};

export const ResponsiveAllBreakpoints: Story = {
  args: { about: aboutUpdateAvailable },
  render: (args) => (
    <ResponsivePageStory
      route="settings"
      title="Catnap • 系统设置"
      subtitle="使用顶部 Viewport 选择断点进行验收"
      actions={<span className="pill sm">主题切换</span>}
      renderPage={() => <SettingsViewPanelDemo about={args.about ?? null} />}
    />
  ),
  play: async ({ canvasElement }) => {
    await expectResponsiveBreakpoints(canvasElement, "page-settings");
  },
};
