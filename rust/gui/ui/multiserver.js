"use strict";
(() => {
  const storageKey = "ipsherpa.serverProfiles.v1";
  const profile = document.querySelector("#server-profile");
  const save = document.querySelector("#save-server");
  const remove = document.querySelector("#remove-server");
  const message = document.querySelector("#profiles-message");
  const targets = document.querySelector("#cross-targets");
  const form = document.querySelector("#cross-form");
  const query = document.querySelector("#cross-query");
  const field = document.querySelector("#cross-field");
  const kind = document.querySelector("#cross-kind");
  const search = document.querySelector("#cross-search");
  const stop = document.querySelector("#cross-stop");
  const status = document.querySelector("#cross-status");
  const context = document.querySelector("#cross-context");
  const empty = document.querySelector("#cross-empty");
  const wrapper = document.querySelector("#cross-table-wrapper");
  const body = document.querySelector("#cross-rows");
  const issues = document.querySelector("#cross-issues");
  let saved = [];
  let snapshot = null;
  let page = 0;
  let scanning = false;
  let stopRequested = false;
  let locked = false;
  const pageSize = 100;
  try {
    const parsed = JSON.parse(localStorage.getItem(storageKey) || "[]");
    if (Array.isArray(parsed)) saved = MultiServerCore.uniqueServers(parsed);
  } catch { message.textContent = "Saved servers are unavailable. You can use servers for this session."; }

  function persist() {
    try { localStorage.setItem(storageKey, JSON.stringify(saved)); message.textContent = "Saved server names stay on this computer. No credentials are stored."; }
    catch { message.textContent = "These changes apply only to this session; saved servers could not be stored."; }
  }
  function renderProfiles() {
    const old = profile.value;
    profile.replaceChildren(new Option("Choose a saved server…", ""));
    for (const server of saved) profile.append(new Option(server, server));
    profile.value = saved.find(server => server.toLowerCase() === (connectedServer || old).toLowerCase()) || "";
    renderTargets();
    controls();
  }
  function renderTargets() {
    const previous = new Map([...targets.querySelectorAll("input")].map(input => [input.value.toLowerCase(), input.checked]));
    targets.replaceChildren();
    const servers = MultiServerCore.uniqueServers([...saved, ...(connectedServer ? [connectedServer] : [])]);
    for (const server of servers) {
      const label = document.createElement("label");
      const input = document.createElement("input");
      input.type = "checkbox"; input.value = server;
      input.checked = previous.get(server.toLowerCase()) ?? true;
      input.disabled = locked;
      const text = document.createElement("span"); text.textContent = server;
      label.append(input, text); targets.append(label);
    }
    if (!servers.length) { const hint = document.createElement("p"); hint.className = "hint"; hint.textContent = "Save a server above or connect to a server to add a search target."; targets.append(hint); }
  }
  function controls() {
    for (const control of [profile, save, query, field, kind, search]) control.disabled = locked;
    remove.disabled = locked || !profile.value;
    targets.querySelectorAll("input").forEach(input => { input.disabled = locked; });
    body.querySelectorAll("button").forEach(button => { button.disabled = locked; });
    stop.disabled = !scanning || stopRequested;
    stop.textContent = scanning && stopRequested ? "Stopping after current query…" : "Stop after current query";
  }
  save.addEventListener("click", () => {
    if (!MultiServerCore.validServer(serverInput.value)) { message.textContent = "Enter a valid server hostname or IP without spaces."; serverInput.focus(); return; }
    saved = MultiServerCore.uniqueServers([...saved, serverInput.value]);
    persist(); renderProfiles();
    profile.value = saved.find(server => server.toLowerCase() === serverInput.value.trim().toLowerCase()) || "";
    controls();
  });
  profile.addEventListener("change", () => { if (profile.value) serverInput.value = profile.value; controls(); });
  remove.addEventListener("click", () => { saved = saved.filter(server => server !== profile.value); profile.value = ""; persist(); renderProfiles(); });
  stop.addEventListener("click", () => { stopRequested = true; controls(); });

  async function openScope(match) {
    if (busy) return;
    serverInput.value = match.server;
    await queryScopes(match.server);
    if (connectedServer?.toLowerCase() !== match.server.toLowerCase()) return;
    const scope = loadedScopes.find(scope => scope.scopeId === match.scopeId);
    if (!scope) { status.textContent = "The selected scope is no longer present on that server. Refresh the cross-server search."; return; }
    await selectScope(scope, match.kind);
  }
  function renderMatches() {
    const rows = snapshot?.matches || [];
    const fragment = document.createDocumentFragment();
    for (const match of rows.slice(page * pageSize, (page + 1) * pageSize)) {
      const row = document.createElement("tr");
      const values = [match.server, match.scopeId, match.kind === "leases" ? "Lease" : "Reservation",
        match.record.ipAddress, match.record.name || "(Unnamed)", match.record.clientId || "—",
        match.kind === "leases" ? `${match.record.state || "Unknown"}${match.record.isReservation ? " · Reserved" : ""}` : match.record.clientType];
      values.forEach((value, index) => {
        const cell = document.createElement("td");
        if (index === 1) {
          const button = document.createElement("button"); button.type = "button"; button.className = "scope-select";
          button.textContent = value; button.disabled = locked;
          button.setAttribute("aria-label", `Open ${match.kind} for scope ${match.scopeId} on ${match.server}`);
          button.addEventListener("click", () => openScope(match)); cell.append(button);
          if (match.scopeName) { const name = document.createElement("div"); name.className = "hint"; name.textContent = match.scopeName; cell.append(name); }
        } else cell.textContent = value;
        row.append(cell);
      });
      fragment.append(row);
    }
    body.replaceChildren(fragment);
    wrapper.hidden = !rows.length;
    empty.hidden = Boolean(rows.length);
    document.querySelector("#cross-pager").hidden = rows.length <= pageSize;
    document.querySelector("#cross-prev").disabled = page === 0;
    document.querySelector("#cross-next").disabled = (page + 1) * pageSize >= rows.length;
    document.querySelector("#cross-page-status").textContent = rows.length ? `${page * pageSize + 1}–${Math.min((page + 1) * pageSize, rows.length)} of ${rows.length}` : "";
    issues.replaceChildren();
    for (const issue of snapshot?.issues || []) {
      const li = document.createElement("li");
      li.textContent = `${issue.server}${issue.scopeId ? ` · ${issue.scopeId}` : ""} · ${issue.kind}: ${issue.message}`;
      issues.append(li);
    }
    document.querySelector("#cross-issue-panel").hidden = !snapshot?.issues.length;
  }
  document.querySelector("#cross-prev").addEventListener("click", () => { if (page > 0) { page--; renderMatches(); } });
  document.querySelector("#cross-next").addEventListener("click", () => { if ((page + 1) * pageSize < (snapshot?.matches.length || 0)) { page++; renderMatches(); } });
  form.addEventListener("submit", async event => {
    event.preventDefault();
    if (busy) return;
    const servers = [...targets.querySelectorAll("input:checked")].map(input => input.value);
    const term = query.value.trim();
    if (!servers.length || !term) { status.textContent = "Select at least one server and enter a search term."; return; }
    if (!window.__TAURI__?.core?.invoke) { status.textContent = "Open the IPSherpa desktop application to query servers."; return; }
    scanning = true; stopRequested = false; setBusy(true);
    page = 0; snapshot = null; renderMatches();
    context.textContent = `Search “${term}” · ${servers.join(", ")} · ${kind.options[kind.selectedIndex].text}`;
    empty.textContent = "Search in progress. Matches will appear as queries complete.";
    document.querySelector("#cross-results").setAttribute("aria-busy", "true");
    try {
      snapshot = await MultiServerCore.scan({ targets: servers, query: term, field: field.value, kind: kind.value,
        invoke: (command, args) => window.__TAURI__.core.invoke(command, args),
        shouldStop: () => stopRequested,
        onProgress: progress => {
          snapshot = progress.result;
          if (progress.stage !== "done") status.textContent = `Scanning ${progress.server}${progress.scopeId ? ` · ${progress.scopeId} · ${progress.kind}` : " · scopes"} · ${snapshot.completedQueries} record queries completed · ${snapshot.matches.length} matches`;
          renderMatches();
        }
      });
      const partial = snapshot.stopped || snapshot.failedQueries > 0;
      status.textContent = `${partial ? "Partial results" : snapshot.issues.length ? "Search complete with warnings" : "Search complete"} · ${snapshot.matches.length} matches · ${snapshot.scopesDiscovered} scopes · ${snapshot.completedQueries} record queries completed · ${snapshot.serversCompleted}/${servers.length} servers complete · ${snapshot.failedQueries} failures${snapshot.stopped ? " · Stopped" : ""}`;
      empty.textContent = partial ? "No matches in completed queries. The search was incomplete; review failures or retry." : "No matching records were returned by the completed search of all selected servers and scopes.";
    } catch (error) { status.textContent = `Search failed: ${error}`; empty.textContent = "The search could not complete. Retry after checking the connection."; }
    finally { scanning = false; document.querySelector("#cross-results").setAttribute("aria-busy", "false"); setBusy(false); renderMatches(); }
  });
  window.MultiServer = {
    setBusy(value) { locked = value; controls(); },
    connectionChanged() { renderProfiles(); }
  };
  renderProfiles();
})();
