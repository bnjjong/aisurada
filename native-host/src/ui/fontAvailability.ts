// OS에 설치된 글꼴인지 캔버스 2D 문맥의 measureText로 감지하고 결과를 캐시한다.

const fontCache = new Map<string, boolean>();

/**
 * 주어진 글꼴 이름이 현재 시스템에 설치되어 있는지 검사한다.
 * Canvas 2D 문맥을 사용해 fallback 폰트(monospace, serif)와의 너비 차이를 비교한다.
 * 환경에 document/canvas가 없으면 기본으로 true를 반환한다.
 */
export function isFontAvailable(
  name: string,
  measure?: (font: string) => number,
): boolean {
  const trimmed = name.trim();
  if (!trimmed) return true;
  if (fontCache.has(trimmed)) return fontCache.get(trimmed)!;

  let result = true;
  if (typeof measure === "function") {
    const monoBase = measure("72px monospace");
    const monoTest = measure(`72px "${trimmed}", monospace`);
    const serifBase = measure("72px serif");
    const serifTest = measure(`72px "${trimmed}", serif`);
    result = monoTest !== monoBase || serifTest !== serifBase;
  } else if (typeof document !== "undefined") {
    try {
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d");
      if (ctx) {
        const testString = "mmmmmmmmmmlli 한글가나다 0123";
        ctx.font = "72px monospace";
        const monoBase = ctx.measureText(testString).width;
        ctx.font = `72px "${trimmed}", monospace`;
        const monoTest = ctx.measureText(testString).width;

        ctx.font = "72px serif";
        const serifBase = ctx.measureText(testString).width;
        ctx.font = `72px "${trimmed}", serif`;
        const serifTest = ctx.measureText(testString).width;

        result = monoTest !== monoBase || serifTest !== serifBase;
      }
    } catch {
      result = true;
    }
  }

  fontCache.set(trimmed, result);
  return result;
}

/** 캐시를 비운다 (단위 테스트 등에서 사용) */
export function clearFontCache(): void {
  fontCache.clear();
}

/**
 * 글꼴 목록 중 사용 가능한 것만 골라낸다.
 */
export function filterAvailableFonts(
  fonts: readonly string[],
  measure?: (font: string) => number,
): string[] {
  return fonts.filter((f) => isFontAvailable(f, measure));
}
