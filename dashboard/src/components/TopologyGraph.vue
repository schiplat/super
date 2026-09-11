<script setup lang="ts">
import { ref, watch, nextTick, markRaw, onMounted, onUnmounted, provide, computed } from 'vue';
import { VueFlow, useVueFlow } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls, ControlButton } from '@vue-flow/controls';
import { MiniMap } from '@vue-flow/minimap';
import { Maximize2, Minimize2, RefreshCw } from 'lucide-vue-next';
import dagre from 'dagre';
import type { Program } from '@/types';
import { useDark } from '@vueuse/core';
import ProcessNode from '@/components/flow/ProcessNode.vue';
import { PROCESS_NODE_HEIGHT, PROCESS_NODE_WIDTH } from '@/components/flow/processNodeLayout';
import SectionLabelNode from '@/components/flow/SectionLabelNode.vue';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/controls/dist/style.css';
import '@vue-flow/minimap/dist/style.css';

const props = defineProps<{ programs: Program[] }>();
const emit = defineEmits(['node-click']);
const isDark = useDark({ selector: 'html', attribute: 'class', valueDark: 'dark', valueLight: 'light' });

const graphRoot = ref<HTMLElement | null>(null);
const isFullscreen = ref(false);
provide('topologyFullscreen', isFullscreen);

const backgroundPatternColor = computed(() => {
  if (isFullscreen.value) {
    return isDark.value ? '#52525b' : '#d4d4d8';
  }
  return isDark.value ? '#3f3f46' : '#e4e4e7';
});

const { fitView, onNodeClick, setViewport } = useVueFlow();

const nodeTypes = {
  'process-node': markRaw(ProcessNode),
  'section-label': markRaw(SectionLabelNode),
};
const nodes = ref<any[]>([]);
const edges = ref<any[]>([]);

// Layout constants — keep in sync with ProcessNode.vue
const NODE_WIDTH = PROCESS_NODE_WIDTH;
const NODE_HEIGHT = PROCESS_NODE_HEIGHT;
const STANDALONE_WIDTH = PROCESS_NODE_WIDTH;
const STANDALONE_HEIGHT = PROCESS_NODE_HEIGHT;
const GAP_X = 40;
const GAP_Y = 28;
// Canvas padding so nodes are not clipped at edges
const CANVAS_PADDING_X = 50;
const CANVAS_PADDING_Y = 50;

function refreshGraph() {
  transformData();
  nextTick(() => {
    fitView({ padding: 0.12 });
  });
}

function toggleFullscreen() {
  const el = graphRoot.value;
  if (!el) return;

  if (document.fullscreenElement === el) {
    void document.exitFullscreen?.();
    return;
  }

  void el.requestFullscreen?.().catch(() => {
    // Safari / restricted contexts may reject fullscreen
  });
}

function onFullscreenChange() {
  isFullscreen.value = document.fullscreenElement === graphRoot.value;
  nextTick(() => {
    fitView({ padding: 0.12 });
  });
}

onMounted(() => {
  document.addEventListener('fullscreenchange', onFullscreenChange);
});

onUnmounted(() => {
  document.removeEventListener('fullscreenchange', onFullscreenChange);
});

