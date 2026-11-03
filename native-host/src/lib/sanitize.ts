import DOMPurify from "dompurify";

export const FORBIDDEN_TAGS = [
  "style",
  "form",
  "animate",
  "animateMotion",
  "animateTransform",
  "set",
  "discard",
];

// Ensure all new-window links receive rel="noopener noreferrer" when addHook is available
if (typeof (DOMPurify as any)?.addHook === "function") {
  (DOMPurify as any).addHook("afterSanitizeAttributes", (node: Element) => {
    if (node.tagName === "A" && (node.getAttribute("target") === "_blank" || node.hasAttribute("target"))) {
      node.setAttribute("rel", "noopener noreferrer");
    }
  });
}

/**
 * Sanitizes HTML strings for safe rendering in notes and previews.
 * Strips style, form, and SVG animation tags, and ensures new-window links
 * have rel="noopener noreferrer".
 */
export function sanitizeHtml(html: string): string {
  const sanitizeFn: (source: string, config?: any) => string =
    typeof (DOMPurify as any)?.sanitize === "function"
      ? (s, cfg) => (DOMPurify as any).sanitize(s, cfg)
      : (s) => s;
  const clean = sanitizeFn(html, {
    FORBID_TAGS: FORBIDDEN_TAGS,
    ADD_ATTR: ["target", "rel"],
  });
  if (typeof document === "undefined") return clean;
  const template = document.createElement("template");
  template.innerHTML = clean;
  template.content.querySelectorAll("a").forEach((a) => {
    if (a.getAttribute("target") === "_blank" || a.hasAttribute("target")) {
      a.setAttribute("rel", "noopener noreferrer");
    }
  });
  template.content.querySelectorAll("style, form, animate, animateMotion, animateTransform, set, discard").forEach((el) => {
    el.remove();
  });
  return template.innerHTML;
}
