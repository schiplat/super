<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import 'xterm/css/xterm.css';
import { FONT_MONO } from '@/constants/fonts';

const props = defineProps<{
  logs: string[]; // Optional historical log lines
}>();

const terminalContainer = ref<HTMLElement | null>(null);
let term: Terminal | null = null;
let fitAddon: FitAddon | null = null;

onMounted(() => {
  if (!terminalContainer.value) return;

  // 1. Init xterm
  term = new Terminal({
    cursorBlink: true,
    fontSize: 12,
    fontFamily: FONT_MONO,
    theme: {
      background: '#1e1e1e',
      foreground: '#d4d4d4',
    },
    disableStdin: true, // Read-only
    convertEol: true,   // Normalize line endings
  });

  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);

  term.open(terminalContainer.value);
  fitAddon.fit();

  // 2. Write initial lines
  props.logs.forEach(line => term?.writeln(line));

  // Reflow on window resize
  window.addEventListener('resize', handleResize);
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', handleResize);
  term?.dispose();
});

function handleResize() {
  fitAddon?.fit();
}

// Exposed API for parent components
defineExpose({
  writeLine: (line: string, source: 'stdout' | 'stderr' | 'superd') => {
    if (!term) return;
    if (source === 'superd') {
      term.writeln(`\x1b[33m${line}\x1b[0m`);
    } else if (source === 'stderr') {
      term.writeln(`\x1b[31m${line}\x1b[0m`);
    } else {
      term.writeln(line);
    }
  },
  clear: () => term?.clear(),
  fit: () => setTimeout(() => fitAddon?.fit(), 100) // Defer until container has size
});
</script>

<template>
  <div class="h-full w-full bg-[#1e1e1e] p-2 rounded-lg overflow-hidden">
    <div ref="terminalContainer" class="h-full w-full"></div>
  </div>
</template>
