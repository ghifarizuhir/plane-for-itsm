/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 *
 * ShapeWaves2D — Canvas 2D rendition of ShapeWaves for browsers without a
 * usable WebGPU adapter. Same grid, bands, carved text, intro and pointer
 * ripples, just without the bloom pass.
 */

import { useEffect, useRef, useState } from "react";
// styles
// oxlint-disable-next-line import/no-unassigned-import
import "./shape-waves.css";

export type ShapeWaves2DProps = {
  text?: string;
  fontFamily?: string;
  fontWeight?: string | number;
  textSize?: number;
  cellSize?: number;
  dotSize?: number;
  color?: string;
  hoverColor?: string;
  backgroundColor?: string;
  speed?: number;
  scale?: number;
  contrast?: number;
  brightness?: number;
  fade?: number;
  interactive?: boolean;
  splashRadius?: number;
  splashStrength?: number;
  glow?: number;
  intro?: boolean;
  introDuration?: number;
  paused?: boolean;
  className?: string;
};

const MAX_DPR = 1.5;
const SIMULATION_STEP = 1 / 60;
const WAVE_SPEED = 0.42;
const WAVE_FRICTION = 0.94;
const WAVE_DECAY = 0.972;
const SETTLED_THRESHOLD = 0.01;
const INTRO_BAND = 0.2;
const INTRO_WARP = 0.3;
const INTRO_JITTER = 0.16;
const INTRO_END = 1 + INTRO_WARP + INTRO_JITTER + INTRO_BAND;
const TARGET_FRAME_MS = 1000 / 40;

const parseColor = (value: string | undefined, fallback: string): [number, number, number] => {
  const source = typeof value === "string" ? value.trim() : "";
  const match = /^#?([\da-f]{3}|[\da-f]{6})$/i.exec(source);
  let hex = match?.[1];
  if (!hex) hex = /^#?([\da-f]{6})$/i.exec(fallback)?.[1] ?? "000000";
  if (hex.length === 3) hex = hex.replace(/./g, (char) => char + char);
  return [0, 2, 4].map((offset) => parseInt(hex.slice(offset, offset + 2), 16) / 255) as [number, number, number];
};

const toCss = (rgb: [number, number, number], alpha = 1) =>
  `rgba(${rgb.map((channel) => Math.round(channel * 255)).join(",")},${alpha})`;

const mixChannel = (from: number, to: number, amount: number) => Math.round((from + (to - from) * amount) * 255);

const hash21 = (x: number, y: number) => {
  const value = Math.sin(x * 127.1 + y * 311.7) * 43758.5453;
  return value - Math.floor(value);
};

const valueNoise = (x: number, y: number) => {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  const xf = x - xi;
  const yf = y - yi;
  const u = xf * xf * (3 - 2 * xf);
  const v = yf * yf * (3 - 2 * yf);
  const a = hash21(xi, yi);
  const b = hash21(xi + 1, yi);
  const c = hash21(xi, yi + 1);
  const d = hash21(xi + 1, yi + 1);
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
};

const fbm = (x: number, y: number) => (valueNoise(x, y) + 0.5 * valueNoise(x * 2, y * 2)) / 1.5;

