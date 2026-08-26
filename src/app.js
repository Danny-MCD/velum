const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const state = {
  sites: [],
  selectedId: null,
  torStatus: { state: "stopped" },
  newSiteMode: "static",
};

const el = (id) => document.getElementById(id);

// ---------- Tor status ----------

function describeStatus(status) {
  switch (status.state) {
    case "stopped":
      return { cls: "stopped", label: "Tor is stopped", sub: "Sites are offline" };
    case "starting":
      return { cls: "starting", label: "Starting Tor…", sub: "This can take a few seconds" };
    case "bootstrapping":
      return { cls: "bootstrapping", label: `Connecting to Tor… ${status.percent}%`, sub: "Building circuits" };
    case "running":
      return { cls: "running", label: "Tor is connected", sub: "Ready to publish" };
    case "failed":
      return { cls: "failed", label: "Tor couldn't start", sub: status.message || "See logs for details" };
    default:
      return { cls: "stopped", label: "Unknown", sub: "" };
  }
}

function renderTorStatus() {
  const { cls, label, sub } = describeStatus(state.torStatus);
  el("status-dot").className = `status-dot ${cls}`;
  el("status-label").textContent = label;
  el("status-sub").textContent = sub;
  renderDetail(); // reachability note depends on tor status too
}

// ---------- Sidebar / site list ----------

function renderSiteList() {
  const list = el("site-list");
  list.innerHTML = "";

  if (state.sites.length === 0) {
    const empty = document.createElement("div");
    empty.className = "site-list-empty";
    empty.textContent = "No sites yet";
    list.appendChild(empty);
    return;
  }

  for (const site of state.sites) {
    const btn = document.createElement("button");
    btn.className = `site-item${site.running ? " live" : ""}${site.id === state.selectedId ? " active" : ""}`;
    btn.innerHTML = `<span class="dot"></span><span class="site-item-name"></span>`;
    btn.querySelector(".site-item-name").textContent = site.name;
    btn.addEventListener("click", () => selectSite(site.id));
    list.appendChild(btn);
  }
}

function selectSite(id) {
  state.selectedId = id;
  renderSiteList();
  renderDetail();
}

// ---------- Detail view ----------

function currentSite() {
  return state.sites.find((s) => s.id === state.selectedId) || null;
}

async function renderDetail() {
  const site = currentSite();
  const empty = el("empty-state");
  const detail = el("site-detail");

  if (!site) {
    empty.hidden = false;
    detail.hidden = true;
    return;
  }
  empty.hidden = true;
  detail.hidden = false;

  el("detail-name").textContent = site.name;
  const badge = el("detail-badge");
  badge.textContent = site.running ? "Live" : site.enabled ? "Reconnecting…" : "Draft";
  badge.className = `badge${site.running ? " live" : ""}`;

  const toggleBtn = el("btn-toggle-publish");
  toggleBtn.textContent = site.enabled ? "Unpublish" : "Publish";
  toggleBtn.onclick = () => (site.enabled ? unpublish(site.id) : publish(site.id));

  const addressCard = el("address-card");
  if (site.onion_address) {
    addressCard.hidden = false;
    el("onion-address").textContent = site.onion_address;
    const note = el("reachability-note");
    if (state.torStatus.state === "running" && site.running) {
      note.textContent = "Reachable over Tor.";
    } else if (!site.running) {
      note.textContent = "Not currently published.";
    } else {
      note.textContent = "Waiting for Tor to finish connecting…";
    }
    renderQr(site.onion_address);
  } else {
    addressCard.hidden = true;
  }

  const sourceEl = el("source-summary");
  if (site.mode.kind === "static") {
    sourceEl.innerHTML = `Serving folder <code></code>`;
    sourceEl.querySelector("code").textContent = site.mode.folder;
  } else {
    sourceEl.innerHTML = `Pointing at <code></code>`;
    sourceEl.querySelector("code").textContent = `127.0.0.1:${site.mode.local_port}`;
  }

  // Reset key reveal state whenever the selected site changes.
  el("key-value").textContent = "";
  el("key-value").classList.add("hidden-key");
  el("btn-reveal-key").classList.remove("hidden");
  el("btn-copy-key").classList.add("hidden");

  el("btn-unpublish").disabled = !site.enabled;
  el("btn-unpublish").onclick = () => unpublish(site.id);
  el("btn-delete-site").onclick = () => deleteSite(site.id);
}

async function renderQr(address) {
  try {
    const svg = await invoke("onion_qr_svg", { address });
    el("qr-wrap").innerHTML = svg;
  } catch {
    el("qr-wrap").innerHTML = "";
  }
}

// ---------- Actions ----------

async function refreshSites() {
  state.sites = await invoke("list_sites");
  if (!state.selectedId && state.sites.length > 0) {
    state.selectedId = state.sites[0].id;
  }
  renderSiteList();
  renderDetail();
}

async function publish(id) {
  setBusy(true, "Publishing…");
  try {
    await invoke("publish_site", { id });
    await refreshSites();
    showToast("Site published");
  } catch (e) {
    showToast(`Couldn't publish: ${e}`, true);
  } finally {
    setBusy(false);
  }
}

async function unpublish(id) {
  setBusy(true, "Unpublishing…");
  try {
    await invoke("unpublish_site", { id });
    await refreshSites();
    showToast("Site unpublished");
  } catch (e) {
    showToast(`Couldn't unpublish: ${e}`, true);
  } finally {
    setBusy(false);
  }
}

