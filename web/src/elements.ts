import type { Component, Rect, BlockPlacement } from './model';

export function contentBounds(component: Component): Rect {
  return { x: 0, y: 0, width: component.rect.width - 2 * component.padding, height: component.rect.height - 2 * component.padding - (component.blocks.some(block => block.source.kind === 'field') ? 18 : 0) };
}
function intersects(a: Rect, b: Rect) { return a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y; }
export function validElement(rect: Rect, id: string, component: Component): boolean {
  const bounds = contentBounds(component);
  return Object.values(rect).every(Number.isInteger) && rect.x >= 0 && rect.y >= 0 && rect.width >= 8 && rect.height >= 8
    && rect.x + rect.width <= bounds.width && rect.y + rect.height <= bounds.height
    && !component.blocks.some(block => block.id !== id && block.rect != null && intersects(rect, block.rect));
}
export function validElements(component: Component): boolean {
  return component.content_layout === 'positioned'
    ? component.blocks.every(block => block.rect != null && validElement(block.rect, block.id, component))
    : component.blocks.every(block => block.rect == null);
}
export function positioned(component: Component, placements: BlockPlacement[]): Component {
  const result = structuredClone(component);
  result.content_layout = 'positioned';
  for (const block of result.blocks) {
    const rect = placements.find(placement => placement.id === block.id)?.rect;
    if (!rect) throw new Error('Some blocks have no visible space. Enlarge the component or adjust its content before positioning elements.');
    block.rect = { ...rect, height: Math.max(8, rect.height) };
  }
  if (!validElements(result)) throw new Error('Give every block room using size or spacing before switching to positioned elements.');
  return result;
}
export function freeElementRect(component: Component, id: string, height = 30): Rect | null {
  const bounds = contentBounds(component);
  const width = Math.min(128, bounds.width);
  if (width < 8 || height > bounds.height) return null;
  for (let y = 0; y + height <= bounds.height; y += 8) {
    for (let x = 0; x + width <= bounds.width; x += 8) {
      const rect = { x, y, width, height };
      if (validElement(rect, id, component)) return rect;
    }
  }
  return null;
}
