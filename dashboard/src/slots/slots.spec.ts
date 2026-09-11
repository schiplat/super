/**
 * Slot contract tests — OSS CI guards the extension surface.
 *
 * A contributor who renames/drops a field on Program or moves the slot anchor
 * breaks every plugin UI; these tests fail the PR with a readable message so
 * no private context is needed to understand why.
 */
// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import { registerSlot, unregisterSlot, slotExtensions } from './registry';
import Slot from './Slot.vue';

const ProcCard = defineComponent({
  props: { context: { type: Object, default: undefined } },
  setup(props) {
    return () => h('button', { 'data-proc': JSON.stringify(props.context?.process ?? null) }, 'plugin');
  },
});

describe('slot registry', () => {
  it('registers and unregisters extensions', () => {
    const ext = { id: 't1', component: defineComponent({ render: () => h('span', 'x') }) };
    const off = registerSlot('process.actions', ext);
    expect(slotExtensions('process.actions')).toHaveLength(1);
    off();
    expect(slotExtensions('process.actions')).toHaveLength(0);
  });

  it('accepts nav.* slot names', () => {
    const ext = { id: 'nav1', component: defineComponent({ render: () => h('a', 'Tokens') }) };
    const off = registerSlot('nav.account', ext);
    expect(slotExtensions('nav.account')).toHaveLength(1);
    off();
  });

  it('renders nothing (no DOM) with zero registrations', () => {
    const wrapper = mount(Slot, { props: { name: 'process.actions' } });
    expect(wrapper.html()).toBe('');
  });
});

describe('contract: process.actions', () => {
  it('delivers id/name/status/pid on context.process', () => {
    const ext = { id: 'contract', component: ProcCard };
    const off = registerSlot('process.actions', ext);
    try {
      const proc = { id: 'p-1', pid: 1234, name: 'api', status: 'Running' };
      const wrapper = mount(Slot, {
        props: { name: 'process.actions', context: { process: proc } },
      });
      const delivered = JSON.parse(
        (wrapper.get('button').attributes('data-proc') as string) ?? 'null',
      );
      expect(delivered).toEqual({ id: 'p-1', pid: 1234, name: 'api', status: 'Running' });
    } finally {
      off();
    }
  });

  it('delivers the LIVE process object (same reference the view renders)', () => {
    const proc = { id: 'p-2', pid: 99, name: 'worker', status: 'Backoff' };
    const ext = { id: 'contract2', component: ProcCard };
    const off = registerSlot('process.actions', ext);
    try {
      const wrapper = mount(Slot, {
        props: { name: 'process.actions', context: { process: proc } },
      });
      const delivered = JSON.parse(wrapper.get('button').attributes('data-proc') as string);
      expect(delivered.id).toBe('p-2');
      expect(delivered.status).toBe('Backoff');
    } finally {
      off();
    }
  });

  it('contains a throwing extension: healthy extension still mounts (error swallowed by Slot)', async () => {
    const bad = {
      id: 'bad',
      component: defineComponent({
        setup() {
          throw new Error('boom');
        },
        render: () => h('span', 'never'),
      }),
    };
    const good = { id: 'good', component: ProcCard };
    const offBad = registerSlot('process.actions', bad);
    const offGood = registerSlot('process.actions', good);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    try {
      // mount must not throw even though one extension explodes in setup()
      const wrapper = mount(Slot, { props: { name: 'process.actions' } });
      await wrapper.vm.$nextTick();
      expect(warn).toHaveBeenCalled();
    } finally {
      warn.mockRestore();
      offBad();
      offGood();
    }
  });
});
