import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IReleaseStore } from "@/store/release.store";

export const useRelease = (): IReleaseStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useRelease must be used within StoreProvider");
  return context.release;
};
