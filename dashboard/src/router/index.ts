import { createRouter, createWebHistory } from 'vue-router'
import MainLayout from '@/layout/MainLayout.vue'
import ProcessList from '@/views/ProcessList.vue'
import Login from '@/views/Login.vue'
import { useCapabilitiesStore } from '@/stores/capabilities'

// Lazy-loaded routes
const Tokens = () => import('@/views/Tokens.vue')
const License = () => import('@/views/License.vue')
const NotifyLayout = () => import('@/views/Settings/notify/NotifyLayout.vue')
const NotifyWebhooks = () => import('@/views/Settings/notify/NotifyWebhooks.vue')
const NotifyRules = () => import('@/views/Settings/notify/NotifyRules.vue')
const NotifyDelivery = () => import('@/views/Settings/notify/NotifyDelivery.vue')
const StackEditor = () => import('@/views/StackEditor.vue')
const ProgramCreate = () => import('@/views/ProgramCreate.vue')
const ProgramEdit = () => import('@/views/ProgramEdit.vue');

const ErrorPage = () => import('@/views/ErrorPage.vue')

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      name: 'Login',
      component: Login,
      meta: { requiresSecurity: true },
    },
    {
      path: '/error/:code',
      name: 'Error',
      component: ErrorPage,
      meta: { title: 'Error' }
    },
    {
      path: '/',
      component: MainLayout,
      children: [
        {
          path: '',
          name: 'Dashboard',
          component: ProcessList
        },
        {
          path: 'tokens',
          name: 'Tokens',
          component: Tokens,
          meta: { requiresSecurity: true },
        },
        {
          path: 'license',
          name: 'License',
          component: License
        },
        {
          path: 'stack',
          name: 'StackEditor',
          component: StackEditor
        },
        {
          path: 'settings/notify',
          component: NotifyLayout,
          meta: { requiresNotify: true },
          children: [
            {
              path: '',
              redirect: '/settings/notify/webhooks',
            },
            {
              path: 'webhooks',
              name: 'NotifyWebhooks',
              component: NotifyWebhooks,
              meta: { requiresNotify: true },
            },
            {
              path: 'rules',
              name: 'NotifyRules',
              component: NotifyRules,
              meta: { requiresNotify: true },
            },
            {
              path: 'delivery',
              name: 'NotifyDelivery',
              component: NotifyDelivery,
              meta: { requiresNotify: true },
            },
          ],
        },
        {
          path: 'programs/new',
          name: 'ProgramCreate',
          component: ProgramCreate
        },
        {
          path: 'programs/:id/edit',
          name: 'ProgramEdit',
          component: ProgramEdit
        },
      ]
    },

    // Catch-all for undefined routes
    {
      path: '/:pathMatch(.*)*',
      redirect: '/error/404'
    }
  ]
})

router.beforeEach(async (to) => {
  const caps = useCapabilitiesStore()
  await caps.ensureDiscovered()

  if (to.meta.requiresNotify && !caps.notify) {
    return { path: '/' }
  }
  if (to.meta.requiresSecurity && !caps.security) {
    return { path: '/' }
  }

  // Licensed + security: require a session for app pages (not login/error).
  if (
    caps.security &&
    !localStorage.getItem('super_token') &&
    to.path !== '/login' &&
    !to.path.startsWith('/error')
  ) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }

  return true
})

export default router
