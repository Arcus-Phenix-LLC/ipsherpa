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
let loadedScopes = [];
let selectedScope = null;
let activeTab = "leases";
let recordData = [];
let recordPage = 0;
let recordsLoaded = false;
let recordCache = {};
const blankFilter = () => ({ query: "", field: "all", state: "" });
let filters = { leases: blankFilter(), reservations: blankFilter() };
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
const searchInput = document.querySelector("#records-search");
const searchField = document.querySelector("#records-search-field");
const stateFilter = document.querySelector("#records-state");
const clearFilters = document.querySelector("#clear-filters");

function filteredRecords() {
  return recordData.filter(record => RecordFilters.matches(record, filters[activeTab], activeTab));
}

function updateFilterControls() {
  for (const control of [searchInput, searchField, stateFilter]) control.disabled = busy || !recordsLoaded;
  const filter = filters[activeTab];
  clearFilters.disabled = busy || !recordsLoaded || !(filter.query || filter.state || filter.field !== "all");
}

function syncFilters() {
  const filter = filters[activeTab];
  searchInput.value = filter.query;
  searchField.value = filter.field;
  document.querySelector("#lease-state-control").hidden = activeTab !== "leases";
  stateFilter.replaceChildren(new Option("All states", ""));
  const states = new Set(activeTab === "leases" ? recordData.map(record => record.state || "Unknown") : []);
  if (filter.state) states.add(filter.state);
  for (const state of [...states].sort()) stateFilter.append(new Option(state, state));
  stateFilter.value = filter.state;
  updateFilterControls();
}

function applyFilters() {
  if (busy || !recordsLoaded) return;
  recordPage = 0;
  renderRecordsPage();
  updateFilterControls();
}

searchInput.addEventListener("input", () => { filters[activeTab].query = searchInput.value; applyFilters(); });
searchField.addEventListener("change", () => { filters[activeTab].field = searchField.value; applyFilters(); });
stateFilter.addEventListener("change", () => { filters[activeTab].state = stateFilter.value; applyFilters(); });
clearFilters.addEventListener("click", () => {
  filters[activeTab] = blankFilter(); syncFilters(); applyFilters(); searchInput.focus();
});

function updatePaging() {
  const length = filteredRecords().length;
  document.querySelector("#records-prev").disabled = busy || recordPage === 0;
  document.querySelector("#records-next").disabled = busy || (recordPage + 1) * pageSize >= length;
  document.querySelector("#records-pager").hidden = length <= pageSize;
  document.querySelector("#records-page-status").textContent = length
    ? `${recordPage * pageSize + 1}–${Math.min((recordPage + 1) * pageSize, length)} of ${length}` : "";
}

function setBusy(value) {
  busy = value;
  serverInput.disabled = value;
  connectButton.disabled = value;
  refreshButton.disabled = value || connectedServer === null;
  document.querySelectorAll(".scope-select").forEach(button => { button.disabled = value; });
  tabButtons.forEach(button => { button.disabled = value; });
  document.querySelectorAll(".reservation-edit-row").forEach(button => { button.disabled = value || button.dataset.unsupported === "true"; });
  recordRefresh.disabled = value || selectedScope === null;
  updateFilterControls();
  updatePaging();
  window.MultiServer?.setBusy(value);
  window.ScopeCapacity?.setBusy(value);
  window.InspectionViews?.setBusy(value);
  window.Navigation?.setBusy(value);
  window.ReservationUI?.setBusy(value);
}

