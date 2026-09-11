import { createApp } from 'vue'
import './style.css'
import App from './App.vue'
import router from './router'
import { createPinia } from 'pinia'
import { useCapabilitiesStore } from '@/stores/capabilities'
import { installBridge } from '@/slots/bridge'
import { loadPluginUIs, loadDevPluginUI } from '@/slots/loader'
import { APP_VERSION } from '@/lib/version'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
// Bridge + router bind before plugins so registerRoute/addRoute works.
installBridge(app, APP_VERSION, router)

// Discover plugins before first paint / route guards rely on ready flags.
void useCapabilitiesStore(pinia)
  .ensureDiscovered()
  .then(async () => {
    // Plugin UI bundles self-register into slots + routes; must land before
    // mount so the first paint already includes injected actions/nav/pages.
    await loadPluginUIs()
    await loadDevPluginUI()
  })
  .finally(() => {
    app.use(router)
    app.mount('#app')
  })
