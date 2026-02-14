import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { type BootstrapResponse, SettingsViewPanel } from "../../App";
import { demoBootstrap } from "../fixtures";

function SettingsViewPanelDemo() {
  const [bootstrap, setBootstrap] = useState<BootstrapResponse>(demoBootstrap);

  return (
    <SettingsViewPanel
      bootstrap={bootstrap}
      onSave={async (next) => {
        const { telegramBotToken: _telegramBotToken, ...settings } = next;
        setBootstrap((prev) => ({ ...prev, settings: { ...prev.settings, ...settings } }));
      }}
      fetchUpdate={async () => ({
        currentVersion: bootstrap.app.effectiveVersion,
        updateAvailable: false,
        checkedAt: new Date().toISOString(),
      })}
    />
  );
}

const meta = {
  title: "Pages/SettingsViewPanel",
  component: SettingsViewPanelDemo,
} satisfies Meta<typeof SettingsViewPanelDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
