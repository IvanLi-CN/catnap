import {
  type Placement,
  arrow,
  autoUpdate,
  flip,
  offset,
  shift,
  size,
  useFloating,
} from "@floating-ui/react";
import {
  type AriaRole,
  type CSSProperties,
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

export type SettingsFeedbackTone = "error" | "success" | "neutral";
export const SETTINGS_FEEDBACK_BUBBLE_ANIMATION_MS = 180;

type SettingsFeedbackBubbleProps = {
  anchorRef?: RefObject<HTMLElement | null>;
  children?: ReactNode;
  dismissible?: boolean;
  inline?: boolean;
  message: string | null;
  onClose?: () => void;
  open?: boolean;
  placement?: Placement;
  role?: AriaRole;
  showIcon?: boolean;
  testId?: string;
  tone: SettingsFeedbackTone;
};

type BubbleRenderState = "hidden" | "entering" | "open" | "closing";

function SettingsFeedbackIcon({ tone }: { tone: SettingsFeedbackTone }) {
  if (tone === "success") {
    return (
      <svg aria-hidden="true" viewBox="0 0 16 16">
        <path
          d="M3.75 8.5 6.75 11.5 12.25 5"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="1.9"
        />
      </svg>
    );
  }

  if (tone === "neutral") {
    return (
      <svg aria-hidden="true" viewBox="0 0 16 16">
        <circle cx="8" cy="8" fill="none" r="5.4" stroke="currentColor" strokeWidth="1.6" />
        <path
          d="M8 6.2v2.7"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeWidth="1.8"
        />
        <circle cx="8" cy="10.95" fill="currentColor" r="1.05" />
      </svg>
    );
  }

  return (
    <svg aria-hidden="true" viewBox="0 0 16 16">
      <path
        d="M8 3.75v4.75"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.9"
      />
      <circle cx="8" cy="11.55" fill="currentColor" r="1.1" />
    </svg>
  );
}

function fallbackPlacementsFor(placement: Placement): Placement[] {
  const [side, align] = placement.split("-") as [string, string | undefined];
  const withAlign = (next: "top" | "bottom" | "left" | "right"): Placement =>
    (align ? `${next}-${align}` : next) as Placement;

  switch (side) {
    case "top":
      return [withAlign("bottom"), withAlign("right"), withAlign("left")];
    case "bottom":
      return [withAlign("top"), withAlign("right"), withAlign("left")];
    case "left":
      return [withAlign("right"), withAlign("top"), withAlign("bottom")];
    case "right":
      return [withAlign("left"), withAlign("top"), withAlign("bottom")];
    default:
      return [withAlign("bottom"), withAlign("right"), withAlign("left")];
  }
}

function clearBubbleTimer(timerRef: { current: number | null }) {
  if (timerRef.current !== null) {
    window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }
}

function clearBubbleFrame(frameRef: { current: number | null }) {
  if (frameRef.current !== null) {
    window.cancelAnimationFrame(frameRef.current);
    frameRef.current = null;
  }
}

export function SettingsFeedbackBubble({
  anchorRef,
  children,
  dismissible = true,
  inline = false,
  message,
  onClose,
  open = true,
  placement = "top",
  role,
  showIcon = true,
  testId,
  tone,
}: SettingsFeedbackBubbleProps) {
  const arrowRef = useRef<HTMLSpanElement | null>(null);
  const innerRef = useRef<HTMLDivElement | null>(null);
  const textRef = useRef<HTMLSpanElement | null>(null);
  const exitTimerRef = useRef<number | null>(null);
  const enterFrameRef = useRef<number | null>(null);
  const [contentMinHeight, setContentMinHeight] = useState<number>(40);
  const [isMultiline, setIsMultiline] = useState(false);
  const [renderState, setRenderState] = useState<BubbleRenderState>(
    open && (message || children) ? "entering" : "hidden",
  );
  const [renderedMessage, setRenderedMessage] = useState<string | null>(message);
  const [renderedChildren, setRenderedChildren] = useState<ReactNode>(children ?? null);
  const [renderedTone, setRenderedTone] = useState(tone);
  const contentNode = children ?? message;
  const shouldShow = open && Boolean(contentNode);
  const floatingOpen = inline && renderState !== "hidden";
  const {
    floatingStyles,
    isPositioned,
    middlewareData,
    placement: resolvedPlacement,
    refs,
  } = useFloating({
    open: floatingOpen,
    placement,
    strategy: "fixed",
    whileElementsMounted: floatingOpen ? autoUpdate : undefined,
    middleware: floatingOpen
      ? [
          offset(10),
          flip({ fallbackPlacements: fallbackPlacementsFor(placement), padding: 16 }),
          shift({ padding: 16 }),
          size({
            padding: 16,
            apply({ availableWidth, elements }) {
              elements.floating.style.maxWidth = `${Math.max(0, Math.min(420, availableWidth))}px`;
            },
          }),
          arrow({ element: arrowRef, padding: 14 }),
        ]
      : [],
  });

  useEffect(() => {
    clearBubbleTimer(exitTimerRef);
    clearBubbleFrame(enterFrameRef);

    if (shouldShow) {
      setRenderedMessage(message);
      setRenderedChildren(children ?? null);
      setRenderedTone(tone);
      setRenderState((current) => {
        if (current === "hidden") {
          enterFrameRef.current = window.requestAnimationFrame(() => {
            setRenderState("open");
            enterFrameRef.current = null;
          });
          return "entering";
        }
        return "open";
      });
      return;
    }

    setRenderState((current) => {
      if (current === "hidden") return current;
      exitTimerRef.current = window.setTimeout(() => {
        setRenderState("hidden");
        exitTimerRef.current = null;
      }, SETTINGS_FEEDBACK_BUBBLE_ANIMATION_MS);
      return "closing";
    });
  }, [children, message, shouldShow, tone]);

  useEffect(() => {
    return () => {
      clearBubbleTimer(exitTimerRef);
      clearBubbleFrame(enterFrameRef);
    };
  }, []);

  useLayoutEffect(() => {
    const textEl = textRef.current;
    if (!textEl || renderState === "hidden" || renderedChildren) return;

    const update = () => {
      const lineHeight = Number.parseFloat(window.getComputedStyle(textEl).lineHeight);
      if (Number.isFinite(lineHeight) && lineHeight > 0) {
        setIsMultiline(textEl.getBoundingClientRect().height > lineHeight * 1.45);
        return;
      }
      setIsMultiline(textEl.scrollHeight - textEl.clientHeight > 1);
    };

    update();
    if (typeof ResizeObserver !== "undefined") {
      const observer = new ResizeObserver(update);
      observer.observe(textEl);
      return () => observer.disconnect();
    }

    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [renderState, renderedChildren]);

  useLayoutEffect(() => {
    if (!floatingOpen) return;
    refs.setReference(anchorRef?.current ?? null);
  }, [anchorRef, floatingOpen, refs]);

  useLayoutEffect(() => {
    const innerEl = innerRef.current;
    if (!innerEl || renderState === "hidden") return;

    const update = () => {
      const nextHeight = Math.max(40, Math.ceil(innerEl.getBoundingClientRect().height + 20));
      setContentMinHeight(nextHeight);
    };

    update();
    if (typeof ResizeObserver !== "undefined") {
      const observer = new ResizeObserver(update);
      observer.observe(innerEl);
      return () => observer.disconnect();
    }

    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [renderState]);

  if (renderState === "hidden" || (!renderedChildren && !renderedMessage)) return null;

  const inlineSide = inline ? resolvedPlacement.split("-")[0] : null;
  const bubbleStyle = inline
    ? ({
        ...floatingStyles,
        minHeight: `${contentMinHeight}px`,
        visibility: isPositioned ? "visible" : "hidden",
      } as CSSProperties)
    : ({
        minHeight: `${contentMinHeight}px`,
      } as CSSProperties);
  const arrowStyle =
    inline && inlineSide
      ? ({
          ...(middlewareData.arrow?.x != null ? { left: `${middlewareData.arrow.x}px` } : {}),
          ...(middlewareData.arrow?.y != null ? { top: `${middlewareData.arrow.y}px` } : {}),
          ...(inlineSide === "left" ? { right: "-8px" } : {}),
          ...(inlineSide === "right" ? { left: "-8px" } : {}),
          ...(inlineSide === "top" ? { bottom: "-8px" } : {}),
          ...(inlineSide === "bottom" ? { top: "-8px" } : {}),
        } as CSSProperties)
      : undefined;
  const resolvedRole =
    role ??
    (renderedTone === "success" ? "status" : renderedTone === "neutral" ? "tooltip" : "alert");
  const isSuccess = resolvedRole === "status";

  return (
    <div
      aria-live={isSuccess ? "polite" : undefined}
      className={`settings-feedback-bubble settings-feedback-bubble-${renderedTone} ${
        inline && inlineSide
          ? `settings-feedback-bubble-inline settings-feedback-bubble-inline-side-${inlineSide}`
          : ""
      } ${isMultiline ? "is-multiline" : "is-singleline"}`}
      data-state={renderState === "closing" ? "closing" : "open"}
      data-testid={testId}
      ref={inline ? refs.setFloating : undefined}
      role={resolvedRole}
      style={bubbleStyle}
    >
      <div className="settings-feedback-inner" ref={innerRef}>
        {showIcon ? (
          <span className={`settings-feedback-badge settings-feedback-badge-${renderedTone}`}>
            <SettingsFeedbackIcon tone={renderedTone} />
          </span>
        ) : null}
        {renderedChildren ? (
          <div className="settings-feedback-content">{renderedChildren}</div>
        ) : (
          <span className="settings-feedback-text" ref={textRef}>
            {renderedMessage}
          </span>
        )}
        {dismissible ? (
          <button
            aria-label="关闭提示"
            className="settings-feedback-close"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        ) : null}
      </div>
      {inline && inlineSide ? (
        <span
          aria-hidden="true"
          className={`settings-feedback-arrow settings-feedback-arrow-${inlineSide}`}
          ref={arrowRef}
          style={arrowStyle}
        />
      ) : null}
    </div>
  );
}
