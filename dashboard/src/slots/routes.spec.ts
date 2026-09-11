/**
 * Plugin route registry tests.
 */
// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { createMemoryHistory, createRouter } from 'vue-router';
import { defineComponent, h } from 'vue';
import { bindRouter, registerRoute } from './routes';

const Page = defineComponent({
  render: () => h('div', 'plugin-page'),
});

const Shell = defineComponent({
  render: () => h('div', [h('router-view')]),
});

describe('registerRoute', () => {
  it('adds a child under the shell parent', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        {
          path: '/',
          name: 'shell',
          component: Shell,
          children: [{ path: '', name: 'home', component: defineComponent({ render: () => h('div') }) }],
        },
      ],
    });
    bindRouter(router);
    const off = registerRoute({
      path: 'tokens',
      name: 'Tokens',
      component: Page,
      meta: { requiresSecurity: true },
    });
    expect(router.hasRoute('Tokens')).toBe(true);
    await router.push('/tokens');
    expect(router.currentRoute.value.name).toBe('Tokens');
    off();
    expect(router.hasRoute('Tokens')).toBe(false);
  });
});
