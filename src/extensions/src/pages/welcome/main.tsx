import { createRoot } from "react-dom/client";
import "../../ui/kandado";
import { loadTheme } from "../../ui/theme";
import { WelcomeApp } from "./WelcomeApp";

void loadTheme();
const root = document.getElementById("root");
if (root) createRoot(root).render(<WelcomeApp />);
