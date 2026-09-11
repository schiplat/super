import { createApp } from 'vue'
import './style.css'
import App from './App.vue'
import router from './router'
import { createPinia } from 'pinia'
import { useCapabilitiesStore } from '@/stores/capabilities'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)

// Discover plugins before first paint / route guards rely on ready flags.
void useCapabilitiesStore(pinia)
  .ensureDiscovered()
  .finally(() => {
    app.use(router)
    app.mount('#app')
  })
