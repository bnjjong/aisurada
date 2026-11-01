const layers: symbol[] = [];
let captured: unknown,
  hadModal = false;
export function addLayer(token: symbol) {
  layers.push(token);
  return () => {
    const i = layers.indexOf(token);
    if (i >= 0) layers.splice(i, 1);
  };
}
export const topLayer = () => layers.at(-1);
export function captureEscape(event: KeyboardEvent) {
  if (event.key === "Escape") {
    captured = event;
    hadModal = layers.length > 0;
  }
}
export const blocksEscape = (event: unknown) =>
  layers.length > 0 || (captured === event && hadModal);
