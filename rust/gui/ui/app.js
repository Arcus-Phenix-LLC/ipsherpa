"use strict";

const form = document.querySelector("#connection-form");
const serverInput = document.querySelector("#server");
const connectButton = document.querySelector("#connect");
const refreshButton = document.querySelector("#refresh");
const status = document.querySelector("#status");
const errorBox = document.querySelector("#error");
const slowQuery = document.querySelector("#slow-query");
const panel = document.querySelector("#scope-panel");
const rows = document.querySelector("#scope-rows");
const count = document.querySelector("#scope-count");
const context = document.querySelector("#scope-context");
const empty = document.querySelector("#empty");
const table = document.querySelector("#table-wrapper");
let busy = false;
let connectedServer = null;
let selectedScope = null;
let activeTab = "leases";
let recordData = [];
let recordPage = 0;
const pageSize = 100;
const recordPanel = document.querySelector("#records-panel");
const recordStatus = document.querySelector("#records-status");
const recordError = document.querySelector("#records-error");
const recordEmpty = document.querySelector("#records-empty");
const recordTable = document.querySelector("#records-table-wrapper");
const recordBody = document.querySelector("#records-rows");
const recordRefresh = document.querySelector("#records-refresh");
const recordSlow = document.querySelector("#records-slow");
const tabButtons = [...document.querySelectorAll(".tab")];

function updatePaging() {
  document.querySelector("#records-prev").disabled = busy || recordPage === 0;
  document.querySelector("#records-next").disabled = busy || (recordPage + 1) * pageSize >= recordData.length;
  document.querySelector("#records-pager").hidden = recordData.length <= pageSize;
  document.querySelector("#records-page-status").textContent = recordData.length
    ? `${recordPage * pageSize + 1}–${Math.min((recordPage + 1) * pageSize, recordData.length)} of ${recordData.length}` : "";
}

function setBusy(value) {
  busy = value;
  serverInput.disabled = value;
  connectButton.disabled = value;
  refreshButton.disabled = value || connectedServer === null;
  document.querySelectorAll(".scope-select").forEach(button => { button.disabled = value; });
  tabButtons.forEach(button => { button.disabled = value; });
  recordRefresh.disabled = value || selectedScope === null;
  updatePaging();
}

function clearSelection() {
  selectedScope = null;
  recordData = [];
  recordPage = 0;
  recordPanel.hidden = true;
  recordBody.replaceChildren();
  recordError.hidden = true;
}

function emptyRecords(title, message) {
  recordEmpty.querySelector("h3").textContent = title;
  recordEmpty.querySelector("p").textContent = message;
  recordEmpty.hidden = false;
  recordTable.hidden = true;
}

function expiryText(record) {
  if (record.expiresUnixSeconds === null) return record.expiryLabel || "Not set";
  const when = new Date(record.expiresUnixSeconds * 1000);
  return Number.isNaN(when.getTime()) ? "Invalid server timestamp" : when.toLocaleString(undefined, { timeZoneName: "short" });
}

function renderRecordsPage() {
  const fragment = document.createDocumentFragment();
  for (const record of recordData.slice(recordPage * pageSize, (recordPage + 1) * pageSize)) {
    const row = document.createElement("tr");
    const values = activeTab === "leases"
      ? [record.ipAddress, record.name || "(Unnamed)", record.clientId || "—", `${record.state || "Unknown"}${record.isReservation ? " · Reserved" : ""}`, expiryText(record), record.description || "—"]
      : [record.ipAddress, record.name || "(Unnamed)", record.clientId || "—", record.clientType, record.optionsPresent === null ? "—" : record.optionsPresent ? "Yes" : "No", record.description || "—"];
    for (const value of values) {
      const cell = document.createElement("td");
      cell.textContent = value;
      row.append(cell);
    }
    fragment.append(row);
  }
  recordBody.replaceChildren(fragment);
  updatePaging();
}

