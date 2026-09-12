/**
 * Slot / UI-bridge contract tests — **OSS CI gate**.
 *
 * DO NOT weaken, skip, rename, or “simplify” these assertions unless you fully
 * understand the paid-plugin surface. Subscription UI (`super-pro` `ui` plugin)
 * registers against these slot names and reads `context.process.{id,name,status,pid}`
 * (and detail-tab / nav helpers). Breaking this contract silently disables Pro
 * extensions for customers; CI is meant to fail the PR in public with a clear
 * message — not to be edited around.
 *
 * Allowed changes: adding NEW optional fields / NEW slot names (extend tests).
 * Forbidden without maintainer review: removing required fields, renaming slots,
 * changing mount-site `:context` shapes, deleting these tests.
 */
// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h } from 'vue';
import { registerSlot, slotExtensions, type ProcessContext } from './registry';
import Slot from './Slot.vue';

const here = dirname(fileURLToPath(import.meta.url));
const srcRoot = join(here, '..');

function readSrc(rel: string): string {
  return readFileSync(join(srcRoot, rel), 'utf8');
}

/** Fields Pro / third-party slot extensions may rely on. */
const REQUIRED_PROCESS_FIELDS: (keyof ProcessContext)[] = [
  'id',
  'name',
  'status',
  'pid',
];

const ProcCard = defineComponent({
  props: { context: { type: Object, default: undefined } },
  setup(props) {
    return () =>
      h(
        'button',
        { 'data-proc': JSON.stringify(props.context?.process ?? null) },
        'plugin',
      );
  },
});

describe('slot registry', () => {
  it('registers and unregisters extensions', () => {
    const ext = {
      id: 't1',
      component: defineComponent({ render: () => h('span', 'x') }),
    };
    const off = registerSlot('process.actions', ext);
    expect(slotExtensions('process.actions')).toHaveLength(1);
    off();
    expect(slotExtensions('process.actions')).toHaveLength(0);
  });

  it('accepts nav.* slot names', () => {
    const ext = {
      id: 'nav1',
      component: defineComponent({ render: () => h('a', 'Tokens') }),
    };
    const off = registerSlot('nav.account', ext);
    expect(slotExtensions('nav.account')).toHaveLength(1);
    off();
  });

  it('renders nothing (no DOM) with zero registrations', () => {
    const wrapper = mount(Slot, { props: { name: 'process.actions' } });
    expect(wrapper.html()).toBe('');
  });
});

describe('contract: process.actions context', () => {
  it('delivers id/name/status/pid on context.process', () => {
    const ext = { id: 'contract', component: ProcCard };
    const off = registerSlot('process.actions', ext);
    try {
      const proc: ProcessContext = {
        id: 'p-1',
        pid: 1234,
        name: 'api',
        status: 'Running',
      };
      const wrapper = mount(Slot, {
        props: { name: 'process.actions', context: { process: proc } },
      });
      const delivered = JSON.parse(
        (wrapper.get('button').attributes('data-proc') as string) ?? 'null',
      );
      for (const key of REQUIRED_PROCESS_FIELDS) {
        expect(delivered, `context.process.${key} must be present`).toHaveProperty(
          key,
        );
      }
      expect(delivered).toEqual({
        id: 'p-1',
        pid: 1234,
        name: 'api',
        status: 'Running',
      });
    } finally {
      off();
    }
  });

  it('contains a throwing extension: shell must not white-screen', async () => {
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
      const wrapper = mount(Slot, { props: { name: 'process.actions' } });
      await wrapper.vm.$nextTick();
      expect(warn).toHaveBeenCalled();
      expect(wrapper.find('button').exists()).toBe(true);
    } finally {
      warn.mockRestore();
      offBad();
      offGood();
    }
  });
});

describe('contract: mount sites (source) — do not change without review', () => {
  it('ProcessList passes the live program as context.process', () => {
    const src = readSrc('views/ProcessList.vue');
    expect(src).toMatch(/name=["']process\.actions["']/);
    expect(src).toMatch(
      /:context=["']\{\s*process:\s*proc\s*\}["']|:context="\{\s*process:\s*proc\s*\}"/,
    );
  });

  it('ProcessDetailDrawer exposes process + registerTab + activeTab', () => {
    const src = readSrc('components/ProcessDetailDrawer.vue');
    expect(src).toMatch(/name=["']process\.detail\.tabs["']/);
    expect(src).toMatch(/process:\s*summaryData/);
    expect(src).toMatch(/registerTab/);
    expect(src).toMatch(/activeTab/);
  });

  it('nav slots keep navigate/close helpers', () => {
    const layout = readSrc('layout/MainLayout.vue');
    expect(layout).toMatch(/name=["']nav\.manage["']/);
    expect(layout).toMatch(/name=["']nav\.mobile["']/);
    expect(layout).toMatch(/navigate:\s*navigateTo/);

    const nav = readSrc('extensions/NavbarRight.vue');
    expect(nav).toMatch(/name=["']nav\.account["']/);
    expect(nav).toMatch(/navigate:\s*navigateTo/);
  });
});

describe('contract: @super/ui-bridge types — do not change without review', () => {
  it('ProcessContext documents id/name/status/pid', () => {
    const bridge = readSrc('slots/ui-bridge.d.ts');
    for (const field of ['id: string', 'name: string', 'status: string', 'pid?: number']) {
      expect(bridge, `ui-bridge ProcessContext must keep ${field}`).toContain(field);
    }
    for (const name of [
      'process.actions',
      'process.detail.tabs',
      'nav.account',
      'nav.manage',
      'nav.mobile',
    ]) {
      expect(bridge, `SlotName must include ${name}`).toContain(`'${name}'`);
    }
  });

  it('Program type still exposes the same identity fields', () => {
    const types = readSrc('types/index.ts');
    expect(types).toMatch(/export interface Program/);
    expect(types).toMatch(/id:\s*string/);
    expect(types).toMatch(/name:\s*string/);
    expect(types).toMatch(/status:\s*ProcessStatus/);
    expect(types).toMatch(/pid\?:\s*number/);
  });
});
