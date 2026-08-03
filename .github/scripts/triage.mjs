// Issue triage for spotatui, powered by DeepSeek V4-Flash.
//
// Two modes:
//   node triage.mjs            -> calls the model, prints a validated triage
//                                 JSON object to stdout (redirect to triage.json)
//   node triage.mjs --render   -> reads triage.json and prints a Markdown block
//                                 (posted as the bot comment)
//
// No dependencies: Node 20+ built-in fetch and fs only. Untrusted issue text is
// read from files (issue.json / open-issues.json) written by `gh`, never passed
// through shell interpolation.

import { readFileSync } from "node:fs";

// Rename here if DeepSeek changes the model string. Endpoint is OpenAI-compatible.
const MODEL = "deepseek-v4-flash";
const API_URL = "https://api.deepseek.com/v1/chat/completions";

// Labels the bot may apply. Deliberately excludes `duplicate` (on this repo that
// means "closed as a dup" — a human decision) and judgement labels like
// `good first issue` / `help wanted`. Dedup results go in the comment, not a label.
const ALLOWED_LABELS = ["bug", "enhancement", "question", "documentation"];

// ---- render mode -----------------------------------------------------------
if (process.argv.includes("--render")) {
  const r = JSON.parse(readFileSync("triage.json", "utf8"));
  const lines = [];
  lines.push("### 🎧 spotatui triage");
  lines.push("");
  lines.push("_Auto-triaged with DeepSeek V4-Flash._");
  lines.push("");
  lines.push(`**Label:** ${r.labels.length ? r.labels.map((l) => `\`${l}\``).join(", ") : "_none_"}`);
  if (r.likely_area) lines.push(`**Likely area:** ${r.likely_area}`);
  lines.push("");
  lines.push("**Summary**");
  lines.push("");
  lines.push(r.summary || "_n/a_");
  if (r.key_points && r.key_points.length) {
    lines.push("");
    lines.push("**Key points**");
    for (const p of r.key_points) lines.push(`- ${p}`);
  }
  if (r.missing_info && r.missing_info.length) {
    lines.push("");
    lines.push("**What would help**");
    for (const m of r.missing_info) lines.push(`- ${m}`);
  }
  lines.push("");
  if (r.dup_candidates.length) {
    lines.push("**Possible duplicates**");
    for (const d of r.dup_candidates) lines.push(`- #${d.number} (${d.confidence}) — ${d.reason}`);
  } else {
    lines.push("**Possible duplicates:** none found");
  }
  lines.push("");
  lines.push(`<sub>Compared against the ${r.compared} most recently updated open issues.</sub>`);
  console.log(lines.join("\n"));
  process.exit(0);
}

// ---- triage mode -----------------------------------------------------------
const apiKey = process.env.DEEPSEEK_API_KEY;
if (!apiKey) {
  console.error("DEEPSEEK_API_KEY is not set");
  process.exit(1);
}

const issue = JSON.parse(readFileSync("issue.json", "utf8"));
let openIssues = [];
try {
  openIssues = JSON.parse(readFileSync("open-issues.json", "utf8"));
} catch {
  openIssues = [];
}
// Never compare an issue against itself.
openIssues = openIssues.filter((i) => i.number !== issue.number);
const compared = openIssues.length;

const system = `You triage GitHub issues for "spotatui", a Rust terminal UI Spotify client
(it also has non-Spotify sources: local files, Subsonic, internet radio, YouTube).
Return ONLY a JSON object — no prose, no Markdown fences — with this exact shape:
{
  "category": one of ${JSON.stringify(ALLOWED_LABELS)} or null,
  "summary": "2 to 4 sentence plain-language paragraph describing the issue and its impact on the user",
  "key_points": ["short bullets for concrete specifics THE ISSUE ACTUALLY STATES: OS/platform, terminal, version, reproduction steps, expected vs actual behavior, error messages"],
  "likely_area": "short phrase naming the most likely affected part of spotatui IF the issue clearly indicates it (e.g. 'Spotify OAuth login', 'internet radio playback', 'native streaming device', 'keybindings'), otherwise null",
  "missing_info": ["short bullets for details a maintainer would need that are ABSENT from the report, e.g. 'OS and terminal', 'spotatui version', 'steps to reproduce'"],
  "dup_candidates": [ { "number": <int taken from the provided open-issue list>, "confidence": "high"|"medium"|"low", "reason": "short" } ]
}
Rules:
- Ground EVERYTHING in the issue text. Do NOT speculate about code, root causes, or file paths you cannot see. Do not invent facts the reporter did not state.
- "category" is your single best classification, or null if none fit.
- "key_points" and "missing_info": at most 5 items each; omit anything not evident; use [] if none.
- "dup_candidates": only issues from the provided list that plausibly describe the SAME underlying bug/request. Empty array if none. Never invent numbers.
- Be conservative: prefer null / empty over guessing.`;

