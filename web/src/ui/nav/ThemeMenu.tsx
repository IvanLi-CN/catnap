import { type ChangeEvent, useCallback, useState } from "react";
import { applyThemeMode, coerceThemeMode, loadThemeMode, saveThemeMode } from "../../app/theme";

export function ThemeMenu() {
  const [mode, setMode] = useState(() => loadThemeMode());

  const onChange = useCallback((e: ChangeEvent<HTMLSelectElement>) => {
    const next = coerceThemeMode(e.target.value);
    setMode(next);
    saveThemeMode(next);
    applyThemeMode(next);
  }, []);

  return (
    <label className="pill select" title="主题偏好：system / dark / light">
      <span className="pill-prefix">主题</span>
      <select aria-label="Theme mode" value={mode} onChange={onChange}>
        <option value="system">系统</option>
        <option value="dark">深色</option>
        <option value="light">亮色</option>
      </select>
    </label>
  );
}