function clearSelection() {
  recordCache = {};
  window.Navigation?.cleared();
  window.ScopeCapacity?.reset();
  window.InspectionViews?.scope(null);
  selectedScope = null;
  window.ReservationUI?.setBusy(busy);
  recordData = [];
  recordsLoaded = false;
  filters = { leases: blankFilter(), reservations: blankFilter() };
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

function reservationMac(value) {
  let hex = (value || "").replace(/[-:.]/g, "").toUpperCase();
  if (hex.length === 22 && selectedScope) {
    const prefix = selectedScope.scopeId.split(".").reverse().map(v => Number(v).toString(16).padStart(2,"0")).join("").toUpperCase() + "01";
    if (!hex.startsWith(prefix)) return null;
    hex = hex.slice(10);
  }
  if (!/^[0-9A-F]{12}$/.test(hex) || (parseInt(hex.slice(0,2),16) & 1) || /^0+$/.test(hex)) return null;
  return hex.match(/../g).join("-");
}

function renderRecordsPage() {
  const filtered = filteredRecords();
  const fragment = document.createDocumentFragment();
  for (const record of filtered.slice(recordPage * pageSize, (recordPage + 1) * pageSize)) {
    const row = document.createElement("tr");
    const values = activeTab === "leases"
      ? [record.ipAddress, record.name || "(Unnamed)", record.clientId || "—", `${record.state || "Unknown"}${record.isReservation ? " · Reserved" : ""}`, expiryText(record), record.description || "—"]
      : [record.ipAddress, record.name || "(Unnamed)", record.clientId || "—", record.clientType, record.optionsPresent === null ? "—" : record.optionsPresent ? "Yes" : "No", record.description || "—"];
    for (const value of values) {
      const cell = document.createElement("td");
      cell.textContent = value;
      row.append(cell);
    }
    if (activeTab === "reservations") {
      const cell = document.createElement("td"), button = document.createElement("button");
      button.type = "button"; button.className = "secondary reservation-edit-row"; button.textContent = "Edit";
      const mac = reservationMac(record.clientId);
      const supported = ["DHCP","BOOTP","DHCP / BOOTP"].includes(record.clientType) && Boolean(mac);
      button.dataset.unsupported = String(!supported); button.disabled = busy || !supported;
      button.setAttribute("aria-label", `Edit reservation ${record.ipAddress}`);
      button.title = supported ? "Edit name and description" : "Editing supports DHCP, BOOTP and DHCP/BOOTP with six-byte unicast MACs.";
      button.addEventListener("click", () => window.ReservationUI?.open(record)); cell.append(button); row.append(cell);
    }
    fragment.append(row);
  }
  recordBody.replaceChildren(fragment);
  document.querySelector("#filter-summary").textContent = `${filtered.length} of ${recordData.length} ${activeTab} shown`;
  if (filtered.length) { recordEmpty.hidden = true; recordTable.hidden = false; }
  else if (recordData.length) emptyRecords(`No matching ${activeTab}`, "Change the search or lease state, or select Clear filters.");
  else emptyRecords(`No ${activeTab} found`, "The server responded successfully with an empty result.");
  updatePaging();
}

async function queryRecords(refresh = false) {
  if (busy || !selectedScope || !connectedServer) return;
  const server = connectedServer;
  const scopeId = selectedScope.scopeId;
  const kind = activeTab;
  window.Navigation?.recordsChanged();
  document.querySelector("#records-context").textContent = `${server} · ${scopeId}`;
  document.querySelector("#records-results").setAttribute("aria-labelledby", `tab-${kind}`);
  document.querySelector("#records-caption").textContent = `${kind} for scope ${scopeId} on ${server}`;
  document.querySelector("#lease-time-hint").hidden = kind !== "leases";
  const headings = kind === "leases" ? ["IP address", "Hostname", "Client ID", "State", "Lease expires", "Description"]
    : ["IP address", "Name", "Client ID", "Client type", "Options", "Description", "Actions"];
  document.querySelector("#records-columns").replaceChildren(...headings.map(label => {
    const th = document.createElement("th"); th.scope = "col"; th.textContent = label; return th;
  }));
  if (!refresh && recordCache[kind]) {
    const cached = recordCache[kind]; recordData = cached.records; recordPage = cached.page;
    recordsLoaded = true; recordError.hidden = true; recordSlow.hidden = true;
    recordStatus.dataset.state = "success"; recordStatus.textContent = cached.status;
    syncFilters(); renderRecordsPage(); return;
  }
  delete recordCache[kind];
  setBusy(true);
  recordsLoaded = false;
  recordData = [];
  recordPage = 0;
  recordBody.replaceChildren();
  document.querySelector("#filter-summary").textContent = "";
  syncFilters();
  updatePaging();
  recordError.hidden = true;
  recordSlow.hidden = true;
  recordStatus.dataset.state = "loading";
  recordStatus.textContent = `Loading ${kind}…`;
  document.querySelector("#records-results").setAttribute("aria-busy", "true");
  emptyRecords(`Loading ${kind}`, "Waiting for the DHCP server to respond.");
  const timer = setTimeout(() => { recordSlow.hidden = false; }, 12000);
  try {
    const result = await window.__TAURI__.core.invoke("list_records", { server, scopeId, kind });
    if (result.server !== server || result.scopeId !== scopeId || result.kind !== kind) {
      throw new Error("The record response did not match the selected server and scope. Please retry.");
    }
    recordData = result.records;
    recordsLoaded = true;
    syncFilters();
    renderRecordsPage();
    recordStatus.dataset.state = "success";
    const noun = kind === "leases" ? "lease" : "reservation";
    recordStatus.textContent = `${recordData.length} ${noun}${recordData.length === 1 ? "" : "s"} returned · Updated ${new Date().toLocaleTimeString()}`;
    recordCache[kind] = {records:recordData, page:0, status:recordStatus.textContent};
  } catch (error) {
    recordsLoaded = false;
    recordData = [];
    recordBody.replaceChildren();
    document.querySelector("#filter-summary").textContent = "";
    updatePaging();
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

function selectScope(scope, kind = null) {
  if (busy) return;
  if (selectedScope?.scopeId === scope.scopeId) {
    window.Navigation?.selected(scope, kind);
    if (kind) { activeTab = kind; updateTabs(); return queryRecords(false); }
    return;
  }
  recordCache = {};
  recordsLoaded = false; recordData = []; recordPage = 0;
  selectedScope = scope;
  window.ReservationUI?.setBusy(busy);
  window.ScopeCapacity?.selectionChanged(connectedServer, scope);
  window.InspectionViews?.scope(scope.scopeId);
  filters = { leases: blankFilter(), reservations: blankFilter() };
  activeTab = kind || "leases";
  document.querySelector("#records-context").textContent = `${connectedServer} · ${scope.scopeId}`;
  recordPanel.hidden = false;
  document.querySelector("#records-title").textContent = scope.name || `Scope ${scope.scopeId}`;
  document.querySelectorAll(".scope-row").forEach(row => {
    const selected = row.dataset.scopeId === scope.scopeId;
    row.classList.toggle("selected", selected);
    row.querySelector(".scope-select").setAttribute("aria-pressed", String(selected));
  });
  updateTabs();
  window.Navigation?.selected(scope, kind);
  if (kind) return queryRecords(false);
}

function updateTabs() {
  tabButtons.forEach(button => {
    const selected = button.id === `tab-${activeTab}`;
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
  });
}

tabButtons.forEach(button => button.addEventListener("click", () => {
  if (busy) return;
  if (button.id === `tab-${activeTab}`) { if (!recordsLoaded) queryRecords(false); return; }
  if (recordsLoaded && recordCache[activeTab]) recordCache[activeTab].page = recordPage;
  activeTab = button.id.replace("tab-", "");
  updateTabs();
  queryRecords(false);
}));
recordRefresh.addEventListener("click", () => queryRecords(true));
document.querySelector("#records-prev").addEventListener("click", () => { if (!busy && recordPage > 0) { recordPage--; renderRecordsPage(); } });
document.querySelector("#records-next").addEventListener("click", () => { if (!busy && (recordPage + 1) * pageSize < filteredRecords().length) { recordPage++; renderRecordsPage(); } });

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
  window.InspectionViews?.server(null);
  loadedScopes = [];
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
    window.Navigation?.connected();
    window.InspectionViews?.server(result.server);
    loadedScopes = result.scopes;
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
    window.MultiServer?.connectionChanged();
  }
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  queryScopes(serverInput.value);
});
refreshButton.addEventListener("click", () => {
  if (connectedServer) queryScopes(connectedServer);
});
