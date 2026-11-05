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
    result.scopeIds.forEach((scopeId, index) => {
      const row = document.createElement("tr");
      for (const value of [String(index + 1), scopeId]) {
        const cell = document.createElement("td");
        cell.textContent = value;
        row.append(cell);
      }
      fragment.append(row);
    });
    rows.replaceChildren(fragment);
    count.textContent = String(result.scopeIds.length);
    context.textContent = `${result.server} · Updated ${new Date().toLocaleTimeString()}`;
    status.dataset.state = "success";
    status.textContent = `Query complete · ${result.scopeIds.length} scope${result.scopeIds.length === 1 ? "" : "s"} returned`;
    if (result.scopeIds.length) {
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
