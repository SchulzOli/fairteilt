// Saves a JSON export (M7, DSGVO): in the app into the downloads folder (Rust command
// `save_file`), in the browser as a normal download. Returns where it went, for the message.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { session } from "$lib/session.svelte";

export async function saveExport(path: string, name: string): Promise<string> {
  const data = await session.request<unknown>("GET", path);
  const content = JSON.stringify(data, null, 2);
  const file = `${name}-${new Date().toISOString().slice(0, 10)}.json`;
  if (isTauri()) {
    return invoke<string>("save_file", { name: file, content });
  }
  const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = file;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  return `Downloads/${file}`;
}
