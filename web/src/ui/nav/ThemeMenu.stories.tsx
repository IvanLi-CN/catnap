import type { Meta, StoryObj } from "@storybook/react";
import { expect, userEvent, within } from "storybook/test";
import { ThemeMenu } from "./ThemeMenu";

const meta = {
  title: "Components/ThemeMenu",
  component: ThemeMenu,
  globals: { theme: "system" },
  render: () => (
    <div style={{ padding: 24 }}>
      <ThemeMenu />
    </div>
  ),
} satisfies Meta<typeof ThemeMenu>;

export default meta;
type Story = StoryObj<typeof meta>;

export const SwitchTheme: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const select = canvas.getByLabelText("Theme mode");

    await userEvent.selectOptions(select, "light");
    expect(localStorage.getItem("catnap.theme")).toBe('"light"');
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(document.documentElement.style.colorScheme).toBe("light");

    await userEvent.selectOptions(select, "dark");
    expect(localStorage.getItem("catnap.theme")).toBe('"dark"');
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");

    await userEvent.selectOptions(select, "system");
    expect(localStorage.getItem("catnap.theme")).toBe('"system"');
    expect(document.documentElement.getAttribute("data-theme")).toBeNull();
    expect(document.documentElement.style.colorScheme).toBe("light dark");
  },
};
