import { useContext } from "react";
// store
import { StoreContext } from "@/lib/store-context";
import type { IReviewStore } from "@/store/review.store";

export const useReview = (): IReviewStore => {
  const context = useContext(StoreContext);
  if (context === undefined) throw new Error("useReview must be used within StoreProvider");
  return context.review;
};