async function queryRecords() {
  if (busy || !selectedScope || !connectedServer) return;
  const server = connectedServer;
  const scopeId = selectedScope.scopeId;
  const kind = activeTab;
  setBusy(true);
  recordData = [];
  recordPage = 0;
  recordBody.replaceChildren();
  updatePaging();
  recordError.hidden = true;
  recordSlow.hidden = true;
  recordStatus.dataset.state = "loading";
  recordStatus.textContent = `Loading ${kind}…`;
  document.querySelector("#records-results").setAttribute("aria-busy", "true");
  emptyRecords(`Loading ${kind}`, "Waiting for the DHCP server to respond.");
  document.querySelector("#records-context").textContent = `${server} · ${scopeId}`;
  document.querySelector("#records-results").setAttribute("aria-labelledby", `tab-${kind}`);
  document.querySelector("#records-caption").textContent = `${kind} for scope ${scopeId} on ${server}`;
  document.querySelector("#lease-time-hint").hidden = kind !== "leases";
  const headings = kind === "leases" ? ["IP address", "Hostname", "Client ID", "State", "Lease expires", "Description"]
    : ["IP address", "Name", "Client ID", "Client type", "Options", "Description"];
  document.querySelector("#records-columns").replaceChildren(...headings.map(label => {
    const th = document.createElement("th"); th.scope = "col"; th.textContent = label; return th;
  }));
  const timer = setTimeout(() => { recordSlow.hidden = false; }, 12000);
  try {
    const result = await window.__TAURI__.core.invoke("list_records", { server, scopeId, kind });
    if (result.server !== server || result.scopeId !== scopeId || result.kind !== kind) {
      throw new Error("The record response did not match the selected server and scope. Please retry.");
    }
    recordData = result.records;
    renderRecordsPage();
    recordStatus.dataset.state = "success";
    const noun = kind === "leases" ? "lease" : "reservation";
    recordStatus.textContent = `${recordData.length} ${noun}${recordData.length === 1 ? "" : "s"} returned · Updated ${new Date().toLocaleTimeString()}`;
    if (recordData.length) { recordEmpty.hidden = true; recordTable.hidden = false; }
    else emptyRecords(`No ${kind} found`, "The server responded successfully with an empty result.");
  } catch (error) {
    recordStatus.dataset.state = "error";
    recordStatus.textContent = `${kind === "leases" ? "Lease" : "Reservation"} query failed`;
    recordError.textContent = typeof error === "string" ? error : String(error);
    recordError.hidden = false;
    emptyRecords(`Could not load ${kind}`, "Check the error above, then select Refresh records to retry.");
  } finally {
    clearTimeout(timer);
    recordSlow.hidden = true;
    document.querySelector("#records-results").setAttribute("aria-busy", "false");
    setBusy(false);
  }
}

function selectScope(scope) {
  if (busy) return;
  selectedScope = scope;
  activeTab = "leases";
  recordPanel.hidden = false;
  document.querySelector("#records-title").textContent = scope.name || `Scope ${scope.scopeId}`;
  document.querySelectorAll(".scope-row").forEach(row => {
    const selected = row.dataset.scopeId === scope.scopeId;
    row.classList.toggle("selected", selected);
    row.querySelector(".scope-select").setAttribute("aria-pressed", String(selected));
  });
  updateTabs();
  recordPanel.scrollIntoView({ block: "start" });
  queryRecords();
}

function updateTabs() {
  tabButtons.forEach(button => {
    const selected = button.id === `tab-${activeTab}`;
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
  });
}

