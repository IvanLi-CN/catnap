import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { expect, userEvent, within } from "storybook/test";
import { type Config, ProductCard } from "../../App";
import { countriesById, demoConfigs } from "../fixtures";

const demoCountriesById = countriesById();

const unmappedCountryConfig: Config = {
  ...demoConfigs[1],
  id: "cfg-unmapped-country",
  countryId: "unmapped",
  regionId: null,
};

const cloudWithOrderConfig: Config = {
  ...demoConfigs[0],
  id: "cfg-cloud-with-order",
  sourcePid: "117",
};

function ProductCardDemo({ initial }: { initial: Config }) {
  const [cfg, setCfg] = useState<Config>(initial);
  const orderLink = cfg.sourcePid
    ? {
        url: `https://lxc.lazycat.wiki/cart?action=configureproduct&pid=${cfg.sourcePid}`,
        mode: "configureproduct" as const,
      }
    : null;
  return (
    <div style={{ padding: 24 }}>
      <ProductCard
        cfg={cfg}
        countriesById={demoCountriesById}
        orderLink={orderLink}
        onToggle={(configId, enabled) => {
          setCfg((prev) => (prev.id === configId ? { ...prev, monitorEnabled: enabled } : prev));
        }}
      />
    </div>
  );
}

const meta = {
  title: "Components/ProductCard",
  component: ProductCardDemo,
} satisfies Meta<typeof ProductCardDemo>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Cloud: Story = {
  args: { initial: demoConfigs[0] },
};

export const MonitorOn: Story = {
  args: { initial: demoConfigs[1] },
};

export const MonitorOff: Story = {
  args: { initial: demoConfigs[2] },
};

export const UnmappedCountry: Story = {
  args: { initial: unmappedCountryConfig },
};

export const MissingOrderLink: Story = {
  args: { initial: demoConfigs[7] },
};

export const KeyboardOpensOrder: Story = {
  args: { initial: demoConfigs[1] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const openCalls: string[] = [];
    const originalOpen = window.open;
    window.open = ((url?: string | URL) => {
      openCalls.push(String(url));
      return null;
    }) as typeof window.open;

    try {
      const cardLink = await canvas.findByRole("link", { name: "打开下单页（新标签页）" });
      cardLink.focus();
      await userEvent.keyboard("{Enter}");
      expect(openCalls.length).toBe(1);
      expect(openCalls[0]).toContain("action=configureproduct");
      expect(openCalls[0]).toContain("pid=128");
    } finally {
      window.open = originalOpen;
    }
  },
};

export const ToggleButtonDoesNotOpenOrder: Story = {
  args: { initial: demoConfigs[1] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const openCalls: string[] = [];
    const originalOpen = window.open;
    window.open = ((url?: string | URL) => {
      openCalls.push(String(url));
      return null;
    }) as typeof window.open;

    try {
      const toggle = await canvas.findByRole("button", { name: "监控：开" });
      await userEvent.click(toggle);
      expect(openCalls.length).toBe(0);
    } finally {
      window.open = originalOpen;
    }
  },
};

export const MissingOrderLinkIsNotFocusable: Story = {
  args: { initial: demoConfigs[7] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    expect(canvas.queryByRole("link")).toBeNull();
    expect(canvas.queryByRole("button", { name: "打开下单页（新标签页）" })).toBeNull();
    expect(await canvas.findByText("暂无下单链接")).toBeTruthy();
  },
};

export const CloudBadgeAreaOpensOrder: Story = {
  args: { initial: cloudWithOrderConfig },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const openCalls: string[] = [];
    const originalOpen = window.open;
    window.open = ((url?: string | URL) => {
      openCalls.push(String(url));
      return null;
    }) as typeof window.open;

    try {
      expect(canvas.queryByRole("button", { name: "监控：禁用" })).toBeNull();
      await userEvent.click(await canvas.findByRole("link", { name: "打开下单页（新标签页）" }));
      expect(openCalls.length).toBe(1);
      expect(openCalls[0]).toContain("pid=117");
    } finally {
      window.open = originalOpen;
    }
  },
};