const getLayoutedElements = (inputNodes: any[], inputEdges: any[]) => {
  const dagreGraph = new dagre.graphlib.Graph();
  dagreGraph.setDefaultEdgeLabel(() => ({}));

  // 1. Split connected vs isolated nodes
  const connectedNodeIds = new Set<string>();
  inputEdges.forEach(edge => {
    connectedNodeIds.add(edge.source);
    connectedNodeIds.add(edge.target);
  });

  const connectedNodes: any[] = [];
  const isolatedNodes: any[] = [];

  inputNodes.forEach(node => {
    if (connectedNodeIds.has(node.id)) {
      connectedNodes.push(node);
    } else {
      isolatedNodes.push(node);
    }
  });

  // 2. Layout connected subgraph (LR)
  dagreGraph.setGraph({ rankdir: 'LR', align: 'UL', nodesep: 30, ranksep: 60 });

  connectedNodes.forEach((node) => {
    dagreGraph.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  });
  inputEdges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  // --- Normalize coordinates ---
  let minX = Infinity;
  let minY = Infinity;
  let maxGraphY = 0;

  connectedNodes.forEach(node => {
    const pos = dagreGraph.node(node.id);
    // Dagre pos.x is center; compute left edge
    const leftEdge = pos.x - NODE_WIDTH / 2;
    const topEdge = pos.y - NODE_HEIGHT / 2;

    if (leftEdge < minX) minX = leftEdge;
    if (topEdge < minY) minY = topEdge;
  });

  if (minX === Infinity) minX = 0;
  if (minY === Infinity) minY = 0;

  const layoutedConnected = connectedNodes.map((node) => {
    const pos = dagreGraph.node(node.id);
    const x = (pos.x - NODE_WIDTH / 2) - minX + CANVAS_PADDING_X;
    const y = (pos.y - NODE_HEIGHT / 2) - minY + CANVAS_PADDING_Y;

    if ((y + NODE_HEIGHT) > maxGraphY) maxGraphY = y + NODE_HEIGHT;

    return {
      ...node,
      data: { ...node.data, variant: 'chain' as const },
      position: { x, y },
    };
  });

  const sectionLabels: any[] = [];
  if (connectedNodes.length > 0) {
    sectionLabels.push({
      id: 'section-chain-label',
      type: 'section-label',
      data: { label: 'Dependency flow', hint: 'depends_on' },
      position: { x: CANVAS_PADDING_X, y: Math.max(CANVAS_PADDING_Y - 28, 8) },
      selectable: false,
      draggable: false,
      connectable: false,
      focusable: false,
      zIndex: 0,
    });
  }

  // 3. Layout isolated nodes in a left-aligned grid
  let startY = connectedNodes.length > 0 ? maxGraphY + 80 : CANVAS_PADDING_Y;

  if (isolatedNodes.length > 0 && connectedNodes.length > 0) {
    sectionLabels.push({
      id: 'section-standalone-label',
      type: 'section-label',
      data: { label: 'Standalone', hint: 'no dependencies' },
      position: { x: CANVAS_PADDING_X, y: startY - 28 },
      selectable: false,
      draggable: false,
      connectable: false,
      focusable: false,
      zIndex: 0,
    });
    startY += 8;
  } else if (isolatedNodes.length > 0) {
    sectionLabels.push({
      id: 'section-standalone-label',
      type: 'section-label',
      data: { label: 'Standalone', hint: 'no dependencies' },
      position: { x: CANVAS_PADDING_X, y: Math.max(CANVAS_PADDING_Y - 28, 8) },
      selectable: false,
      draggable: false,
      connectable: false,
      focusable: false,
      zIndex: 0,
    });
  }

  const COLUMNS = 6;
  const GRID_X_STEP = STANDALONE_WIDTH + GAP_X;
  const GRID_Y_STEP = STANDALONE_HEIGHT + GAP_Y;

  const layoutedIsolated = isolatedNodes.map((node, index) => {
    const col = index % COLUMNS;
    const row = Math.floor(index / COLUMNS);

    return {
      ...node,
      data: { ...node.data, variant: 'standalone' as const },
      position: {
        x: CANVAS_PADDING_X + (col * GRID_X_STEP),
        y: startY + row * GRID_Y_STEP,
      },
    };
  });

  const processNodes = [...layoutedConnected, ...layoutedIsolated].map((node) => ({
    ...node,
    zIndex: 1,
  }));

  return {
    nodes: [...sectionLabels, ...processNodes],
    edges: inputEdges,
  };
};

function syncNodeData(programs: Program[]) {
  const byId = new Map(programs.map(p => [p.id, p]));
  nodes.value = nodes.value.map((node) => {
    if (node.type !== 'process-node') return node;
    const p = byId.get(node.id);
    if (!p) return node;
    return {
      ...node,
      data: { ...node.data, status: p.status, fullData: p },
    };
  });
}