tabButtons.forEach(button => button.addEventListener("click", () => {
  if (busy || button.id === `tab-${activeTab}`) return;
  activeTab = button.id.replace("tab-", "");
  updateTabs();
  queryRecords();
}));
document.querySelector(".tabs").addEventListener("keydown", event => {
  if (busy || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  const index = tabButtons.indexOf(document.activeElement);
  const next = event.key === "Home" ? 0 : event.key === "End" ? 1 : (index + 1) % 2;
  tabButtons[next].focus(); tabButtons[next].click();
});
recordRefresh.addEventListener("click", queryRecords);
document.querySelector("#records-prev").addEventListener("click", () => { if (!busy && recordPage > 0) { recordPage--; renderRecordsPage(); } });
document.querySelector("#records-next").addEventListener("click", () => { if (!busy && (recordPage + 1) * pageSize < recordData.length) { recordPage++; renderRecordsPage(); } });

function setEmpty(title, message) {
  empty.querySelector("h3").textContent = title;
  empty.querySelector("p").textContent = message;
  empty.hidden = false;
  table.hidden = true;
}

async function queryScopes(server) {
  if (busy) return;
  server = server.trim();
  if (!server || /\s|\p{Cc}/u.test(server) || server.startsWith("-") || server.length > 253) {
    errorBox.textContent = "Enter a DHCP server hostname or IP address, without spaces or control characters.";
    errorBox.hidden = false;
    serverInput.focus();
    return;
  }
  if (!window.__TAURI__?.core?.invoke) {
    errorBox.textContent = "Open the IPSherpa desktop application to connect to a DHCP server.";
    errorBox.hidden = false;
    return;
  }

  busy = true;
  connectedServer = null;
  clearSelection();
  setBusy(true);
  serverInput.disabled = true;
  connectButton.disabled = true;
  connectButton.textContent = "Connecting…";
  refreshButton.disabled = true;
  panel.setAttribute("aria-busy", "true");
  errorBox.hidden = true;
  slowQuery.hidden = true;
  rows.replaceChildren();
  count.textContent = "—";
  context.textContent = server;
  status.dataset.state = "loading";
  status.textContent = `Querying ${server}…`;
  setEmpty("Looking up scopes", "Waiting for the DHCP server to respond.");
  const slowTimer = setTimeout(() => { slowQuery.hidden = false; }, 12000);
  try {
    const result = await window.__TAURI__.core.invoke("list_scopes", { server });
    connectedServer = result.server;
    serverInput.value = result.server;
    const fragment = document.createDocumentFragment();
    result.scopes.forEach((scope) => {
      const row = document.createElement("tr");
      row.className = "scope-row";
      row.dataset.scopeId = scope.scopeId;
      row.addEventListener("click", event => { if (!event.target.closest("button")) selectScope(scope); });
      const ranges = scope.ranges === null ? "Unavailable" : scope.ranges.length
        ? scope.ranges.map((range) => `${range.start} – ${range.end}`).join("\n")
        : "No ranges configured";
      const values = [scope.scopeId, scope.name === null ? "Unavailable" : scope.name || "(Unnamed)", ranges, scope.state ?? "Unavailable"];
      for (const [index, value] of values.entries()) {
        const cell = document.createElement("td");
        if (index === 0) {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "scope-select";
          button.textContent = value;
          button.setAttribute("aria-pressed", "false");
          button.setAttribute("aria-label", `Browse scope ${scope.scopeId}`);
          button.addEventListener("click", () => selectScope(scope));
          cell.append(button);
        } else cell.textContent = value;
        row.append(cell);
      }
      fragment.append(row);
      if (scope.warnings.length) {
        const warningRow = document.createElement("tr");
        warningRow.className = "scope-warning";
        const cell = document.createElement("td");
        cell.colSpan = 4;
        cell.textContent = `${scope.scopeId}: ${scope.warnings.join("; ")}`;
        warningRow.append(cell);
        fragment.append(warningRow);
      }
    });
    rows.replaceChildren(fragment);
    if (result.notice) {
      errorBox.textContent = result.notice;
      errorBox.hidden = false;
    }
    count.textContent = String(result.scopes.length);
    context.textContent = `${result.server} · Updated ${new Date().toLocaleTimeString()}`;
    status.dataset.state = "success";
    const warnings = result.scopes.filter((scope) => scope.warnings.length).length;
    status.textContent = `Query complete · ${result.scopes.length} scope${result.scopes.length === 1 ? "" : "s"} returned${warnings ? ` · Details unavailable for ${warnings} scope${warnings === 1 ? "" : "s"}` : ""}`;
    if (result.scopes.length) {
      empty.hidden = true;
      table.hidden = false;
    } else {
      setEmpty("No IPv4 scopes found", "The server responded successfully and returned no scope IDs.");
    }
  } catch (error) {
    status.dataset.state = "error";
    status.textContent = "Connection failed";
    errorBox.textContent = typeof error === "string" ? error : String(error);
    errorBox.hidden = false;
    setEmpty("Scopes could not be loaded", "Check the connection details above, then try again.");
  } finally {
    clearTimeout(slowTimer);
    slowQuery.hidden = true;
    busy = false;
    serverInput.disabled = false;
    connectButton.disabled = false;
    connectButton.textContent = "Connect";
    refreshButton.disabled = connectedServer === null;
    setBusy(false);
    panel.setAttribute("aria-busy", "false");
  }
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  queryScopes(serverInput.value);
});
refreshButton.addEventListener("click", () => {
  if (connectedServer) queryScopes(connectedServer);
});
