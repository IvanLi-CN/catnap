import type { Decorator, Preview } from "@storybook/react";
import { applyThemeMode, coerceThemeMode, saveThemeMode } from "../src/app/theme";
import "../src/app.css";

const withTheme: Decorator = (Story, context) => {
  const mode = coerceThemeMode(context.globals.theme);
  saveThemeMode(mode);
  applyThemeMode(mode);
  return <Story />;
};

const preview: Preview = {
  globalTypes: {
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