async function deleteSite(id) {
  const site = state.sites.find((s) => s.id === id);
  const ok = confirm(
    `Delete "${site?.name ?? "this site"}"? This removes its saved key. If it's published, you'll permanently lose this .onion address unless you already backed the key up.`
  );
  if (!ok) return;
  try {
    await invoke("delete_site", { id });
    state.selectedId = null;
    await refreshSites();
    showToast("Site deleted");
  } catch (e) {
    showToast(`Couldn't delete: ${e}`, true);
  }
}

function setBusy(busy, label) {
  const toggleBtn = el("btn-toggle-publish");
  toggleBtn.disabled = busy;
  if (busy) toggleBtn.dataset.prevLabel = toggleBtn.textContent, (toggleBtn.textContent = label);
}

// ---------- New site modal ----------

function openNewSiteModal() {
  el("input-name").value = "";
  el("input-folder").value = "";
  el("input-port").value = "";
  setNewSiteMode("static");
  el("modal-backdrop").classList.remove("hidden");
  el("input-name").focus();
}

function closeNewSiteModal() {
  el("modal-backdrop").classList.add("hidden");
}

function setNewSiteMode(mode) {
  state.newSiteMode = mode;
  el("mode-static").classList.toggle("active", mode === "static");
  el("mode-existing").classList.toggle("active", mode === "existing");
  el("mode-static").setAttribute("aria-selected", mode === "static");
  el("mode-existing").setAttribute("aria-selected", mode === "existing");
  el("mode-static-panel").hidden = mode !== "static";
  el("mode-existing-panel").hidden = mode !== "existing";
}

async function submitNewSite() {
  const name = el("input-name").value.trim() || "Untitled site";
  let mode;
  if (state.newSiteMode === "static") {
    const folder = el("input-folder").value.trim();
    if (!folder) return showToast("Choose a folder first", true);
    mode = { kind: "static", folder };
  } else {
    const port = parseInt(el("input-port").value, 10);
    if (!port || port < 1 || port > 65535) return showToast("Enter a valid port", true);
    mode = { kind: "existing", local_port: port };
  }

  try {
    const site = await invoke("create_site", { name, mode });
    closeNewSiteModal();
    await refreshSites();
    selectSite(site.id);
    showToast("Site created - hit Publish when you're ready");
  } catch (e) {
    showToast(`Couldn't create site: ${e}`, true);
  }
}

// ---------- Toast ----------

let toastTimer = null;
function showToast(message, isError = false) {
  const toast = el("toast");
  toast.textContent = message;
  toast.style.borderColor = isError ? "var(--danger)" : "var(--border)";
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.hidden = true), 3200);
}

// ---------- Wiring ----------

function wireEvents() {
  el("btn-new-site").addEventListener("click", openNewSiteModal);
  el("btn-empty-new-site").addEventListener("click", openNewSiteModal);
  el("btn-cancel-modal").addEventListener("click", closeNewSiteModal);
  el("modal-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "modal-backdrop") closeNewSiteModal();
  });

  el("mode-static").addEventListener("click", () => setNewSiteMode("static"));
  el("mode-existing").addEventListener("click", () => setNewSiteMode("existing"));

  el("btn-pick-folder").addEventListener("click", async () => {
    const folder = await invoke("pick_folder");
    if (folder) el("input-folder").value = folder;
  });

  el("btn-use-starter").addEventListener("click", async () => {
    const name = el("input-name").value.trim();
    if (!name) {
      showToast("Give it a name first", true);
      el("input-name").focus();
      return;
    }
    try {
      const folder = await invoke("generate_starter_site", { name });
      el("input-folder").value = folder;
      showToast("Starter page ready - hit Create site");
    } catch (e) {
      showToast(`Couldn't create starter page: ${e}`, true);
    }
  });

  el("btn-create-site").addEventListener("click", submitNewSite);

  el("btn-copy-address").addEventListener("click", async () => {
    const site = currentSite();
    if (!site?.onion_address) return;
    await navigator.clipboard.writeText(site.onion_address);
    showToast("Address copied");
  });

  el("btn-reveal-key").addEventListener("click", async () => {
    const site = currentSite();
    if (!site) return;
    try {
      const key = await invoke("reveal_private_key", { id: site.id });
      const keyEl = el("key-value");
      keyEl.textContent = key;
      keyEl.classList.remove("hidden-key");
      el("btn-reveal-key").classList.add("hidden");
      el("btn-copy-key").classList.remove("hidden");
    } catch (e) {
      showToast(String(e), true);
    }
  });

  el("btn-copy-key").addEventListener("click", async () => {
    const text = el("key-value").textContent;
    if (!text) return;
    await navigator.clipboard.writeText(text);
    showToast("Key copied - keep it safe");
  });

  el("link-help").addEventListener("click", (e) => {
    e.preventDefault();
    el("help-backdrop").classList.remove("hidden");
  });
  el("btn-close-help").addEventListener("click", () => el("help-backdrop").classList.add("hidden"));
  el("help-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "help-backdrop") el("help-backdrop").classList.add("hidden");
  });
}

async function init() {
  wireEvents();
  state.torStatus = await invoke("tor_status");
  renderTorStatus();
  await refreshSites();

  await listen("tor://status", (event) => {
    state.torStatus = event.payload;
    renderTorStatus();
  });

  // Bootstrap can take a little while; poll site state during that window so
  // the "Reachable over Tor" note updates without the user doing anything.
  setInterval(refreshSites, 4000);
}

init();
