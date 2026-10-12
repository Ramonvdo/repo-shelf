import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

type Category = { name: string; count: number };
type Repo = {
  id: number;
  full_name: string;
  category: string;
  note: string;
  description: string | null;
  stars: number;
  topics: string[];
};

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const form = $<HTMLFormElement>("add-form");
const linkInput = $<HTMLInputElement>("link");
const drawerInput = $<HTMLInputElement>("drawer-name");
const addButton = $<HTMLButtonElement>("add-button");
const drawerNames = $<HTMLDataListElement>("drawer-names");
const drawersList = $<HTMLUListElement>("drawers");
const drawerTitle = $<HTMLHeadingElement>("drawer-title");
const removeDrawer = $<HTMLButtonElement>("remove-drawer");
const partsList = $<HTMLUListElement>("parts");
const empty = $<HTMLDivElement>("empty");
const sheet = $<HTMLElement>("sheet");
const sheetTitle = $<HTMLHeadingElement>("sheet-title");
const sheetBody = $<HTMLDivElement>("sheet-body");
const openConnect = $<HTMLButtonElement>("open-connect");
const openToken = $<HTMLButtonElement>("open-token");
const tokenLabel = $<HTMLSpanElement>("token-label");
const statusLine = $<HTMLParagraphElement>("status");
const linkError = $<HTMLParagraphElement>("link-error");

/** The open drawer; null shows every repo. */
let open: string | null = null;
let categories: Category[] = [];
let justFiled: number | null = null;

const stars = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { class?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) node.className = cls;
  Object.assign(node, rest);
  node.append(...children);
  return node;
}

function icon(paths: string): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = paths;
  return svg;
}

const ICON = {
  star: '<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
  copy: '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
};

function say(message: string, tone: "ok" | "error" = "ok") {
  statusLine.textContent = message;
  statusLine.dataset.tone = tone;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    say(String(e), "error");
    return undefined;
  }
}

// ---- drawers --------------------------------------------------------------

function drawerButton(label: string, count: number, name: string | null): HTMLLIElement {
  const button = el(
    "button",
    { type: "button", class: "drawer-front" },
    el("span", { class: "slot" }, el("span", { class: "label" }, label), el("span", { class: "count" }, String(count))),
  );
  if (open === name) button.setAttribute("aria-current", "true");
  button.addEventListener("click", () => openDrawer(name));
  return el("li", {}, button);
}

function renderDrawers() {
  const total = categories.reduce((n, c) => n + c.count, 0);
  drawersList.replaceChildren(
    drawerButton("All", total, null),
    ...categories.map((c) => drawerButton(c.name, c.count, c.name)),
  );
  drawerNames.replaceChildren(...categories.map((c) => el("option", { value: c.name })));
}

async function openDrawer(name: string | null) {
  open = name;
  if (name) drawerInput.value = name;
  await refresh();
}

// ---- parts ----------------------------------------------------------------

function moveSelect(repo: Repo): HTMLLabelElement {
  const select = el("select", { class: "move" });
  for (const c of categories) {
    select.append(el("option", { value: c.name, textContent: c.name, selected: c.name === repo.category }));
  }
  select.addEventListener("change", async () => {
    await call("move_repo", { id: repo.id, category: select.value });
    say(`Moved ${repo.full_name} to ${select.value}.`);
    await refresh();
  });
  return el("label", { class: "move-label", title: "Move to another category" },
    el("span", { class: "visually-hidden" }, `Category for ${repo.full_name}`),
    select,
  );
}

function removeButton(repo: Repo): HTMLButtonElement {
  const button = el("button", { type: "button", class: "icon-button remove" }, icon(ICON.trash));
  button.setAttribute("aria-label", `Remove ${repo.full_name}`);
  let armed: number | undefined;
  button.addEventListener("click", async () => {
    if (armed === undefined) {
      button.classList.add("armed");
      button.replaceChildren("Remove?");
      armed = window.setTimeout(() => {
        armed = undefined;
        button.classList.remove("armed");
        button.replaceChildren(icon(ICON.trash));
      }, 3000);
      return;
    }
    window.clearTimeout(armed);
    await call("delete_repo", { id: repo.id });
    say(`Removed ${repo.full_name}.`);
    await refresh();
  });
  return button;
}

