/**
 * Preview mode — the work panel's "maximize" toggle — hides the center pane so
 * the dock can take the whole client area beside the sidebar.
 *
 * Hiding the pane is only safe for the page the user was looking at when they
 * entered preview: if they navigate to another destination while previewing,
 * that page would mount behind a hidden pane and the click on the sidebar would
 * look like it did nothing. Preview therefore ends on a page *change*.
 *
 * The guard keys on the change, not on "is this the chat page". Keyed on the
 * latter, entering preview from any other destination was reverted on the very
 * next frame — the dock flashed open and snapped straight back, which read as
 * a dead button.
 */
export function shouldLeavePreviewForPageChange({
  maximized,
  previewPage,
  page,
}: {
  /** Whether preview mode is currently on. */
  maximized: boolean;
  /** The page preview was entered from. */
  previewPage: string;
  /** The page the store wants to show now. */
  page: string;
}): boolean {
  return maximized && page !== previewPage;
}
