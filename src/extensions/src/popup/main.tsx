import { createRoot } from "react-dom/client";
import "../ui/kandado";
import { loadTheme } from "../ui/theme";
import { App } from "./App";

void loadTheme();
const root = document.getElementById("root");
if (root) createRoot(root).render(<App />);
