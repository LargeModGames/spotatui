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
  lines.push(`**Summary:** ${r.summary || "_n/a_"}`);
  lines.push("");
  if (r.dup_candidates.length) {
    lines.push("**Possible duplicates:**");
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

const system = `You triage GitHub issues for "spotatui", a Rust terminal UI Spotify client.
Return ONLY a JSON object — no prose, no Markdown fences — with this exact shape:
{
  "category": one of ${JSON.stringify(ALLOWED_LABELS)} or null,
  "summary": "one plain sentence describing what the issue is about",
  "dup_candidates": [ { "number": <int taken from the provided open-issue list>, "confidence": "high"|"medium"|"low", "reason": "short" } ]
}
Rules:
- "category" is your single best classification, or null if none fit.
- "dup_candidates": only issues from the provided list that plausibly describe the SAME underlying bug/request. Empty array if none. Never invent numbers.
- Be conservative: prefer a null category and an empty dup list over guessing.`;

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

// Hard-filter everything the model returned against reality.
const openNumbers = new Set(openIssues.map((i) => i.number));
const category = ALLOWED_LABELS.includes(triage.category) ? triage.category : null;
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
    summary: String(triage.summary || "").slice(0, 300),
    dup_candidates: dupCandidates,
    compared,
  }),
);
