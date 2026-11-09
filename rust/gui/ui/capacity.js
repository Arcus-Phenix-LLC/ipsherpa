"use strict";
(() => {
  const button = document.querySelector("#capacity-refresh");
  const status = document.querySelector("#capacity-status");
  const context = document.querySelector("#capacity-context");
  const errors = document.querySelector("#capacity-warnings");
  const rows = document.querySelector("#exclusion-rows");
  const table = document.querySelector("#exclusion-table-wrapper");
  const empty = document.querySelector("#exclusion-empty");
  const meter = document.querySelector("#capacity-meter");
  const slow = document.querySelector("#capacity-slow");
  let target = null;
  let locked = false;
  let loaded = false;
  const valueIds = ["in-use", "free", "offers", "percent", "configured", "excluded", "usable"];
  function write(id, value) { document.querySelector(`#capacity-${id}`).textContent = value; }
  function controls() { button.disabled = locked || !target; }
  function reset() {
    loaded = false;
    valueIds.forEach(id => write(id, "Not loaded"));
    meter.hidden = true; rows.replaceChildren(); table.hidden = true;
    errors.replaceChildren(); document.querySelector("#capacity-issue-panel").hidden = true;
    status.textContent = "Overview not loaded";
    status.dataset.state = "";
    empty.textContent = "Load the overview to read exclusion ranges.";
    empty.hidden = false; slow.hidden = true;
    button.textContent = "Load overview";
  }
  function number(value) { return Number(value).toLocaleString(); }
  function render(result) {
    const stats = result.statistics;
    write("in-use", stats === null ? "Unavailable" : number(stats.inUse));
    write("free", stats === null ? "Unavailable" : number(stats.free));
    write("offers", stats === null ? "Unavailable" : number(stats.pendingOffers));
    const percent = stats?.utilizationPercent;
    write("percent", percent === null || percent === undefined ? "Unavailable" : `${percent.toFixed(1)}%`);
    meter.hidden = percent === null || percent === undefined;
    if (!meter.hidden) meter.value = percent;
    for (const [id, key] of [["configured", "configured"], ["excluded", "excluded"], ["usable", "afterExclusions"]]) {
      write(id, result.totals === null ? "Unavailable" : number(result.totals[key]));
    }
    const fragment = document.createDocumentFragment();
    for (const range of result.exclusions || []) {
      const row = document.createElement("tr");
      const asNumber = value => value.split(".").reduce((total, part) => total * 256 + Number(part), 0);
      for (const value of [range.start, range.end, number(asNumber(range.end) - asNumber(range.start) + 1)]) {
        const cell = document.createElement("td"); cell.textContent = value; row.append(cell);
      }
      fragment.append(row);
    }
    rows.replaceChildren(fragment);
    table.hidden = !result.exclusions?.length;
    empty.hidden = !table.hidden;
    empty.textContent = result.exclusions === null ? "Exclusion ranges are unavailable. Review the query errors." : "No exclusion ranges configured.";
    errors.replaceChildren();
    for (const warning of result.warnings) { const item = document.createElement("li"); item.textContent = warning; errors.append(item); }
    document.querySelector("#capacity-issue-panel").hidden = !result.warnings.length;
    status.dataset.state = result.warnings.length ? "error" : "success";
    const available = result.statistics !== null || result.configuredRanges !== null || result.exclusions !== null;
    status.textContent = `${!available ? "Overview unavailable" : result.warnings.length ? "Partial overview · Some values unavailable" : "Overview loaded"} · Updated ${new Date().toLocaleTimeString()}`;
  }
  button.addEventListener("click", async () => {
    if (busy || !target) return;
    const source = { ...target };
    reset(); valueIds.forEach(id => write(id, "—"));
    setBusy(true);
    status.dataset.state = "loading"; status.textContent = "Loading utilization and exclusions…";
    document.querySelector("#capacity-results").setAttribute("aria-busy", "true");
    const timer = setTimeout(() => { slow.hidden = false; }, 12000);
    try {
      const result = await window.__TAURI__.core.invoke("scope_capacity", { server: source.server, scopeId: source.scopeId });
      if (result.server !== source.server || result.scopeId !== source.scopeId) throw new Error("Capacity response did not match the selected server and scope.");
      render(result); loaded = true;
    } catch (error) {
      rows.replaceChildren(); table.hidden = true; empty.hidden = false;
      valueIds.forEach(id => write(id, "Unavailable")); meter.hidden = true;
      status.dataset.state = "error"; status.textContent = "Overview query failed";
      const item = document.createElement("li"); item.textContent = String(error); errors.append(item);
      document.querySelector("#capacity-issue-panel").hidden = false;
      empty.textContent = "Exclusion ranges could not be loaded. Retry after checking the query error.";
    } finally {
      clearTimeout(timer); slow.hidden = true;
      document.querySelector("#capacity-results").setAttribute("aria-busy", "false");
      button.textContent = loaded ? "Refresh overview" : "Retry overview";
      setBusy(false);
    }
  });
  window.ScopeCapacity = {
    reset() { target = null; reset(); context.textContent = ""; controls(); },
    selectionChanged(server, scope) { target = { server, scopeId: scope.scopeId }; reset(); context.textContent = `${server} · ${scope.scopeId}`; controls(); },
    setBusy(value) { locked = value; controls(); }
  };
  reset(); controls();
})();
