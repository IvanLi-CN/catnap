import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";
import {
  type AboutResponse,
  type BootstrapResponse,
  SETTINGS_TEST_SUCCESS_BUBBLE_MS,
  SettingsViewPanel,
} from "../../App";
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

function ensureWebPushEnvironment() {
  const grantedNotification = {
    permission: "granted",
    requestPermission: async () => "granted" as NotificationPermission,
  };
  Object.defineProperty(window, "Notification", {
    configurable: true,
    value: grantedNotification,
  });
  Object.defineProperty(window, "PushManager", {
    configurable: true,
    value: function PushManager() {},
  });
  Object.defineProperty(navigator, "serviceWorker", {
    configurable: true,
    value: {
      register: async () => ({}),
      ready: Promise.resolve({
        pushManager: {
          getSubscription: async () => ({
            toJSON: () => ({
              endpoint: "https://push.example.com/subscriptions/demo",
              keys: {
                p256dh: "demo-p256dh",
                auth: "demo-auth",
              },
            }),
          }),
        },
      }),
    },
  });
}

function SettingsViewPanelWebPushDemo({ about }: DemoProps) {
  ensureWebPushEnvironment();
  return <SettingsViewPanelDemo about={about} />;
}

function jsonOk(body: unknown = { ok: true }) {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function installFetchMock(
  resolver: (url: string, init?: RequestInit) => Response | Promise<Response>,
) {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (input, init) => {
    const url =
      input instanceof Request
        ? input.url
        : input instanceof URL
          ? input.toString()
          : String(input);
    return resolver(url, init);
  }) as typeof fetch;
  return () => {
    globalThis.fetch = originalFetch;
  };
}

