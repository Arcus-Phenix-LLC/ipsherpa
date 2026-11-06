"use strict";
((root) => {
  const matching = typeof module === "object" && module.exports ? require("./filters.js") : root.RecordFilters;
  function validServer(value) {
    return typeof value === "string" && value.trim().length > 0 && value.trim().length <= 253
      && !value.trim().startsWith("-") && !/[\s\p{Cc}]/u.test(value.trim());
  }
  function uniqueServers(values) {
    const seen = new Set();
    return values.filter(validServer).map(value => value.trim()).filter(value => {
      const key = value.toLowerCase(); if (seen.has(key)) return false; seen.add(key); return true;
    });
  }
  async function scan({ targets, query, field = "all", kind = "both", invoke, onProgress = () => {}, shouldStop = () => false }) {
    targets = uniqueServers(targets);
    query = query.trim();
    if (!targets.length || !query) throw new Error("Select at least one server and enter a search term.");
    if (!["all", "ip", "name", "clientId"].includes(field) || !["both", "leases", "reservations"].includes(kind)) throw new Error("Invalid cross-server search options.");
    const result = { matches: [], issues: [], completedQueries: 0, failedQueries: 0,
      serversAttempted: 0, serversCompleted: 0, scopesDiscovered: 0, stopped: false };
    const kinds = kind === "both" ? ["leases", "reservations"] : [kind];
    const emit = (stage, server = "", scopeId = "", recordKind = "") => onProgress({ stage, server, scopeId, kind: recordKind, result });
    servers: for (const server of targets) {
      if (shouldStop()) { result.stopped = true; break; }
      result.serversAttempted++;
      emit("scopes", server);
      let scopeList;
      try {
        scopeList = await invoke("list_scopes", { server });
        if (scopeList.server?.toLowerCase() !== server.toLowerCase() || !Array.isArray(scopeList.scopes)) throw new Error("Scope response did not match the requested server.");
      } catch (error) {
        result.failedQueries++;
        result.issues.push({ server, scopeId: "", kind: "scopes", message: String(error), warning: false });
        emit("progress", server); continue;
      }
      result.scopesDiscovered += scopeList.scopes.length;
      if (scopeList.notice) result.issues.push({ server, scopeId: "", kind: "scope details", message: scopeList.notice, warning: true });
      let failed = false;
      for (const scope of scopeList.scopes) {
        for (const message of scope.warnings || []) result.issues.push({ server, scopeId: scope.scopeId, kind: "scope details", message, warning: true });
        for (const recordKind of kinds) {
          if (shouldStop()) { result.stopped = true; break servers; }
          emit("records", server, scope.scopeId, recordKind);
          try {
            const records = await invoke("list_records", { server, scopeId: scope.scopeId, kind: recordKind });
            if (records.server?.toLowerCase() !== server.toLowerCase() || records.scopeId !== scope.scopeId
              || records.kind !== recordKind || !Array.isArray(records.records)) throw new Error("Record response did not match the requested server, scope and type.");
            for (const record of records.records) {
              if (matching.matches(record, { query, field, state: "" }, recordKind)) {
                result.matches.push({ server, scopeId: scope.scopeId, scopeName: scope.name || "", kind: recordKind, record });
              }
            }
            result.completedQueries++;
          } catch (error) {
            failed = true; result.failedQueries++;
            result.issues.push({ server, scopeId: scope.scopeId, kind: recordKind, message: String(error), warning: false });
          }
          emit("progress", server, scope.scopeId, recordKind);
        }
      }
      if (!failed) result.serversCompleted++;
    }
    emit("done");
    return result;
  }
  const api = { validServer, uniqueServers, scan };
  if (typeof module === "object" && module.exports) module.exports = api;
  else root.MultiServerCore = api;
})(globalThis);