const user = `NEW ISSUE #${issue.number}
Title: ${issue.title}
Body:
${(issue.body || "").slice(0, 8000)}

OPEN ISSUES (${compared} most recently updated; compare for duplicates):
${openIssues.map((i) => `#${i.number}: ${i.title}`).join("\n") || "(none)"}`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function callModel() {
  const resp = await fetch(API_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${apiKey}` },
    body: JSON.stringify({
      model: MODEL,
      temperature: 0,
      // Roomy enough for the richer JSON so the object is never truncated.
      max_tokens: 900,
      // If the API ever rejects this field, drop it — the parser below already
      // tolerates a model that wraps its JSON in a Markdown fence.
      response_format: { type: "json_object" },
      messages: [
        { role: "system", content: system },
        { role: "user", content: user },
      ],
    }),
  });
  if (!resp.ok) {
    const err = new Error(`DeepSeek API error ${resp.status}: ${await resp.text()}`);
    // 4xx (bad model string, bad request) won't fix itself; only retry 429/5xx.
    err.retryable = resp.status === 429 || resp.status >= 500;
    throw err;
  }
  return (await resp.json()).choices?.[0]?.message?.content ?? "";
}

// DeepSeek occasionally returns HTTP 200 with empty content; retry a few times
// for that (and for transient 429/5xx) before giving up.
const MAX_ATTEMPTS = 3;
let raw = "";
for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
  try {
    raw = await callModel();
    if (raw.trim() !== "") break;
    if (attempt === MAX_ATTEMPTS) {
      console.error("DeepSeek returned empty content after retries");
      process.exit(1);
    }
  } catch (e) {
    if (!e.retryable || attempt === MAX_ATTEMPTS) {
      console.error(e.message);
      process.exit(1);
    }
  }
  await sleep(1000 * attempt);
}

// Flash models occasionally wrap JSON in a ```json fence even with
// response_format set; strip it before parsing.
function parseModelJson(s) {
  const fenced = s.match(/```(?:json)?\s*([\s\S]*?)```/i);
  return JSON.parse((fenced ? fenced[1] : s).trim());
}

let triage;
try {
  triage = parseModelJson(raw);
} catch (e) {
  console.error(`Could not parse model output as JSON: ${e.message}\n---\n${raw}`);
  process.exit(1);
}
// A valid JSON scalar/array/null is not a usable triage object; treat as empty
// rather than crashing on field access below.
if (!triage || typeof triage !== "object" || Array.isArray(triage)) {
  triage = {};
}

// Normalize a model-provided list into <=n clean, length-capped strings.
const strList = (v, max, n) =>
  Array.isArray(v)
    ? v.filter((x) => typeof x === "string" && x.trim()).map((x) => x.trim().slice(0, max)).slice(0, n)
    : [];

// Hard-filter everything the model returned against reality.
const openNumbers = new Set(openIssues.map((i) => i.number));
const category = ALLOWED_LABELS.includes(triage.category) ? triage.category : null;
const likelyArea =
  typeof triage.likely_area === "string" && triage.likely_area.trim()
    ? triage.likely_area.trim().slice(0, 120)
    : null;
const dupCandidates = Array.isArray(triage.dup_candidates)
  ? triage.dup_candidates
      .filter((d) => d && typeof d === "object" && openNumbers.has(d.number))
      .map((d) => ({
        number: d.number,
        confidence: ["high", "medium", "low"].includes(d.confidence) ? d.confidence : "low",
        reason: String(d.reason || "").slice(0, 200),
      }))
  : [];

console.log(
  JSON.stringify({
    issue: issue.number,
    category,
    labels: category ? [category] : [],
    summary: String(triage.summary || "").slice(0, 800),
    likely_area: likelyArea,
    key_points: strList(triage.key_points, 200, 5),
    missing_info: strList(triage.missing_info, 200, 5),
    dup_candidates: dupCandidates,
    compared,
  }),
);