async function expectActionFeedbackBeforeButton(
  canvasElement: HTMLElement,
  buttonName: string,
  feedbackTestId: string,
  beforeMetrics: { left: number; top: number; width: number; height: number },
) {
  const canvas = within(canvasElement);
  const button = await canvas.findByRole("button", { name: buttonName });
  const bubble = await canvas.findByTestId(feedbackTestId);
  const bubbleRect = bubble.getBoundingClientRect();
  const afterRect = button.getBoundingClientRect();
  const afterMetrics = {
    left: button.offsetLeft,
    top: button.offsetTop,
    width: button.offsetWidth,
    height: button.offsetHeight,
  };
  const bubbleClasses = bubble.className;

  expect(afterMetrics.left).toBe(beforeMetrics.left);
  expect(afterMetrics.top).toBe(beforeMetrics.top);
  expect(afterMetrics.width).toBe(beforeMetrics.width);
  expect(afterMetrics.height).toBe(beforeMetrics.height);
  expect(bubbleRect.left).toBeGreaterThanOrEqual(-1);
  expect(bubbleRect.right).toBeLessThanOrEqual(window.innerWidth + 1);
  expect(bubbleRect.height).toBeGreaterThan(28);

  const bubbleCenterY = bubbleRect.top + bubbleRect.height / 2;
  const buttonCenterY = afterRect.top + afterRect.height / 2;

  if (bubbleClasses.includes("settings-feedback-bubble-inline-side-left")) {
    expect(bubbleRect.right).toBeLessThanOrEqual(afterRect.left - 8);
    expect(Math.abs(bubbleCenterY - buttonCenterY)).toBeLessThanOrEqual(8);
    return;
  }

  if (bubbleClasses.includes("settings-feedback-bubble-inline-side-right")) {
    expect(bubbleRect.left).toBeGreaterThanOrEqual(afterRect.right + 8);
    expect(Math.abs(bubbleCenterY - buttonCenterY)).toBeLessThanOrEqual(8);
    return;
  }

  if (bubbleClasses.includes("settings-feedback-bubble-inline-side-top")) {
    expect(bubbleRect.bottom).toBeLessThanOrEqual(afterRect.top - 8);
    return;
  }

  expect(bubbleClasses.includes("settings-feedback-bubble-inline-side-bottom")).toBe(true);
  expect(bubbleRect.top).toBeGreaterThanOrEqual(afterRect.bottom + 8);
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

export const TelegramSuccessBubble: Story = {
  args: { about: aboutOk },
  play: async ({ canvasElement }) => {
    const restoreFetch = installFetchMock((url) => {
      if (url.endsWith("/api/notifications/telegram/test")) {
        return jsonOk();
      }
      throw new Error(`Unexpected fetch in TelegramSuccessBubble: ${url}`);
    });

    try {
      const canvas = within(canvasElement);
      const button = await canvas.findByRole("button", { name: "测试 Telegram" });
      const beforeMetrics = {
        left: button.offsetLeft,
        top: button.offsetTop,
        width: button.offsetWidth,
        height: button.offsetHeight,
      };
      await userEvent.click(button);
      const bubble = await canvas.findByTestId("settings-feedback-tg-test");
      await waitFor(() => {
        expect(bubble).toBeVisible();
      });
      expect(bubble).toHaveTextContent("已发送");
      await expectActionFeedbackBeforeButton(
        canvasElement as HTMLElement,
        "测试 Telegram",
        "settings-feedback-tg-test",
        beforeMetrics,
      );
    } finally {
      restoreFetch();
    }
  },
};

export const TelegramSuccessBubbleDismissible: Story = {
  args: { about: aboutOk },
  play: async ({ canvasElement }) => {
    const restoreFetch = installFetchMock((url) => {
      if (url.endsWith("/api/notifications/telegram/test")) {
        return jsonOk();
      }
      throw new Error(`Unexpected fetch in TelegramSuccessBubbleDismissible: ${url}`);
    });

    try {
      const canvas = within(canvasElement);
      await userEvent.click(await canvas.findByRole("button", { name: "测试 Telegram" }));
      const bubble = await canvas.findByTestId("settings-feedback-tg-test");
      await userEvent.click(within(bubble).getByRole("button", { name: "关闭提示" }));
      await waitFor(() => {
        expect(canvas.queryByTestId("settings-feedback-tg-test")).toBeNull();
      });
    } finally {
      restoreFetch();
    }
  },
};

export const WebPushSuccessBubble: Story = {
  args: { about: aboutOk },
  render: (args) => <SettingsViewPanelWebPushDemo about={args.about ?? null} />,
  play: async ({ canvasElement }) => {
    ensureWebPushEnvironment();
    const restoreFetch = installFetchMock((url) => {
      if (
        url.endsWith("/api/notifications/web-push/subscriptions") ||
        url.endsWith("/api/notifications/web-push/test")
      ) {
        return jsonOk();
      }
      throw new Error(`Unexpected fetch in WebPushSuccessBubble: ${url}`);
    });

    try {
      const canvas = within(canvasElement);
      const button = await canvas.findByRole("button", { name: "测试 Web Push" });
      const beforeMetrics = {
        left: button.offsetLeft,
        top: button.offsetTop,
        width: button.offsetWidth,
        height: button.offsetHeight,
      };
      await userEvent.click(button);
      const bubble = await canvas.findByTestId("settings-feedback-wp-test");
      await waitFor(() => {
        expect(bubble).toBeVisible();
      });
      expect(bubble).toHaveTextContent("已发送（如权限/订阅正常，应很快弹出通知）");
      await expectActionFeedbackBeforeButton(
        canvasElement as HTMLElement,
        "测试 Web Push",
        "settings-feedback-wp-test",
        beforeMetrics,
      );
    } finally {
      restoreFetch();
    }
  },
};

export const WebPushSuccessBubbleAutoDismiss: Story = {
  args: { about: aboutOk },
  render: (args) => <SettingsViewPanelWebPushDemo about={args.about ?? null} />,
  play: async ({ canvasElement }) => {
    ensureWebPushEnvironment();
    const restoreFetch = installFetchMock((url) => {
      if (
        url.endsWith("/api/notifications/web-push/subscriptions") ||
        url.endsWith("/api/notifications/web-push/test")
      ) {
        return jsonOk();
      }
      throw new Error(`Unexpected fetch in WebPushSuccessBubbleAutoDismiss: ${url}`);
    });

    try {
      const canvas = within(canvasElement);
      await userEvent.click(await canvas.findByRole("button", { name: "测试 Web Push" }));
      const bubble = await canvas.findByTestId("settings-feedback-wp-test");
      await waitFor(() => {
        expect(bubble).toBeVisible();
      });
      expect(bubble).toHaveTextContent("已发送（如权限/订阅正常，应很快弹出通知）");
      await waitFor(
        () => {
          expect(canvas.queryByTestId("settings-feedback-wp-test")).toBeNull();
        },
        { timeout: SETTINGS_TEST_SUCCESS_BUBBLE_MS + 2_000 },
      );
    } finally {
      restoreFetch();
    }
  },
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