function programsLayoutKey(programs: Program[]) {
  return programs
    .map(p => `${p.id}:${p.name}:${(p.depends_on || []).join(',')}`)
    .sort()
    .join('|');
}

const lastLayoutKey = ref('');

function transformData() {
  const rawNodes: any[] = [];
  const rawEdges: any[] = [];
  const nameToIdMap = new Map<string, string>();

  props.programs.forEach(p => nameToIdMap.set(p.name, p.id));

  props.programs.forEach(p => {
    rawNodes.push({
      id: p.id,
      type: 'process-node',
      data: { status: p.status, fullData: p },
      position: { x: 0, y: 0 }
    });

    const deps = (p as any).depends_on || [];
    deps.forEach((depName: string) => {
      const targetId = nameToIdMap.get(depName);
      if (targetId) {
        rawEdges.push({
          id: `${targetId}-${p.id}`,
          source: targetId,
          target: p.id,
          type: 'smoothstep',
          animated: true,
          style: {
            stroke: isDark.value ? '#818cf8' : '#4338ca',
            strokeWidth: isFullscreen.value ? 2.5 : 2,
          },
        });
      }
    });
  });

  const layouted = getLayoutedElements(rawNodes, rawEdges);
  nodes.value = layouted.nodes;
  edges.value = layouted.edges;
  lastLayoutKey.value = programsLayoutKey(props.programs);

  // Reset viewport to (0,0); node coords already include padding
  nextTick(() => {
    setViewport({ x: 0, y: 0, zoom: 1 });
  });
}

onNodeClick(({ node }) => emit('node-click', node.id));

watch(
  () => props.programs,
  (programs) => {
    const key = programsLayoutKey(programs);
    if (key !== lastLayoutKey.value || nodes.value.length === 0) {
      transformData();
    } else {
      syncNodeData(programs);
    }
  },
  { deep: true, immediate: true },
);
watch(isDark, transformData);
watch(isFullscreen, () => {
  transformData();
});

defineExpose({
  resize: () => {
    fitView({ padding: 0.12 });
  },
  refresh: refreshGraph,
});
</script>

<template>
  <div
    ref="graphRoot"
    class="topology-graph-root w-full h-full relative font-sans"
    :class="{ 'topology-graph-root--fullscreen': isFullscreen }"
  >
    <VueFlow
      v-model:nodes="nodes"
      v-model:edges="edges"
      :node-types="nodeTypes as any"
      :default-viewport="{ zoom: 1 }"
      :min-zoom="0.1"
      :max-zoom="2"
      fit-view-on-init
    >
      <Background :pattern-color="backgroundPatternColor" :gap="20" :size="1" />
      <Controls
        :show-fit-view="false"
        class="!bg-card !border-border !shadow-lg !m-4 !rounded-lg overflow-hidden !bottom-4 !left-4"
      >
        <ControlButton title="Refresh layout" @click="refreshGraph">
          <RefreshCw class="topology-graph-control-icon" />
        </ControlButton>
        <ControlButton
          :title="isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'"
          @click="toggleFullscreen"
        >
          <Minimize2 v-if="isFullscreen" class="topology-graph-control-icon" />
          <Maximize2 v-else class="topology-graph-control-icon" />
        </ControlButton>
      </Controls>
      <MiniMap class="!bg-card !border-border !shadow-lg !rounded-lg !bottom-4 !right-4" :node-color="isDark ? '#374151' : '#e5e7eb'" :mask-color="isDark ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.6)'" />
    </VueFlow>

    <!-- Legend -->
    <div class="topology-graph-legend">
      <p class="topology-graph-legend__heading">Status</p>

      <div class="topology-graph-legend__item">
        <span class="topology-graph-legend__dot topology-graph-legend__dot--healthy" aria-hidden="true" />
        <span class="topology-graph-legend__label">Healthy</span>
      </div>

      <div class="topology-graph-legend__item topology-graph-legend__item--stacked">
        <div class="topology-graph-legend__row">
          <span class="topology-graph-legend__dot topology-graph-legend__dot--running" aria-hidden="true" />
          <span class="topology-graph-legend__label">Running</span>
        </div>
        <span class="topology-graph-legend__hint">Pending first check; failing checks show Unhealthy (red)</span>
      </div>

      <div class="topology-graph-legend__item">
        <span class="topology-graph-legend__dot topology-graph-legend__dot--waiting" aria-hidden="true" />
        <span class="topology-graph-legend__label">Waiting</span>
      </div>

      <div class="topology-graph-legend__item">
        <span class="topology-graph-legend__dot topology-graph-legend__dot--fatal" aria-hidden="true" />
        <span class="topology-graph-legend__label">Fatal</span>
      </div>

      <div class="topology-graph-legend__item">
        <span class="topology-graph-legend__dot topology-graph-legend__dot--stopped" aria-hidden="true" />
        <span class="topology-graph-legend__label topology-graph-legend__label--muted">Stopped</span>
      </div>

      <p class="topology-graph-legend__heading topology-graph-legend__heading--section">Layout</p>

      <div class="topology-graph-legend__item">
        <span class="topology-graph-legend__node-preview" aria-hidden="true" />
        <span class="topology-graph-legend__label">Process node</span>
      </div>
      <p class="topology-graph-legend__note">Solid arrows show <code>depends_on</code> between programs in the flow section.</p>
    </div>
  </div>