function noteField(repo: Repo): HTMLInputElement {
  const input = el("input", {
    type: "text",
    class: "note",
    value: repo.note,
    placeholder: "Add a note for your agent",
    spellcheck: true,
  });
  input.setAttribute("aria-label", `Note for ${repo.full_name}`);
  const save = async () => {
    if (input.value.trim() === repo.note) return;
    await call("set_note", { id: repo.id, note: input.value });
    repo.note = input.value.trim();
    say(`Saved note on ${repo.full_name}.`);
  };
  input.addEventListener("blur", save);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") input.blur();
    if (e.key === "Escape") {
      input.value = repo.note;
      input.blur();
    }
  });
  return input;
}

function part(repo: Repo): HTMLLIElement {
  const [owner, name] = repo.full_name.split("/");
  const tape = el("a", { class: "tape", href: `https://github.com/${repo.full_name}` },
    el("span", { class: "owner" }, `${owner}/`),
    name,
  );
  tape.addEventListener("click", (e) => {
    e.preventDefault();
    openUrl(tape.href);
  });

  const count = el("span", { class: "stars" }, icon(ICON.star), stars.format(repo.stars));
  count.setAttribute("aria-label", `${repo.stars.toLocaleString("en")} stars`);

  const li = el("li", { class: "part" },
    el("div", { class: "part-head" }, tape, count),
    el("p", { class: "desc" }, repo.description || "No description on GitHub."),
    el("div", { class: "part-foot" }, noteField(repo), moveSelect(repo), removeButton(repo)),
  );
  if (repo.id === justFiled) li.classList.add("filed");
  return li;
}

async function refresh() {
  const [cats, repos] = await Promise.all([
    call<Category[]>("list_categories"),
    call<Repo[]>("list_repos", { category: open }),
  ]);
  categories = cats ?? [];
  if (open && !categories.some((c) => c.name.toLowerCase() === open!.toLowerCase())) open = null;
  renderDrawers();

  const list = repos ?? [];
  drawerTitle.textContent = open ?? "All repos";
  partsList.replaceChildren(...list.map(part));
  removeDrawer.hidden = !(open && list.length === 0);

  empty.hidden = list.length > 0;
  if (categories.length === 0) {
    empty.replaceChildren(
      el("p", { class: "empty-title" }, "The shelf is empty."),
      el("p", {}, "Paste a GitHub link above, give it a category, and press Enter. Then use Connect agent below so your agent can read it."),
    );
  } else if (list.length === 0) {
    empty.replaceChildren(el("p", {}, "Nothing in this category yet."));
  }
  justFiled = null;
}

// ---- filing ---------------------------------------------------------------

function showLinkError(message: string | null) {
  linkError.textContent = message ?? "";
  linkError.hidden = message === null;
  if (message === null) linkInput.removeAttribute("aria-invalid");
  else linkInput.setAttribute("aria-invalid", "true");
}

linkInput.addEventListener("input", () => showLinkError(null));

form.addEventListener("submit", async (e) => {
  e.preventDefault();
  const link = linkInput.value.trim();
  const category = drawerInput.value.trim();
  if (!link || !category) return;
  addButton.disabled = true;
  addButton.textContent = "Filing…";
  say(`Fetching ${link} from GitHub…`);
  let repo: Repo;
  try {
    repo = await invoke<Repo>("add_repo", { link, category });
  } catch (err) {
    say("");
    showLinkError(String(err));
    return;
  } finally {
    addButton.disabled = false;
    addButton.textContent = "File it";
  }
  justFiled = repo.id;
  linkInput.value = "";
  say(`Filed ${repo.full_name} in ${repo.category}.`);
  if (open !== null && open.toLowerCase() !== repo.category.toLowerCase()) open = repo.category;
  await refresh();
  linkInput.focus();
});

removeDrawer.addEventListener("click", async () => {
  if (!open) return;
  const name = open;
  await call("delete_category", { name });
  say(`Removed the ${name} category.`);
  open = null;
  await refresh();
});

// ---- sheet: connect agent and token ----------------------------------------

