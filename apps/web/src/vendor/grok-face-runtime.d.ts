export interface FaceRenderer {
  svg: SVGSVGElement;
  update(props: { shape?: string; state?: string; reducedMotion?: boolean }): void;
  advance(milliseconds: number): void;
  action(name: "blink"): void;
  destroy(): void;
}
export function createGrokRenderer(mount: HTMLElement, props: { shape: string; state: string; seed: number; reducedMotion?: boolean }): FaceRenderer;