</template>

<style>
.topology-graph-legend {
  position: absolute;
  top: 1.5rem;
  right: 1.5rem;
  z-index: 10;
  width: 11.5rem;
  padding: 0.875rem 1rem;
  border-radius: 0.75rem;
  border: 1px solid rgb(229 231 235);
  background: rgb(255 255 255 / 0.96);
  color: rgb(55 65 81);
  box-shadow: 0 4px 16px rgb(0 0 0 / 0.08);
  font-size: 0.75rem;
  line-height: 1.35;
}

.topology-graph-legend__heading {
  margin: 0 0 0.625rem;
  padding-bottom: 0.5rem;
  border-bottom: 1px solid rgb(229 231 235);
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: rgb(107 114 128);
}

.topology-graph-legend__heading--section {
  margin-top: 0.75rem;
  padding-top: 0.625rem;
  border-top: 1px solid rgb(229 231 235);
  border-bottom: none;
  padding-bottom: 0;
}

.topology-graph-legend__item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.topology-graph-legend__item:last-of-type {
  margin-bottom: 0;
}

.topology-graph-legend__item--stacked {
  flex-direction: column;
  align-items: stretch;
  gap: 0.25rem;
}

.topology-graph-legend__row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.topology-graph-legend__dot {
  width: 0.625rem;
  height: 0.625rem;
  border-radius: 9999px;
  flex-shrink: 0;
}

.topology-graph-legend__dot--healthy { background: rgb(16 185 129); }
.topology-graph-legend__dot--running { background: rgb(245 158 11); }
.topology-graph-legend__dot--waiting { background: rgb(99 102 241); }
.topology-graph-legend__dot--fatal { background: rgb(225 29 72); }
.topology-graph-legend__dot--stopped { background: rgb(212 212 216); }

.topology-graph-legend__label {
  font-weight: 600;
  color: rgb(31 41 55);
}

.topology-graph-legend__label--muted {
  color: rgb(107 114 128);
  font-weight: 500;
}

.topology-graph-legend__hint {
  padding-left: 1.125rem;
  font-size: 0.75rem;
  line-height: 1.35;
  color: rgb(107 114 128);
}

.topology-graph-legend__node-preview {
  width: 1.25rem;
  height: 0.75rem;
  border-radius: 9999px;
  border: 1px solid rgb(212 212 216);
  background: rgb(244 244 245);
  flex-shrink: 0;
}

