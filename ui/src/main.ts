import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { applyTheme, readTheme } from './lib/theme'

// Before the first paint, so a forced theme does not flash the other one.
applyTheme({ theme: readTheme(), root: document.documentElement })

mount(App, { target: document.getElementById('app')! })
