/**
 * LRU tile cache — evicts least recently used tiles when capacity is exceeded.
 * Automatically closes evicted ImageBitmaps to free GPU memory.
 *
 * Uses JavaScript Map iteration-order guarantee: map.keys().next().value
 * always returns the oldest (least recently used) entry.
 */

interface TileEntry {
  bitmap: ImageBitmap;
  imgX: number;
  imgY: number;
  imgW: number;
  imgH: number;
  mipLevel: number;
}

export class LRUTileCache {
  private map = new Map<string, TileEntry>();
  private maxTiles: number;

  constructor(maxTiles: number) {
    this.maxTiles = maxTiles;
  }

  /** Check if a tile key exists in cache without touching LRU order. */
  has(key: string): boolean {
    return this.map.has(key);
  }

  /** Access a tile, moving it to the front of the LRU. */
  get(key: string): TileEntry | undefined {
    const tile = this.map.get(key);
    if (tile !== undefined) {
      this.map.delete(key);
      this.map.set(key, tile);
    }
    return tile;
  }

  /**
   * Insert or replace a tile. If replacing, the old bitmap is closed.
   * If capacity is exceeded, the oldest tile is evicted (bitmap closed).
   */
  set(key: string, tile: TileEntry): void {
    const existing = this.map.get(key);
    if (existing) {
      existing.bitmap.close();
      this.map.delete(key);
    }
    if (this.map.size >= this.maxTiles) {
      const firstKey = this.map.keys().next().value!;
      const evicted = this.map.get(firstKey)!;
      console.debug(`[LRU] evict tile ${firstKey} (${this.map.size}/${this.maxTiles})`);
      evicted.bitmap.close();
      this.map.delete(firstKey);
    }
    this.map.set(key, tile);
  }

  /** Remove a tile, closing its bitmap. */
  delete(key: string): boolean {
    const tile = this.map.get(key);
    if (tile) {
      tile.bitmap.close();
      return this.map.delete(key);
    }
    return false;
  }

  /** Clear all tiles, closing all bitmaps. */
  clear(): void {
    for (const tile of this.map.values()) {
      tile.bitmap.close();
    }
    this.map.clear();
  }

  /** Iterate all entries without changing LRU order. */
  entries(): IterableIterator<[string, TileEntry]> {
    return this.map.entries();
  }

  get size(): number {
    return this.map.size;
  }
}