.topology-graph-legend__note {
  margin: 0.375rem 0 0;
  font-size: 0.75rem;
  line-height: 1.4;
  color: rgb(107 114 128);
}

.topology-graph-legend__note code {
  font-size: 0.75rem;
  color: rgb(79 70 229);
}

html.dark .topology-graph-legend {
  border-color: rgb(63 63 70);
  background: rgb(39 39 42 / 0.98);
  color: rgb(212 212 216);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
}

html.dark .topology-graph-legend__heading {
  border-color: rgb(63 63 70);
  color: rgb(161 161 170);
}

html.dark .topology-graph-legend__heading--section {
  border-top-color: rgb(63 63 70);
}

html.dark .topology-graph-legend__label {
  color: rgb(244 244 245);
}

html.dark .topology-graph-legend__label--muted,
html.dark .topology-graph-legend__hint,
html.dark .topology-graph-legend__note {
  color: rgb(161 161 170);
}

html.dark .topology-graph-legend__node-preview {
  border-color: rgb(82 82 91);
  background: rgb(63 63 70);
}

html.dark .topology-graph-legend__note code {
  color: rgb(165 180 252);
}

.topology-graph-root:fullscreen,
.topology-graph-root.topology-graph-root--fullscreen {
  width: 100%;
  height: 100%;
  background: rgb(244 244 245);
}

html.dark .topology-graph-root:fullscreen,
html.dark .topology-graph-root.topology-graph-root--fullscreen {
  background: rgb(24 24 27);
}

.topology-graph-root:fullscreen :deep(.vue-flow),
.topology-graph-root.topology-graph-root--fullscreen :deep(.vue-flow) {
  background: transparent;
}

html.dark .topology-graph-root:fullscreen :deep(.vue-flow),
html.dark .topology-graph-root.topology-graph-root--fullscreen :deep(.vue-flow) {
  background: transparent;
}

html.dark .topology-graph-root:fullscreen :deep(.section-label-node__title),
html.dark .topology-graph-root.topology-graph-root--fullscreen :deep(.section-label-node__title) {
  color: rgb(212 212 216);
}

html.dark .topology-graph-root:fullscreen :deep(.section-label-node__hint),
html.dark .topology-graph-root.topology-graph-root--fullscreen :deep(.section-label-node__hint) {
  color: rgb(161 161 170);
}

.topology-graph-root:fullscreen :deep(.section-label-node__title),
.topology-graph-root.topology-graph-root--fullscreen :deep(.section-label-node__title) {
  color: rgb(75 85 99);
}

.topology-graph-root:fullscreen :deep(.section-label-node__hint),
.topology-graph-root.topology-graph-root--fullscreen :deep(.section-label-node__hint) {
  color: rgb(107 114 128);
}

.topology-graph-control-icon {
  width: 16px;
  height: 16px;
  stroke: currentColor;
  fill: none;
}

.vue-flow__node-section-label {
  pointer-events: none !important;
}

.vue-flow__node-process-node {
  width: auto !important;
  height: auto !important;
  padding: 0 !important;
  border: none !important;
  background: transparent !important;
  box-shadow: none !important;
}

.vue-flow__edge-path {
  stroke-width: 1.5;
  transition: stroke 0.3s;
}

.topology-graph-root:fullscreen :deep(.vue-flow__edge-path),
.topology-graph-root.topology-graph-root--fullscreen :deep(.vue-flow__edge-path) {
  stroke-width: 2.5;
  filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.15));
}
.vue-flow__controls {
  box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1);
  border: 1px solid #e5e7eb;
}
.dark .vue-flow__controls {
  border-color: #374151;
  background: #1f2937;
}
.dark .vue-flow__controls-button {
  border-bottom: 1px solid #374151;
  fill: #f3f4f6;
}
.dark .vue-flow__controls-button:hover {
  background: #374151;
}
</style>