export function ShapeWaves2D({
  text = "",
  fontFamily = 'Geist, "Geist Sans", system-ui, sans-serif',
  fontWeight = 500,
  textSize = 0.6,
  cellSize = 10,
  dotSize = 0.75,
  color = "#929292",
  hoverColor = "#ffffff",
  backgroundColor = "#000000",
  speed = 1,
  scale = 1,
  contrast = 1,
  brightness = 0.4,
  fade = 0.25,
  interactive = true,
  splashRadius = 40,
  splashStrength = 0.4,
  glow = 0.35,
  intro = true,
  introDuration = 1.6,
  paused = false,
  className = "",
}: ShapeWaves2DProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  const settingsRef = useRef({
    text: String(text ?? ""),
    fontFamily,
    fontWeight,
    textSize,
    cellSize: Math.max(2, cellSize),
    dotSize,
    color,
    hoverColor,
    backgroundColor,
    speed,
    scale: Math.max(0.05, scale),
    contrast,
    brightness,
    fade,
    interactive,
    splashRadius,
    splashStrength,
    glow,
    intro,
    introDuration: Math.max(0.1, introDuration),
    paused,
  });
  const rebuildMaskRef = useRef(() => {});
  const wakeRef = useRef(() => {});
  settingsRef.current = {
    text: String(text ?? ""),
    fontFamily,
    fontWeight,
    textSize,
    cellSize: Math.max(2, cellSize),
    dotSize,
    color,
    hoverColor,
    backgroundColor,
    speed,
    scale: Math.max(0.05, scale),
    contrast,
    brightness,
    fade,
    interactive,
    splashRadius,
    splashStrength,
    glow,
    intro,
    introDuration: Math.max(0.1, introDuration),
    paused,
  };

  const maskSignature = [text, fontFamily, fontWeight, textSize].join("|");
  const settingsSignature = [
    cellSize,
    dotSize,
    color,
    hoverColor,
    backgroundColor,
    speed,
    scale,
    contrast,
    brightness,
    fade,
    interactive,
    splashRadius,
    splashStrength,
    glow,
    intro,
    introDuration,
    paused,
  ].join("|");

  useEffect(() => {
    rebuildMaskRef.current();
  }, [maskSignature]);

  useEffect(() => {
    wakeRef.current();
  }, [settingsSignature]);

  useEffect(() => {
    const root = rootRef.current;
    const canvas = canvasRef.current;
    if (!root || !canvas) return undefined;
    const context = canvas.getContext("2d");
    if (!context) return undefined;

    let disposed = false;
    let frameId = 0;
    let lastFrameTime = 0;
    let lastDrawTime = 0;
    let time = 0;
    let dpr = 1;
    let width = 1;
    let height = 1;
    let cols = 1;
    let rows = 1;
    let cellPx = 10;
    let lastCellSize = 0;
    let visible = true;
    let presented = false;
    let chargesActive = false;
    let simulationBacklog = 0;
    let introProgress = INTRO_END;
    let introArmed = false;
    let lastIntro = false;
    let bounds: DOMRect | null = null;
    let charges = new Float32Array(1);
    let heights = new Float32Array(1);
    let previousHeights = new Float32Array(1);
    let mask = new Uint8Array(1);
    let unsubscribeResize: (() => void) | undefined;
    let unsubscribeVisibility: (() => void) | undefined;
    let wakeRenderer = () => {};
    const pointer = { x: 0, y: 0, at: 0, inside: false };
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const maskCanvas = document.createElement("canvas");
    const maskContext = maskCanvas.getContext("2d");

    const invalidateBounds = () => {
      bounds = null;
    };

    const measure = () => {
      const settings = settingsRef.current;
      dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
      width = Math.max(1, Math.round(root.clientWidth * dpr));
      height = Math.max(1, Math.round(root.clientHeight * dpr));
      canvas.width = width;
      canvas.height = height;
      cols = Math.max(1, Math.round(width / (settings.cellSize * dpr)));
      cellPx = width / cols;
      rows = Math.max(1, Math.floor(height / cellPx));
    };

    const rebuildMask = () => {
      if (!maskContext) return;
      const settings = settingsRef.current;
      mask = new Uint8Array(cols * rows);
      const content = settings.text.trim();
      if (!content) {
        wakeRenderer();
        return;
      }
      maskCanvas.width = cols;
      maskCanvas.height = rows;
      maskContext.clearRect(0, 0, cols, rows);
      let fontPx = Math.max(4, settings.textSize * rows);
      maskContext.font = `${settings.fontWeight} ${fontPx}px ${settings.fontFamily}`;
      const measured = maskContext.measureText(content).width;
      const maxWidth = cols * 0.9;
      if (measured > maxWidth) {
        fontPx = Math.max(4, (fontPx * maxWidth) / measured);
        maskContext.font = `${settings.fontWeight} ${fontPx}px ${settings.fontFamily}`;
      }
      maskContext.textAlign = "center";
      maskContext.textBaseline = "middle";
      maskContext.fillStyle = "#fff";
      maskContext.fillText(content, cols / 2, rows / 2);
      const pixels = maskContext.getImageData(0, 0, cols, rows).data;
      for (let index = 0; index < mask.length; index++) {
        mask[index] = pixels[index * 4 + 3] > 128 ? 1 : 0;
      }
      wakeRenderer();
    };

    const splash = (cellX: number, cellY: number, strength: number) => {
      const settings = settingsRef.current;
      const sigma = Math.max(0.5, (settings.splashRadius / settings.cellSize) * 0.5);
      const reach = Math.ceil(sigma * 2.5);
      const centerCol = cellX - 0.5;
      const centerRow = cellY - 0.5;
      const minRow = Math.max(0, Math.floor(centerRow - reach));
      const maxRow = Math.min(rows - 1, Math.ceil(centerRow + reach));
      const minCol = Math.max(0, Math.floor(centerCol - reach));
      const maxCol = Math.min(cols - 1, Math.ceil(centerCol + reach));
      for (let row = minRow; row <= maxRow; row++) {
        const dy = row - centerRow;
        for (let col = minCol; col <= maxCol; col++) {
          const dx = col - centerCol;
          const bump = strength * Math.exp(-(dx * dx + dy * dy) / (2 * sigma * sigma));
          const index = row * cols + col;
          heights[index] = Math.min(1.2, heights[index] + bump);
        }
      }
      chargesActive = true;
    };

    const handlePointerMove = (event: PointerEvent) => {
      const settings = settingsRef.current;
      if (!settings.interactive || disposed) return;
      if (!bounds) bounds = root.getBoundingClientRect();
      const now = performance.now();
      const x = event.clientX - bounds.left;
      const y = event.clientY - bounds.top;
      const inside = x >= 0 && y >= 0 && x <= bounds.width && y <= bounds.height;
      if (inside) {
        const elapsed = pointer.inside ? Math.max(8, now - pointer.at) : 16;
        const travelled = pointer.inside ? Math.hypot(x - pointer.x, y - pointer.y) : 0;
        const pointerSpeed = (travelled / elapsed) * 1000;
        splash(
          x / settings.cellSize,
          y / settings.cellSize,
          Math.min(1, 0.22 + pointerSpeed * 0.0006) * settings.splashStrength
        );
        wakeRenderer();
      }
      pointer.x = x;
      pointer.y = y;
      pointer.at = now;
      pointer.inside = inside;
    };

    const stepRipples = () => {
      const lastCol = cols - 1;
      const lastRow = rows - 1;
      let peak = 0;
      for (let row = 0; row < rows; row++) {
        const up = (row === 0 ? row : row - 1) * cols;
        const down = (row === lastRow ? row : row + 1) * cols;
        const base = row * cols;
        for (let col = 0; col < cols; col++) {
          const index = base + col;
          const left = base + (col === 0 ? col : col - 1);
          const right = base + (col === lastCol ? col : col + 1);
          const current = heights[index];
          const laplacian = heights[left] + heights[right] + heights[up + col] + heights[down + col] - 4 * current;
          const velocity = (current - previousHeights[index]) * WAVE_FRICTION;
          const next = (current + velocity + WAVE_SPEED * laplacian) * WAVE_DECAY;
          previousHeights[index] = next;
          const charge = Math.min(1, Math.max(0, next));
          charges[index] = charge;
          if (charge > peak) peak = charge;
        }
      }
      const swap = heights;
      heights = previousHeights;
      previousHeights = swap;
      return peak;
    };

    const updateCharges = (deltaSeconds: number) => {
      if (!chargesActive) return false;
      simulationBacklog = Math.min(simulationBacklog + deltaSeconds, SIMULATION_STEP * 4);
      let peak = 1;
      while (simulationBacklog >= SIMULATION_STEP) {
        simulationBacklog -= SIMULATION_STEP;
        peak = stepRipples();
      }
      if (peak < SETTLED_THRESHOLD) {
        heights.fill(0);
        previousHeights.fill(0);
        charges.fill(0);
        chargesActive = false;
      }
      return chargesActive;
    };

    const isAnimating = () => {
      const settings = settingsRef.current;
      return visible && !document.hidden && !settings.paused && settings.speed > 0 && !reduceMotion.matches;
    };

    const draw = (deltaSeconds: number) => {
      const settings = settingsRef.current;
      const baseColor = parseColor(settings.color, "#929292");
      const hoverRgb = parseColor(settings.hoverColor, "#ffffff");
      const backgroundRgb = parseColor(settings.backgroundColor, "#000000");
      const half = cellPx * 0.5;
      const radius = settings.dotSize * half;
      context.setTransform(1, 0, 0, 1, 0, 0);
      context.fillStyle = toCss(backgroundRgb);
      context.fillRect(0, 0, width, height);

      if (introArmed) {
        introArmed = false;
        introProgress = 0;
      }
      const introPlaying = introProgress < INTRO_END;
      if (introPlaying) {
        introProgress = Math.min(INTRO_END, introProgress + (deltaSeconds / settings.introDuration) * INTRO_END);
      }

      const noiseScale = 1 / (32 * settings.scale);
      const phase = time;
      const blend = 0.5 + 0.5 * Math.sin(phase * 0.9);
      const threshold = 0.5 - (settings.brightness - 0.5) * 0.4;
      const squares = new Path2D();
      const circles = new Path2D();
      const triangles = new Path2D();
      const lit: { x: number; y: number; size: number; amount: number }[] = [];

      for (let row = 0; row < rows; row++) {
        for (let col = 0; col < cols; col++) {
          const index = row * cols + col;
          if (mask[index]) continue;
          const noiseX = col * noiseScale + phase * 0.35;
          const noiseY = row * noiseScale + phase * 0.22;
          const first = fbm(noiseX, noiseY);
          const second = fbm(noiseX + 0.37 + phase * 0.15, noiseY + 0.61 - phase * 0.12);
          const noise = first + (second - first) * blend;
          const tone = Math.min(0.9999, Math.max(0, (noise - threshold) * 2.8 * settings.contrast + 0.5));
          const band = Math.floor(tone * 3);
          const centerX = (col + 0.5) * cellPx;
          const centerY = (row + 0.5) * cellPx;
          let size = radius;
          let front = 0;
          if (introPlaying) {
            const nx = ((col + 0.5) / cols) * 2 - 1;
            const ny = ((row + 0.5) / rows) * 2 - 1;
            const radial = Math.hypot(nx, ny) * Math.SQRT1_2;
            const jitter = hash21(col, row) * INTRO_JITTER;
            const spread = radial + jitter + INTRO_WARP;
            const bandWidth = INTRO_BAND * (0.6 + 0.8 * hash21(col + 17, row + 9));
            const progress = Math.min(1, Math.max(0, (introProgress - spread) / bandWidth));
            if (progress <= 0) continue;
            const back = progress - 1;
            size = Math.max(size * (1 + 2.70158 * back * back * back + 1.70158 * back * back), 0.02 * half);
            const rawFront = 1 - Math.min(1, Math.max(0, Math.abs(introProgress - spread) / bandWidth));
            front = rawFront * rawFront * (3 - 2 * rawFront);
          }
          const charge = charges[index];
          if (charge > 0.05 || front > 0.05) {
            lit.push({ x: centerX, y: centerY, size, amount: Math.max(charge, front * 0.35) });
          } else if (band === 0) {
            triangles.moveTo(centerX, centerY - size);
            triangles.lineTo(centerX + size * 0.92, centerY + size * 0.8);
            triangles.lineTo(centerX - size * 0.92, centerY + size * 0.8);
            triangles.closePath();
          } else if (band === 1) {
            circles.moveTo(centerX + size, centerY);
            circles.arc(centerX, centerY, size, 0, Math.PI * 2);
          } else {
            squares.rect(centerX - size, centerY - size, size * 2, size * 2);
          }
        }
      }

      context.fillStyle = toCss(baseColor);
      context.fill(squares);
      context.fill(circles);
      context.fill(triangles);

      if (lit.length) {
        const glowBlur = settings.glow * 8;
        for (const cell of lit) {
          const amount = Math.min(1, Math.max(0.15, cell.amount));
          const r = mixChannel(baseColor[0], hoverRgb[0], amount);
          const g = mixChannel(baseColor[1], hoverRgb[1], amount);
          const b = mixChannel(baseColor[2], hoverRgb[2], amount);
          context.fillStyle = `rgb(${r},${g},${b})`;
          if (glowBlur > 0 && amount > 0.3) {
            context.shadowBlur = glowBlur;
            context.shadowColor = `rgb(${r},${g},${b})`;
          } else {
            context.shadowBlur = 0;
          }
          context.beginPath();
          context.rect(cell.x - cell.size, cell.y - cell.size, cell.size * 2, cell.size * 2);
          context.fill();
        }
        context.shadowBlur = 0;
      }

      if (settings.fade > 0) {
        const gradient = context.createRadialGradient(
          width / 2,
          height / 2,
          Math.min(width, height) * 0.1,
          width / 2,
          height / 2,
          Math.max(width, height) * 0.72
        );
        gradient.addColorStop(0, "rgba(0,0,0,0)");
        gradient.addColorStop(Math.min(0.99, Math.max(0, 1 - settings.fade * 1.6)), "rgba(0,0,0,0)");
        gradient.addColorStop(1, toCss(backgroundRgb));
        context.fillStyle = gradient;
        context.fillRect(0, 0, width, height);
      }

      if (!presented) {
        presented = true;
        setReady(true);
      }
    };

    const render = (now: number) => {
      frameId = 0;
      if (disposed) return;
      const settings = settingsRef.current;
      const deltaSeconds = lastFrameTime ? Math.min(0.1, (now - lastFrameTime) / 1000) : 0;
      const shouldDraw = lastDrawTime === 0 || now - lastDrawTime >= TARGET_FRAME_MS || introProgress < INTRO_END;
      if (!shouldDraw) {
        frameId = requestAnimationFrame(render);
        return;
      }
      lastFrameTime = now;
      lastDrawTime = now;
      const animating = isAnimating();
      if (animating) time += deltaSeconds * settings.speed * 0.18;
      const hovering = visible && !document.hidden && updateCharges(deltaSeconds);
      draw(deltaSeconds);
      if (animating || hovering || introProgress < INTRO_END) frameId = requestAnimationFrame(render);
      else lastFrameTime = 0;
    };

    wakeRenderer = () => {
      if (disposed || frameId) return;
      frameId = requestAnimationFrame(render);
    };

    const handleWake = () => wakeRenderer();

    const resize = () => {
      if (disposed) return;
      invalidateBounds();
      measure();
      lastCellSize = settingsRef.current.cellSize;
      charges = new Float32Array(cols * rows);
      heights = new Float32Array(cols * rows);
      previousHeights = new Float32Array(cols * rows);
      chargesActive = false;
      rebuildMask();
      wakeRenderer();
    };

    rebuildMaskRef.current = () => {
      if (disposed) return;
      rebuildMask();
    };
    wakeRef.current = () => {
      const settings = settingsRef.current;
      if (settings.cellSize !== lastCellSize) {
        resize();
        return;
      }
      if (settings.intro !== lastIntro) {
        lastIntro = settings.intro;
        if (settings.intro && !reduceMotion.matches) introArmed = true;
      }
      wakeRenderer();
    };

    const observer = new ResizeObserver(resize);
    observer.observe(root);
    unsubscribeResize = () => observer.disconnect();
    const visibilityObserver = new IntersectionObserver(
      (entries) => {
        visible = entries.some((entry) => entry.isIntersecting);
        if (visible) wakeRenderer();
      },
      { threshold: 0 }
    );
    visibilityObserver.observe(root);
    unsubscribeVisibility = () => visibilityObserver.disconnect();
    document.addEventListener("visibilitychange", handleWake);
    reduceMotion.addEventListener("change", handleWake);
    window.addEventListener("pointermove", handlePointerMove, { passive: true });
    window.addEventListener("scroll", invalidateBounds, { capture: true, passive: true });

    lastIntro = settingsRef.current.intro;
    if (lastIntro && !reduceMotion.matches) introArmed = true;
    resize();

    return () => {
      disposed = true;
      wakeRenderer = () => {};
      rebuildMaskRef.current = () => {};
      wakeRef.current = () => {};
      document.removeEventListener("visibilitychange", handleWake);
      reduceMotion.removeEventListener("change", handleWake);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("scroll", invalidateBounds, { capture: true });
      unsubscribeResize?.();
      unsubscribeVisibility?.();
      if (frameId) cancelAnimationFrame(frameId);
    };
  }, []);

  return (
    <div
      ref={rootRef}
      className={`shape-waves ${className}`}
      data-ready={ready}
      style={{ backgroundColor }}
      aria-hidden="true"
    >
      <canvas ref={canvasRef} className="shape-waves__canvas" />
    </div>
  );
}
