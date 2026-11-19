import { createRoot } from "react-dom/client";
import "../../ui/hardware";
import { loadTheme } from "../../ui/theme";
import { PasskeyPrompt } from "./PasskeyPrompt";
import { Prompt } from "./Prompt";

void loadTheme();
const params = new URLSearchParams(location.search);
const token = params.get("token") ?? "";
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    params.get("kind") === "passkey" ? <PasskeyPrompt token={token} /> : <Prompt token={token} />,
  );
}
