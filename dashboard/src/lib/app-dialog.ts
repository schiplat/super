import { reactive } from 'vue'

export type AppDialogVariant = 'default' | 'destructive' | 'secondary'

export interface AppDialogRequest {
  kind: 'alert' | 'confirm'
  title: string
  message: string
  confirmLabel: string
  cancelLabel: string
  variant: AppDialogVariant
}

interface AppDialogState extends AppDialogRequest {
  open: boolean
  resolve: ((value: boolean) => void) | null
}

const state = reactive<AppDialogState>({
  open: false,
  kind: 'alert',
  title: '',
  message: '',
  confirmLabel: 'OK',
  cancelLabel: 'Cancel',
  variant: 'default',
  resolve: null,
})

export const appDialogState = state

function openDialog(req: AppDialogRequest): Promise<boolean> {
  // Resolve any previous dialog as cancelled (should not overlap in practice).
  if (state.open && state.resolve) {
    state.resolve(false)
  }
  state.kind = req.kind
  state.title = req.title
  state.message = req.message
  state.confirmLabel = req.confirmLabel
  state.cancelLabel = req.cancelLabel
  state.variant = req.variant
  state.open = true
  return new Promise<boolean>((resolve) => {
    state.resolve = resolve
  })
}

/** Alert-style dialog (single OK). Resolves when dismissed. */
export async function alertDialog(
  message: string,
  opts?: {
    title?: string
    confirmLabel?: string
    variant?: AppDialogVariant
  },
): Promise<void> {
  await openDialog({
    kind: 'alert',
    title: opts?.title ?? 'Notice',
    message,
    confirmLabel: opts?.confirmLabel ?? 'OK',
    cancelLabel: 'Cancel',
    variant: opts?.variant ?? 'default',
  })
}

/** Confirm-style dialog. Resolves `true` on confirm, `false` on cancel/dismiss. */
export function confirmDialog(
  message: string,
  opts?: {
    title?: string
    confirmLabel?: string
    cancelLabel?: string
    variant?: AppDialogVariant
  },
): Promise<boolean> {
  return openDialog({
    kind: 'confirm',
    title: opts?.title ?? 'Confirm',
    message,
    confirmLabel: opts?.confirmLabel ?? 'Confirm',
    cancelLabel: opts?.cancelLabel ?? 'Cancel',
    variant: opts?.variant ?? 'default',
  })
}

export function settleAppDialog(confirmed: boolean) {
  const resolve = state.resolve
  state.open = false
  state.resolve = null
  resolve?.(confirmed)
}
