import { createRoot } from "react-dom/client";
import "../../ui/kandado";
import { loadTheme } from "../../ui/theme";
import { Prompt } from "./Prompt";

void loadTheme();
const token = new URLSearchParams(location.search).get("token") ?? "";
const root = document.getElementById("root");
if (root) createRoot(root).render(<Prompt token={token} />);
