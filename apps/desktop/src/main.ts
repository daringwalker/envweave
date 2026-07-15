import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";

const savedTheme = localStorage.getItem("envweave.theme");
const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
document.documentElement.dataset.theme = savedTheme === "dark"
  || (savedTheme !== "light" && prefersDark)
  ? "dark"
  : "light";

createApp(App).mount("#app");
