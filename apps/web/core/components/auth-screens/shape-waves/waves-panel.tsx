/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { lazy, Suspense, useEffect, useState } from "react";
import type { ShapeWavesProps } from "./shape-waves";

const ShapeWaves = lazy(() => import("./shape-waves").then((module) => ({ default: module.ShapeWaves })));
const ShapeWaves2D = lazy(() => import("./shape-waves-2d").then((module) => ({ default: module.ShapeWaves2D })));

const DESKTOP_QUERY = "(min-width: 64rem)";

const WAVES_PROPS: ShapeWavesProps = {
  text: "Terraline ITSM",
  fontFamily: 'Geist, "Geist Sans", system-ui, sans-serif',
  fontWeight: 500,
  textSize: 0.51,
  shapes: "mixed",
  cellSize: 10,
  dotSize: 0.75,
  color: "#929292",
  hoverColor: "#ffffff",
  backgroundColor: "#120f17",
  speed: 1,
  scale: 0.5,
  contrast: 1,
  brightness: 0.38,
  flow: 0,
  direction: 0,
  fade: 0.25,
  interactive: true,
  splashRadius: 40,
  splashStrength: 0.4,
  glow: 0.35,
  intro: true,
  introDuration: 1.6,
  paused: false,
};

type WavesRenderer = "probing" | "webgpu" | "canvas2d";

function useIsDesktop() {
  const [isDesktop, setIsDesktop] = useState(
    () => typeof window !== "undefined" && window.matchMedia(DESKTOP_QUERY).matches
  );

  useEffect(() => {
    const query = window.matchMedia(DESKTOP_QUERY);
    const handleChange = (event: MediaQueryListEvent) => setIsDesktop(event.matches);
    setIsDesktop(query.matches);
    query.addEventListener("change", handleChange);
    return () => query.removeEventListener("change", handleChange);
  }, []);

  return isDesktop;
}

function useWavesRenderer(enabled: boolean): WavesRenderer {
  const [renderer, setRenderer] = useState<WavesRenderer>("probing");

  useEffect(() => {
    if (!enabled) return undefined;
    let cancelled = false;
    const probe = async () => {
      try {
        const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "low-power" });
        if (cancelled) return;
        if (adapter) {
          setRenderer("webgpu");
          return;
        }
      } catch {
        // no adapter is available; the Canvas 2D renderer below covers it
      }
      if (cancelled) return;
      console.warn("[waves] No usable WebGPU adapter found; using the Canvas 2D renderer.");
      setRenderer("canvas2d");
    };
    void probe();
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  return renderer;
}

export function AuthWavesPanel() {
  const isDesktop = useIsDesktop();
  const renderer = useWavesRenderer(isDesktop);
  const [webgpuFailed, setWebgpuFailed] = useState(false);

  const handleWebgpuError = (error: Error) => {
    console.warn("[waves] WebGPU rendering failed; using the Canvas 2D renderer.", error);
    setWebgpuFailed(true);
  };

  const showWebgpu = isDesktop && renderer === "webgpu" && !webgpuFailed;
  const showCanvas2D = isDesktop && (renderer === "canvas2d" || (renderer === "webgpu" && webgpuFailed));

  return (
    <div className="relative hidden h-full min-w-0 flex-1 overflow-hidden border-l border-white/[0.06] bg-[#120f17] lg:block">
      <div className="absolute inset-0 bg-[radial-gradient(130%_130%_at_72%_18%,#221a2e_0%,#120f17_58%)]" />
      {showWebgpu && (
        <Suspense fallback={null}>
          <div className="absolute inset-0">
            <ShapeWaves {...WAVES_PROPS} onError={handleWebgpuError} />
          </div>
        </Suspense>
      )}
      {showCanvas2D && (
        <Suspense fallback={null}>
          <div className="absolute inset-0">
            <ShapeWaves2D {...WAVES_PROPS} />
          </div>
        </Suspense>
      )}
      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-end justify-between p-8 xl:p-10">
        <span className="font-code text-11 tracking-[0.28em] text-white/30 uppercase">Work in all dimensions</span>
      </div>
    </div>
  );
}
