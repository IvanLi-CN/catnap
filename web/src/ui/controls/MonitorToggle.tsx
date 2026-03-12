import { type KeyboardEventHandler, type MouseEventHandler, useId } from "react";

export type MonitorToggleState = "on" | "off" | "disabled";

export type MonitorToggleProps = {
  state: MonitorToggleState;
  labelledBy?: string;
  testId?: string;
  className?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  onKeyDown?: KeyboardEventHandler<HTMLButtonElement>;
};

function monitorLabel(state: MonitorToggleState) {
  if (state === "disabled") return "监控：禁用";
  return state === "on" ? "监控：开" : "监控：关";
}

export function MonitorToggle({
  state,
  labelledBy,
  testId,
  className = "",
  onClick,
  onKeyDown,
}: MonitorToggleProps) {
  const labelId = useId();
  const label = monitorLabel(state);
  const disabled = state === "disabled";
  const classNames = [
    "pill",
    "badge",
    "monitor-toggle",
    state === "on" ? "on" : "",
    disabled ? "disabled" : "",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button
      type="button"
      aria-label={labelledBy ? undefined : label}
      aria-labelledby={labelledBy ? `${labelledBy} ${labelId}` : undefined}
      aria-pressed={disabled ? undefined : state === "on"}
      className={classNames}
      data-testid={testId}
      disabled={disabled}
      onClick={onClick}
      onKeyDown={onKeyDown}
    >
      <span className="monitor-toggle-dot" aria-hidden="true" />
      <span id={labelId}>{label}</span>
    </button>
  );
}
