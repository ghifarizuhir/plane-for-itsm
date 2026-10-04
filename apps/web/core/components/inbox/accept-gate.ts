export type TIntakeAcceptGate = {
  missingType: boolean;
  missingService: boolean;
  ready: boolean;
};

type TIntakeAcceptGateInput = {
  /** Project punya minimal satu tipe non-epic aktif. */
  projectHasTypes: boolean;
  /** Tipe work item item intake (nullable). */
  issueTypeId: string | null | undefined;
  /** `requires_service` tipe terpilih. */
  typeRequiresService: boolean;
  /** Item sudah punya minimal satu service link live. */
  hasLinkedService: boolean;
};

export const getIntakeAcceptGate = (input: TIntakeAcceptGateInput): TIntakeAcceptGate => {
  const missingType = input.projectHasTypes && !input.issueTypeId;
  const missingService =
    !missingType && Boolean(input.issueTypeId) && input.typeRequiresService && !input.hasLinkedService;
  return { missingType, missingService, ready: !missingType && !missingService };
};
