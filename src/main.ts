import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { i18n } from './i18n'
import { initZLog } from './lib/z-log'
import '@fontsource-variable/inter'
import './assets/styles.css'

initZLog()

createApp(App).use(createPinia()).use(i18n).mount('#app')
