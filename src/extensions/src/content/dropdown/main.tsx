import { createRoot } from "react-dom/client";
import "../../ui/hardware";
import { loadTheme } from "../../ui/theme";
import { Dropdown } from "./Dropdown";

void loadTheme();
const token = new URLSearchParams(location.search).get("token") ?? "";
const root = document.getElementById("root");
if (root) createRoot(root).render(<Dropdown token={token} />);
