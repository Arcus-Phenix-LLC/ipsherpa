"use strict";
// Shared browser/Node search logic. Filtering never changes the source snapshot.
((root) => {
  const text = value => String(value ?? "").normalize("NFKC").toLowerCase();
  function clientMatches(value, query) {
    if (text(value).includes(query)) return true;
    // Only hex input is eligible for separator-insensitive UID matching. Do not
    // turn words such as "office" into an unrelated sequence of hex letters.
    if (!/^[0-9a-f: .\-]+$/i.test(query)) return false;
    const compact = query.replace(/[: .\-]/g, "");
    return compact.length >= 2 && text(value).replace(/[: .\-]/g, "").includes(compact);
  }
  function matches(record, filter, kind) {
    if (kind === "leases" && filter.state && text(record.state || "Unknown") !== text(filter.state)) return false;
    const query = text(filter.query).trim();
    if (!query) return true;
    switch (filter.field) {
      case "ip": return text(record.ipAddress).includes(query);
      case "name": return text(record.name).includes(query);
      case "clientId": return clientMatches(record.clientId, query);
      case "all": return text(record.ipAddress).includes(query) || text(record.name).includes(query) || clientMatches(record.clientId, query);
      default: return false;
    }
  }
  const api = { matches };
  if (typeof module === "object" && module.exports) module.exports = api;
  else root.RecordFilters = api;
})(globalThis);
