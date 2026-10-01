import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// Pas de menu contextuel « navigateur » en production : on est une vraie app.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => {
    const t = e.target as HTMLElement;
    if (!t.closest("input, textarea")) e.preventDefault();
  });
}

const app = mount(App, { target: document.getElementById("app")! });

export default app;
