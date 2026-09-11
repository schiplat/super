import { createRouter, createWebHistory } from 'vue-router'
import MainLayout from '@/layout/MainLayout.vue'
import ProcessList from '@/views/ProcessList.vue'
import Login from '@/views/Login.vue'
import { useCapabilitiesStore } from '@/stores/capabilities'

// Lazy-loaded routes (OSS shell only — Tokens / Notify come from the Pro UI plugin)
const License = () => import('@/views/License.vue')
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
      meta: { requiresAuth: true },
    },
    {
      path: '/error/:code',
      name: 'Error',
      component: ErrorPage,
      meta: { title: 'Error' }
    },
    {
      path: '/',
      name: 'shell',
      component: MainLayout,
      children: [
        {
          path: '',
          name: 'Dashboard',
          component: ProcessList
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
  if (to.meta.requiresAuth && !caps.auth) {
    return { path: '/' }
  }

  // Auth gate active: require a session for app pages (not login/error).
  if (
    caps.auth &&
    !localStorage.getItem('super_token') &&
    to.path !== '/login' &&
    !to.path.startsWith('/error')
  ) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }

  return true
})

export default router
