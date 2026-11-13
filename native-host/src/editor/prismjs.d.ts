// prismjs는 타입 선언이 없어서 codeSyntax.ts가 쓰는 부분만 선언한다.
declare module "prismjs" {
  interface PrismStatic {
    /** true면 Prism이 페이지를 읽을 때 코드 블록을 스스로 칠하지 않는다. */
    manual: boolean;
    languages: Record<string, unknown>;
    highlight(text: string, grammar: unknown, language: string): string;
  }
  const Prism: PrismStatic;
  export default Prism;
}