function copyRow(label: string, text: string): HTMLDivElement {
  const button = el("button", { type: "button", class: "icon-button copy" }, icon(ICON.copy));
  button.setAttribute("aria-label", `Copy ${label}`);
  button.addEventListener("click", async () => {
    await navigator.clipboard.writeText(text);
    button.replaceChildren(icon(ICON.check));
    say(`Copied ${label}.`);
    setTimeout(() => button.replaceChildren(icon(ICON.copy)), 1500);
  });
  return el("div", { class: "snippet" },
    el("p", { class: "snippet-label" }, label),
    el("div", { class: "snippet-body" }, el("pre", {}, el("code", {}, text)), button),
  );
}

function showSheet(title: string, trigger: HTMLButtonElement, ...body: Node[]) {
  for (const b of [openConnect, openToken]) b.setAttribute("aria-expanded", String(b === trigger));
  sheetTitle.textContent = title;
  sheetBody.replaceChildren(...body);
  sheet.hidden = false;
  (sheet.querySelector("input, button.copy") as HTMLElement | null)?.focus();
}

function closeSheet() {
  const trigger = [openConnect, openToken].find((b) => b.getAttribute("aria-expanded") === "true");
  sheet.hidden = true;
  openConnect.setAttribute("aria-expanded", "false");
  openToken.setAttribute("aria-expanded", "false");
  trigger?.focus();
}

openConnect.addEventListener("click", async () => {
  if (!sheet.hidden && openConnect.getAttribute("aria-expanded") === "true") return closeSheet();
  const info = await call<{ exe: string }>("connect_info");
  if (!info) return;
  const exe = info.exe;
  const json = JSON.stringify({ mcpServers: { "repo-shelf": { command: exe, args: ["mcp"] } } }, null, 2);
  showSheet(
    "Connect an agent",
    openConnect,
    el("p", { class: "sheet-note" },
      "Run once. The agent starts repo-shelf in the background when it needs the shelf; this window can stay closed."),
    copyRow("Claude Code", `claude mcp add --scope user repo-shelf -- "${exe}" mcp`),
    copyRow("Codex (~/.codex/config.toml)", `[mcp_servers.repo-shelf]\ncommand = '${exe}'\nargs = ["mcp"]`),
    copyRow("Other MCP clients (JSON)", json),
  );
});

async function renderTokenLabel() {
  const has = await call<boolean>("has_token");
  tokenLabel.textContent = has ? "GitHub token saved" : "GitHub token";
  return has ?? false;
}

openToken.addEventListener("click", async () => {
  if (!sheet.hidden && openToken.getAttribute("aria-expanded") === "true") return closeSheet();
  const has = await renderTokenLabel();
  const input = el("input", {
    type: "password",
    id: "token",
    placeholder: has ? "A token is saved. Paste a new one to replace it." : "github_pat_…",
    spellcheck: false,
  });
  const tokenForm = el("form", { class: "token-form" },
    el("label", { htmlFor: "token", class: "visually-hidden" }, "GitHub token"),
    input,
    el("button", { type: "submit", class: "primary" }, "Save"),
  );
  tokenForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!input.value.trim()) return;
    await call("set_token", { token: input.value });
    input.value = "";
    await renderTokenLabel();
    say("Token saved.");
    closeSheet();
  });
  const create = el("a", { href: "https://github.com/settings/personal-access-tokens/new" }, "Create a token on GitHub");
  create.addEventListener("click", (e) => {
    e.preventDefault();
    openUrl(create.href);
  });
  const parts: Node[] = [
    el("p", { class: "sheet-note" },
      "Optional. Without a token GitHub allows 60 requests an hour; with one, 5,000, and agents can use code search. A token with no extra permissions is enough for public repos. It is stored only on this computer. ",
      create, "."),
    tokenForm,
  ];
  if (has) {
    const clear = el("button", { type: "button", class: "quiet" }, "Remove saved token");
    clear.addEventListener("click", async () => {
      await call("set_token", { token: "" });
      await renderTokenLabel();
      say("Token removed.");
      closeSheet();
    });
    parts.push(clear);
  }
  showSheet("GitHub token", openToken, ...parts);
});

$<HTMLButtonElement>("sheet-close").addEventListener("click", closeSheet);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !sheet.hidden) closeSheet();
});

// ---- start ----------------------------------------------------------------

window.addEventListener("DOMContentLoaded", async () => {
  await refresh();
  await renderTokenLabel();
  if (categories.length > 0) drawerInput.value = categories[0].name;
  linkInput.focus();
});
