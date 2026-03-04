import type { Decorator, Preview } from "@storybook/react";
import { applyThemeMode, coerceThemeMode, saveThemeMode } from "../src/app/theme";
import { RESPONSIVE_BREAKPOINTS, RESPONSIVE_BREAKPOINTS_BY_ID } from "../src/stories/breakpoints";
import "../src/app.css";

const CONTAINER_STYLE = {
  containerType: "inline-size" as const,
  containerName: "app-shell",
};

const withTheme: Decorator = (Story, context) => {
  const mode = coerceThemeMode(context.globals.theme);
  saveThemeMode(mode);
  applyThemeMode(mode);
  const viewportPreset = String(context.globals.viewportPreset ?? "auto");
  if (viewportPreset === "auto") {
    return (
      <div style={{ ...CONTAINER_STYLE, minHeight: "100vh" }}>
        <Story />
      </div>
    );
  }

  const selected = RESPONSIVE_BREAKPOINTS_BY_ID.get(viewportPreset);
  if (!selected) {
    return (
      <div style={{ ...CONTAINER_STYLE, minHeight: "100vh" }}>
        <Story />
      </div>
    );
  }

  return (
    <div style={{ padding: 12, background: "var(--bg)", minHeight: "100vh" }}>
      <div style={{ marginBottom: 10, fontSize: 12, fontWeight: 700, color: "var(--muted)" }}>
        {`${selected.label} • ${selected.width}x${selected.height}`}
      </div>
      <div
        style={{
          width: selected.width,
          height: selected.height,
          maxWidth: "100%",
          margin: "0 auto",
          overflow: "auto",
          borderRadius: 16,
          border: "1px solid var(--line)",
          ...CONTAINER_STYLE,
        }}
      >
        <Story />
      </div>
    </div>
  );
};

const preview: Preview = {
  globalTypes: {
    viewportPreset: {
      name: "Viewport",
      description: "Responsive breakpoints for layout checks",
      defaultValue: "auto",
      toolbar: {
        icon: "mirror",
        items: [
          { value: "auto", title: "Auto (no clamp)" },
          ...RESPONSIVE_BREAKPOINTS.map((item) => ({
            value: item.id,
            title: `${item.width}px (${item.range})`,
          })),
        ],
        dynamicTitle: true,
      },
    },
    theme: {
      name: "Theme",
      description: "Theme mode (system/dark/light)",
      defaultValue: "system",
      toolbar: {
        icon: "circlehollow",
        items: [
          { value: "system", title: "System" },
          { value: "dark", title: "Dark" },
          { value: "light", title: "Light" },
        ],
        dynamicTitle: true,
      },
    },
  },
  decorators: [withTheme],
};

export default preview;
