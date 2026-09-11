import { type HTMLAttributes, computed } from 'vue'
import { cn } from '@/lib/utils'

export type ButtonVariant = 'default' | 'secondary' | 'outline' | 'ghost' | 'destructive'
export type ButtonSize = 'default' | 'sm' | 'icon'

const variants: Record<ButtonVariant, string> = {
  default: 'bg-primary text-primary-foreground hover:bg-primary/90 active:bg-primary/85',
  secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
  outline: 'bg-muted text-foreground hover:bg-accent hover:text-accent-foreground',
  ghost: 'hover:bg-muted hover:text-foreground',
  destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
}

const sizes: Record<ButtonSize, string> = {
  default: 'h-9 px-3.5 text-sm',
  sm: 'h-8 px-2.5 text-xs',
  icon: 'h-8 w-8',
}

export function buttonClass(
  variant: ButtonVariant = 'default',
  size: ButtonSize = 'default',
  className?: HTMLAttributes['class'],
) {
  return cn(
    'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl font-medium transition-colors',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30',
    'disabled:pointer-events-none disabled:opacity-50',
    variants[variant],
    sizes[size],
    className,
  )
}

export function useButtonClass(
  variant: () => ButtonVariant,
  size: () => ButtonSize,
  className?: () => HTMLAttributes['class'] | undefined,
) {
  return computed(() => buttonClass(variant(), size(), className?.()))
}
