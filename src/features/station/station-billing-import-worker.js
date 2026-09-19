(function (root, factory) {
  const api = factory();
  if (
    root &&
    typeof root.postMessage === "function" &&
    typeof root.document === "undefined"
  ) {
    root.onmessage = async (event) => {
      try {
        root.postMessage({
          ok: true,
          result: api.analyzeFiles(event.data && event.data.files),
        });
      } catch (error) {
        root.postMessage({
          ok: false,
          error: String((error && error.message) || error),
        });
      }
    };
  }
})(typeof self !== "undefined" ? self : globalThis, function () {
  "use strict";

  const MAX_FILE_BYTES = 5 * 1024 * 1024;
  const MAX_RECORDS = 50000;
  const MAX_LABEL_LENGTH = 128;
  const PRICE_SNAPSHOTS = Object.freeze({
    "gpt-5.6-sol": "openai-gpt-5.6-sol-2026-08-28",
  });
  const ALIASES = Object.freeze({
    model: ["model", "model_name"],
    group: ["group", "group_name"],
    input_total: ["input_total", "input_tokens", "input", "prompt"],
    cache_read: [
      "cache_read",
      "cached",
      "cached_input_tokens",
      "cache_read_tokens",
    ],
    cache_write: ["cache_write", "cache_creation_tokens", "cache_write_tokens"],
    output_total: ["output_total", "output_tokens", "output", "completion"],
    cost: ["cost", "quota", "amount", "actual_cost"],
    currency: ["currency"],
  });

  function analyzeFiles(files) {
    if (!Array.isArray(files) || !files.length) throw new Error("no_files");
    const sources = files.map((file, index) => analyzeFile(file, index));
    return { sources, comparison: compareSources(sources) };
  }

  function analyzeFile(file, index) {
    const name = sanitizeLabel(file && file.name) || `billing-${index + 1}`;
    const bytes = toBytes(file && file.content);
    if (bytes.byteLength > MAX_FILE_BYTES)
      throw new Error(`file_too_large:${name}`);
    const rows = parseRows(name, bytes);
    if (rows.length > MAX_RECORDS) throw new Error(`too_many_records:${name}`);
    const records = rows.map(normalizeRecord);
    return {
      source_id: `source-${index + 1}`,
      source_name: name,
      record_count: records.length,
      groups: aggregate(records).map((group) => ({
        ...group,
        price_snapshot_id: PRICE_SNAPSHOTS[group.model] || null,
        token_mix_signature: tokenMixSignature(group),
      })),
    };
  }

  function parseRows(name, bytes) {
    let text;
    try {
      text = new TextDecoder("utf-8", { fatal: true })
        .decode(bytes)
        .replace(/^\uFEFF/, "");
    } catch (_error) {
      throw new Error(`not_utf8:${name}`);
    }
    if (/\.json$/i.test(name)) {
      let payload;
      try {
        payload = JSON.parse(text);
      } catch (_error) {
        throw new Error(`invalid_json:${name}`);
      }
      if (payload && !Array.isArray(payload) && typeof payload === "object") {
        for (const key of ["data", "items", "rows", "usage"]) {
          if (Array.isArray(payload[key])) {
            payload = payload[key];
            break;
          }
        }
      }
      if (
        !Array.isArray(payload) ||
        payload.some(
          (row) => !row || Array.isArray(row) || typeof row !== "object",
        )
      ) {
        throw new Error(`json_rows_must_be_objects:${name}`);
      }
      return payload;
    }
    if (/\.csv$/i.test(name)) return parseCsv(text, name);
    throw new Error(`unsupported_file_type:${name}`);
  }

  function parseCsv(text, name) {
    const rows = [];
    let row = [];
    let field = "";
    let quoted = false;
    for (let index = 0; index <= text.length; index += 1) {
      const char = index < text.length ? text[index] : "\n";
      if (quoted) {
        if (char === '"' && text[index + 1] === '"') {
          field += '"';
          index += 1;
        } else if (char === '"') quoted = false;
        else field += char;
      } else if (char === '"' && field === "") {
        quoted = true;
      } else if (char === ",") {
        row.push(field);
        field = "";
      } else if (char === "\n" || char === "\r") {
        if (char === "\r" && text[index + 1] === "\n") index += 1;
        row.push(field);
        field = "";
        if (row.some((value) => value !== "")) rows.push(row);
        row = [];
      } else {
        field += char;
      }
    }
    if (quoted) throw new Error(`invalid_csv:${name}`);
    if (!rows.length) return [];
    const headers = rows.shift().map((value) =>
      String(value || "")
        .trim()
        .toLowerCase(),
    );
    return rows.map((values) =>
      Object.fromEntries(
        headers.map((header, index) => [header, values[index] ?? ""]),
      ),
    );
  }

  function normalizeRecord(row) {
    const lookup = {};
    Object.entries(row || {}).forEach(([key, value]) => {
      lookup[String(key).toLowerCase()] = value;
    });
    const other = parseOther(lookup.other);
    Object.entries(other).forEach(([key, value]) => {
      const normalized = String(key).toLowerCase();
      if (lookup[normalized] === undefined || lookup[normalized] === "")
        lookup[normalized] = value;
    });
    const values = {};
    Object.entries(ALIASES).forEach(([field, aliases]) => {
      values[field] = firstValue(lookup, aliases);
    });
    return {
      model: sanitizeLabel(values.model),
      group: sanitizeGroup(values.group),
      currency:
        sanitizeLabel(values.currency) &&
        sanitizeLabel(values.currency).toUpperCase(),
      input_total: optionalInteger(values.input_total),
      cache_read: optionalInteger(values.cache_read),
      cache_write: optionalInteger(values.cache_write),
      output_total: optionalInteger(values.output_total),
      cost: optionalNumber(values.cost),
    };
  }

  function parseOther(value) {
    if (value && typeof value === "object" && !Array.isArray(value))
      return value;
    if (typeof value !== "string" || !value.trim().startsWith("{")) return {};
    try {
      const parsed = JSON.parse(value);
      return parsed && typeof parsed === "object" && !Array.isArray(parsed)
        ? parsed
        : {};
    } catch (_error) {
      return {};
    }
  }

  function firstValue(row, aliases) {
    for (const alias of aliases) {
      if (row[alias] !== undefined && row[alias] !== null && row[alias] !== "")
        return row[alias];
    }
    return null;
  }

  function sanitizeGroup(value) {
    if (value && typeof value === "object" && !Array.isArray(value))
      return sanitizeLabel(value.name ?? value.id);
    return sanitizeLabel(value);
  }

  function sanitizeLabel(value) {
    if (!["string", "number"].includes(typeof value)) return null;
    return (
      String(value)
        .replace(/[\x00-\x1f\x7f]/g, "")
        .trim()
        .slice(0, MAX_LABEL_LENGTH) || null
    );
  }

  function optionalNumber(value) {
    if (
      value === null ||
      value === undefined ||
      value === "" ||
      typeof value === "boolean"
    )
      return null;
    const number = Number(value);
    return Number.isFinite(number) && number >= 0 ? number : null;
  }

  function optionalInteger(value) {
    const number = optionalNumber(value);
    return number !== null && Number.isInteger(number) ? number : null;
  }

  function aggregate(records) {
    const summaries = new Map();
    records.forEach((record) => {
      const key = JSON.stringify([record.model, record.group, record.currency]);
      if (!summaries.has(key)) summaries.set(key, newSummary(record));
      const summary = summaries.get(key);
      summary.record_count += 1;
      for (const field of [
        "input_total",
        "cache_read",
        "cache_write",
        "output_total",
      ]) {
        if (record[field] !== null) {
          summary[field] += record[field];
          summary[`${field}_sample_count`] += 1;
        }
      }
      if (record.input_total !== null && record.cache_read !== null) {
        summary.cache_hit_numerator += record.cache_read;
        summary.cache_hit_denominator += record.input_total;
        summary.cache_hit_sample_count += 1;
      }
      if (record.cost !== null) {
        summary.cost_total = preciseAdd(summary.cost_total, record.cost);
        summary.cost_sample_count += 1;
      }
    });
    return Array.from(summaries.values()).map(finalizeSummary);
  }

  function newSummary(record) {
    return {
      model: record.model,
      group: record.group,
      currency: record.currency,
      record_count: 0,
      input_total: 0,
      input_total_sample_count: 0,
      cache_read: 0,
      cache_read_sample_count: 0,
      cache_write: 0,
      cache_write_sample_count: 0,
      output_total: 0,
      output_total_sample_count: 0,
      cache_hit_numerator: 0,
      cache_hit_denominator: 0,
      cache_hit_sample_count: 0,
      cost_total: 0,
      cost_sample_count: 0,
    };
  }

  function finalizeSummary(summary) {
    const output = { ...summary };
    for (const field of [
      "input_total",
      "cache_read",
      "cache_write",
      "output_total",
    ]) {
      if (!output[`${field}_sample_count`]) output[field] = null;
    }
    output.cache_hit_rate =
      output.cache_hit_denominator > 0
        ? output.cache_hit_numerator / output.cache_hit_denominator
        : null;
    if (!output.cost_sample_count) output.cost_total = null;
    delete output.cache_hit_numerator;
    delete output.cache_hit_denominator;
    return output;
  }

  function tokenMixSignature(group) {
    const values = [
      group.input_total,
      group.cache_read,
      group.cache_write,
      group.output_total,
    ];
    return values.every((value) => value !== null) ? values.join(":") : null;
  }

  function compareSources(sources) {
    const entries = sources.flatMap((source) =>
      source.groups.map((group) => ({
        source_id: source.source_id,
        source_name: source.source_name,
        model: group.model,
        group: group.group,
        currency: group.currency,
        price_snapshot_id: group.price_snapshot_id,
        token_mix_signature: group.token_mix_signature,
        cost_total: group.cost_total,
      })),
    );
    const cohorts = new Map();
    const unranked = [];
    entries.forEach((entry) => {
      if (
        !entry.model ||
        !entry.currency ||
        !entry.price_snapshot_id ||
        !entry.token_mix_signature ||
        entry.cost_total === null
      ) {
        unranked.push({ ...entry, reason: "incomplete_comparison_contract" });
        return;
      }
      const key = [
        entry.model,
        entry.currency,
        entry.price_snapshot_id,
        entry.token_mix_signature,
      ].join("|");
      if (!cohorts.has(key)) cohorts.set(key, []);
      cohorts.get(key).push(entry);
    });
    const comparable = [];
    cohorts.forEach((items) => {
      const distinctSources = new Set(items.map((item) => item.source_id));
      if (distinctSources.size < 2) {
        items.forEach((item) =>
          unranked.push({ ...item, reason: "no_matching_station" }),
        );
        return;
      }
      comparable.push({
        model: items[0].model,
        currency: items[0].currency,
        price_snapshot_id: items[0].price_snapshot_id,
        token_mix_signature: items[0].token_mix_signature,
        ranking: [...items].sort(
          (left, right) => left.cost_total - right.cost_total,
        ),
      });
    });
    return { comparable, unranked };
  }

  function preciseAdd(left, right) {
    const scale = 1e12;
    return (Math.round(left * scale) + Math.round(right * scale)) / scale;
  }

  function toBytes(value) {
    if (value instanceof Uint8Array) return value;
    if (value instanceof ArrayBuffer) return new Uint8Array(value);
    throw new Error("content_must_be_bytes");
  }

  return { MAX_FILE_BYTES, MAX_RECORDS, analyzeFiles };
});
