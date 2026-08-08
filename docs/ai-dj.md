# AI DJ

spotatui can pick music for you. It reads the listening history it already
collects locally, and holds a conversation with a model that can search the
catalogue, look at your queue, and queue tracks. It can also just talk: ask it what
you have been listening to, or answer a question it asks you before it plays
anything.

There are **two ways to use it**, and they share everything below the model —
including the tool list, so both can do exactly the same things:

| | [MCP server](mcp-setup.md) | In-TUI DJ (this page) |
|---|---|---|
| Where you talk | Your coding agent (Claude Code, Codex, …) | A DJ screen inside spotatui |
| Needs an API key | No | Only for the API backends |
| Build with | `--features mcp-server` | `--features ai-dj` |
| Tools | The full set, driven by your agent | The same set, driven by spotatui |

Use the MCP server if you would rather DJ from a window you already have open;
**one command sets it up**, see [`docs/mcp-setup.md`](mcp-setup.md).

## How a turn works

A turn is up to **four steps**. Each step the model either says something, asks for
tools to be run, or both; spotatui runs what it asked for and hands back the
results, so it can look something up before committing. A step with words and no
tool calls ends the turn — which is what an ordinary conversational reply is.

```text
you   something chill for focusing, nothing I already own
dj    Let me see what you've been on lately.
      · get_listening_history → 30 days, 412 plays
      · search_tracks → Says — Nils Frahm [spotify:track:…] [new]
dj    Six ambient and modern-classical picks, none already yours.
      · queue_tracks → Queued 6 track(s):

you   actually more beats, less piano
dj    Downtempo, or closer to house tempo?
```

The tools are the same table the MCP server publishes, so anything added there the
in-TUI DJ can use too. The step cap matters most on an agent CLI, where **every
step is a fresh subprocess**: a four-step turn costs four process launches and four
times the quota of one. The `…thinking (2/4)` row tells you where a slow turn is.

Two paths deliberately never stop to ask a question, because nobody is watching the
screen: the auto-queue refill and the vibe shift. They get two steps and are told to
queue immediately.

> **Why the DJ is home-grown rather than Spotify's.** Spotify's own AI DJ cannot be
> driven through the public API (issue #196), and Spotify
> [restricted `/recommendations`](https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api)
> to apps that already held extended quota on 2024-11-27. spotatui users register
> their own client ID, so that endpoint returns 403 for them. The model is the
> recommender here; Spotify and YouTube are only catalogue and player.

---

## Quick start

```bash
cargo run --features ai-dj
```

Press <kbd>Ctrl</kbd>+<kbd>J</kbd> to open the DJ, type what you want, press
<kbd>Enter</kbd>.

Out of the box the DJ uses **whichever coding agent you already have installed**,
so there is no API key to configure. The default is `claude -p`.

**The first time you open the DJ it asks which AI to use and which of that AI's
models**, because the default backend spends the coding subscription you already pay
for, and the heaviest model exhausts a Claude Pro plan in a handful of turns. See
[Which AI, which model](#which-ai-which-model). Press <kbd>Esc</kbd> to keep
whatever is already configured and it will not ask again;
<kbd>Ctrl</kbd>+<kbd>G</kbd> reopens the picker whenever you want to change it. A
config that already names a backend, a model or a key is left alone and never asked
at all, which is [why some installs never see it](#it-never-asked-me).

### Keys

| Key | Action |
|---|---|
| <kbd>Ctrl</kbd>+<kbd>J</kbd> | Open the DJ screen |
| <kbd>Ctrl</kbd>+<kbd>T</kbd> | Toggle continuous auto-queue |
| <kbd>Ctrl</kbd>+<kbd>Y</kbd> | Vibe shift: drop the DJ's queued tracks and change direction |
| <kbd>Ctrl</kbd>+<kbd>O</kbd> | Toggle "only tracks I don't already have" |
| <kbd>Ctrl</kbd>+<kbd>G</kbd> | Choose which AI and model the DJ uses |
| <kbd>Enter</kbd> | Send the prompt |
| <kbd>↑</kbd> <kbd>↓</kbd> <kbd>PgUp</kbd> <kbd>PgDn</kbd> | Scroll the transcript |
| <kbd>Esc</kbd> | Clear the prompt, or leave the screen when it is empty |

All five are rebindable, see [`docs/keybindings.md`](keybindings.md).

The DJ is also reachable from the **AI DJ** row in the sidebar. The global key
exists because the sidebar's Library panel is only drawn for the Spotify source,
so the row alone would make the DJ unreachable on YouTube.

### Continuous auto-queue

With auto-queue on, the DJ tops the queue up as tracks finish, keeping roughly six
tracks ahead. It refills when the queue drops to two — about 6–8 minutes of
runway, comfortably more than the worst-case round trip.

### Only tracks you don't already have

Ask for "songs like Remind Me to Forget" and a good recommender will name tracks
you already own — they *are* the best answer to the question. Press
<kbd>Ctrl</kbd>+<kbd>O</kbd> to switch that off and get only tracks that are new
to you. The panel title shows the mode.

"Already have it" means:

* anything in your **Liked Songs**, and
* anything in a playlist **you own or collaborate on**.

Playlists you merely follow do not count. Discover Weekly, Release Radar and the
large editorial playlists hold thousands of tracks between them, and treating
those as yours would reject nearly every good recommendation.

Two things to expect the first time you turn it on:

* **A short pause while it indexes.** Liked Songs are checked exactly, one call
  per batch, with nothing cached. Playlists have no such lookup, so spotatui
  crawls yours once per session and caches the result. The title reads
  `fresh only (indexing…)` while that runs.
* **Smaller batches sometimes.** The DJ asks for roughly double what it needs so
  the rejections have somewhere to come from, and it gets one retry when most of a
  batch is filtered out. With a very large library it can still come up short; it
  says so rather than quietly queueing two tracks when you asked for six.

Start in this mode every session with:

```yaml
behavior:
  dj_avoid_library: true
```

Over MCP the same knowledge is offered rather than imposed, because there an agent
queues what it was asked to queue and silently dropping a track because you own it
would be answering a question nobody asked. So `search_tracks` marks every result
`[owned]` or `[new]` — the agent sees it while it is still choosing — and
`queue_tracks` takes `exclude_owned: true` for an agent that wants the same hard
guarantee this toggle gives you. See [`docs/mcp-setup.md`](mcp-setup.md).

---

## Which AI, which model

The first time you open the DJ it shows a two-step picker: which AI, then which of
that AI's models. It appears whether you got there with <kbd>Ctrl</kbd>+<kbd>J</kbd>
or from the sidebar's **AI DJ** row, and <kbd>Ctrl</kbd>+<kbd>G</kbd> brings it back
at any time.

**Step 1, the AI.** Every backend spotatui supports, and what each one bills
against:

| Row | Bills against |
|---|---|
| `claude` | your Claude Pro/Max plan |
| `codex` | your ChatGPT plan |
| `agy` | your Antigravity plan (Antigravity replaced Google's Gemini CLI) |
| `copilot` | your GitHub Copilot plan |
| `opencode` | whatever provider opencode is logged into |
| `anthropic` | an Anthropic API key, per token |
| `openai_compat` | nothing for a local model; per token for a hosted one |

`opencode` names models as `provider/model` (`openai/gpt-5.4`), not a bare name;
the picker says so where you type one.

An agent row you have not installed is marked **not on PATH** and is still
selectable, because "I am about to install it" is a real answer. spotatui identifies
them by looking for the binary on your `PATH` and never runs anything to find out,
so opening the picker cannot stall the UI. The legacy `gemini` row is only offered
if you actually have that binary.

**Step 2, the model.** A short list of known-good values, each with its cost, then
two rows that are always present: *Use the CLI's default* (or *Use the built-in
default* for the API backends), which passes no model at all, and *Custom…*, which
takes any name you type. The lists are conveniences rather than a contract, since
the vendor decides which models its CLI accepts, so the free-text row is the escape
hatch when a list goes out of date. `codex`, `copilot`, `opencode` and the legacy
`gemini` have no list at all: which model ids their subscription accepts is not
knowable without asking them, and the picker never runs a subprocess.

Keys in the picker: <kbd>↑</kbd> <kbd>↓</kbd> (or `j`/`k`) to move,
<kbd>1</kbd>–<kbd>9</kbd> to take a numbered row outright without a further
<kbd>Enter</kbd>, <kbd>Enter</kbd> to choose the highlighted one, <kbd>Esc</kbd> to
step back. Rows past the ninth are left unnumbered, because there is no key that
would select them. In the free-text step `j` and `k` type themselves instead of
navigating, the way any text field has to.

<kbd>Esc</kbd> from the first step closes the picker, leaves your current brain
exactly as it was, and **records that the question was answered**
(`behavior.dj_configured: true`), so it is not asked again on every visit. If that
write fails the status bar says so, and the next launch asks again.

The picker deliberately never asks for an API key: `behavior.dj_api_key` is
plaintext YAML, so it points you at `SPOTATUI_DJ_API_KEY` instead. It also leaves
`dj_base_url` alone, so a self-hosted `openai_compat` endpoint is still configured
by hand.

### It never asked me

Then something in your config already counts as a deliberate choice, and spotatui
does not interrogate a user who has already decided. Any one of these is enough:

* `dj_configured: true`, which is what the picker writes when you finish or dismiss it
* a `dj_backend` other than `agent_cli`
* a `dj_agent_command` other than the shipped `["claude", "-p"]`
* any of `dj_agent_model`, `dj_model`, `dj_api_key` or `dj_base_url` set

Tuning `dj_batch_size`, `dj_history_period`, `dj_avoid_library` or the timeout does
not count, because none of them is a choice of AI. Either way,
<kbd>Ctrl</kbd>+<kbd>G</kbd> opens the picker on demand.

---

## Backends

Set `behavior.dj_backend` in `~/.config/spotatui/config.yml`, or let the picker
write it for you.

**The two ways of paying are not interchangeable, and that is why the model matters
so much more in one of them.** `agent_cli` costs no money and spends the usage
limits of the subscription that CLI is logged into, so on a Claude Pro plan the
model is the difference between a DJ that runs all evening and one that stops after
a handful of turns. The API backends cost money per token and have no subscription
limit to hit, so there the model is a bill rather than a quota.

### `agent_cli` (default) — no API key

Runs a coding agent you already have installed and authenticated, so it costs
nothing beyond the subscription you already pay for.

```yaml
behavior:
  dj_backend: agent_cli
  dj_agent_command: ["claude", "-p"]
  dj_agent_model: haiku           # optional; that CLI's own model name
  # dj_agent_prompt_via: stdin    # only to override the preset; see below
  dj_agent_timeout_secs: 90
```

A **bare binary name expands to a known preset**, so this is enough:

```yaml
  dj_agent_command: ["codex"]     # becomes ["codex", "exec", "-"]
  dj_agent_command: ["agy"]       # becomes ["agy", "-p"], with arg delivery
```

| CLI | Expands to | With `dj_agent_model` set | Prompt via |
|---|---|---|---|
| `claude` | `claude -p` | `claude --model haiku -p` | stdin |
| `codex` | `codex exec -` | `codex exec --model MODEL -` | stdin |
| `agy` (Antigravity) | `agy -p` | `agy --model gemini-3.6-flash-low -p` | argv |
| `gemini` (legacy) | `gemini -p` | `gemini -m MODEL -p` | argv |

`agy` is Google's current CLI. The Gemini CLI it superseded is kept as a preset only
so an existing config keeps working, and it is not offered in the picker unless the
binary is installed.

Note where the model flag lands: **before** the flag that carries the prompt. For
argv delivery the prompt is appended as the last argument, so the other order
(`agy -p --model X "…"`) would hand `--model` to `-p` as its value and the prompt
would never arrive.

`agy` also **ignores stdin entirely**, so it needs `arg` delivery. Leave
`dj_agent_prompt_via` out of your config and the preset picks the right one; the
picker clears that key when you choose an agent, precisely so a `stdin` left behind
by an older config cannot swallow the prompt.

What to put in `dj_agent_model` is whatever that CLI itself accepts. For `claude`
the picker offers its public aliases, cheapest first: `haiku` (easiest on a Pro
plan), `sonnet`, `opus` (a Pro plan hits its limit fast), and `fable`. For `agy` it
offers a snapshot of what `agy models` prints: `gemini-3.6-flash-low` / `-medium` /
`-high`, `gemini-3.5-flash-low`, `gemini-3.1-pro-low` / `-high`,
`claude-sonnet-4-6`, `claude-opus-4-6-thinking`, and `gpt-oss-120b-medium`. Google
changes that list server-side, so run `agy models` for the current one. A name the
CLI does not know fails locally in milliseconds, before anything is sent, so a stale
suggestion costs you one error message and nothing else.

Any other headless command works too, since spotatui writes the prompt and reads
stdout and nothing is hardcoded about these four. **A multi-part command you wrote
yourself is used exactly as written and never gains a model flag**, so your own argv
keeps ownership of its own flags. The single exception is the shipped
`["claude", "-p"]`: it is byte-identical to what the preset produces, so it still
counts as the preset shape and can still take a model.

**Be clear about the cost of this mode:** every step is a fresh subprocess with
several seconds of startup latency, so a multi-step turn is several of those.
That is what the four-step cap bounds. The API backends below keep one HTTP
connection per step instead and are correspondingly quicker.

The agent runs in a scratch directory (`~/.config/spotatui/dj-scratch`), *not* your
current directory — otherwise it reads the `CLAUDE.md` and source files of whatever
project you happen to be in and starts recommending music about Rust.

### `anthropic` — API key

Pay per token against an API key. **A Claude Pro or Max subscription is not involved
here**: its usage limits do not apply, and it does not cover this bill either.

```yaml
behavior:
  dj_backend: anthropic
  dj_model: claude-haiku-4-5       # optional; this is also the default
```

```bash
export SPOTATUI_DJ_API_KEY=sk-ant-...
```

The models the picker offers, with list price per million tokens:

| Model | Input / output per MTok | Context |
|---|---|---|
| `claude-haiku-4-5` (default) | $1 / $5 | 200K |
| `claude-sonnet-5` | $3 / $15 | 1M |
| `claude-opus-5` | $5 / $25 | 1M |
| `claude-opus-4-8` | $5 / $25 | 1M |
| `claude-fable-5` | $10 / $50 | 1M |

One step is a compact brief and a short reply, so a single ask is cheap on any of
them; it is a multi-step turn, and continuous auto-queue over a long evening, that
makes the choice add up. Any other model id works too, since the list is a
convenience rather than a restriction.

### `openai_compat` — API key or a local model

One adapter for anything speaking `/chat/completions`: OpenAI, OpenRouter,
**Ollama**, LM Studio, vLLM, llama.cpp.

```yaml
behavior:
  dj_backend: openai_compat
  dj_base_url: http://localhost:11434/v1   # Ollama; the default
  dj_model: llama3.2
```

A local model needs no key at all. For a hosted one, `export SPOTATUI_DJ_API_KEY=…`.

There is no model list to offer here, because only your endpoint knows what it
serves. The picker's second step gives you *Use the built-in default*, which is
`gpt-4o-mini`, and a free-text row: an Ollama user wants the free-text row
(`llama3.2`), not the default. `dj_base_url` is only ever set by hand.

### The API key

`SPOTATUI_DJ_API_KEY` wins over `behavior.dj_api_key`, and never lands on disk —
prefer it. The config field exists for convenience, but **it is plaintext YAML**;
the config directory is `0700` on unix and carries a `.gitignore`, which is the
same protection the Spotify token cache relies on, but a plaintext secret is still
a plaintext secret. This mirrors how the Subsonic password is handled.

---

## What is sent to the model

Only **aggregate names**, built from your local `listens.jsonl`:

* top artists, tracks, and albums over the configured window
* the last ~25 distinct plays, so the DJ can be told not to repeat them
* what is playing now
* your current steer, if you gave one

**Not sent:** timestamps, play counts tied to a clock, Spotify IDs, URIs, your
account, or anything identifying you. That is a property of building the brief from
aggregates rather than raw records, not a filter applied afterwards.

Tune the window with `behavior.dj_history_period` (`7d`, `30d`, `month`, `year`,
`all`).

Nothing leaves your machine at all if you use `openai_compat` against a local
model.

---

## How it decides, and where it fails

The model returns *names*; spotatui resolves each one against Spotify (then
YouTube, if that feature is on) and **drops anything it cannot confidently match**.
Search always returns something, so a fuzzy near-miss would otherwise be queued in
place of a track that does not exist. A remaster or a "/ Arpeggi" suffix still
matches; a different artist with the same title does not.

Dropped tracks are logged at debug level and never shown as an error — a model
inventing a track is routine. If most of a round fails, the DJ asks once more,
naming the failures. Exactly once: the resolve step is what discovers failures, so
without a cap the retry could loop.

Duplicates are skipped too, against both the current queue and the recent window.
A DJ that re-queues the track that just finished reads as broken.

### Batch size

`behavior.dj_batch_size` (default 6, max 8). Capped for three converging reasons:
on an external Spotify Connect device each queued track costs its own Web API
call, a deep queue cannot respond to a vibe shift, and one model invocation per
batch rather than per track is what keeps latency and quota sane.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| "could not run `claude`. Is it installed and on PATH?" | Install the CLI, or point `dj_agent_command` at the right binary. |
| "did not answer within 90s" | Raise `dj_agent_timeout_secs`. If you go much above ~180, also revisit auto-queue: the refill watermark assumes a round trip well under the queue's runway. |
| "could not read a decision out of … output" | The CLI answered with prose and no JSON. Check it is logged in by running the same command by hand. |
| "There's an issue with the selected model (…)" | `dj_agent_model` names something that CLI does not accept. It fails locally in milliseconds without calling anything, so nothing was spent: pick another model with <kbd>Ctrl</kbd>+<kbd>G</kbd>, or clear the key. |
| `codex`: "Not inside a trusted directory and --skip-git-repo-check was not specified" | `codex exec` refuses to run in the DJ's scratch directory. Known limitation: use another agent (`claude`, `agy`) or an API backend, or trust `~/.config/spotatui/dj-scratch` in your codex config. |
| The DJ answers a question you did not ask | `agy` ignores stdin, so a config carrying `dj_agent_prompt_via: stdin` delivers the prompt nowhere. Remove that key so the preset decides, or set it to `arg`. |
| It never asked which AI to use | Something in your config already counts as a deliberate choice. See [It never asked me](#it-never-asked-me), or just press <kbd>Ctrl</kbd>+<kbd>G</kbd>. |
| "no API key for the Anthropic DJ backend" | `export SPOTATUI_DJ_API_KEY=…`. |
| "the model declined this request" | A safety refusal. Rephrase, or switch backend. |
| Everything is "not found" | Usually no Spotify session — log in from the UI. The catalogue lookup needs it. |
| "Very little listening history so far" | The brief needs a handful of qualifying plays. Play some music, or just tell the DJ what you want. |

Tracks shorter than 30 seconds and internet-radio streams never enter the history
by design, so they never influence the DJ.

---

## Config reference

| Key | Default | Meaning |
|---|---|---|
| `dj_backend` | `agent_cli` | `agent_cli`, `anthropic`, or `openai_compat` |
| `dj_agent_command` | `["claude", "-p"]` | argv for `agent_cli`; a bare name expands to a preset |
| `dj_agent_model` | unset | Model name passed as that CLI's own model flag (`claude --model haiku`). Unset passes no flag at all |
| `dj_agent_prompt_via` | unset (the preset decides) | `stdin` or `arg` |
| `dj_agent_timeout_secs` | `90` | Clamped to 5–600 |
| `dj_model` | unset | Model id for the API backends; the agent CLIs use `dj_agent_model` instead |
| `dj_base_url` | Ollama's local endpoint | For `openai_compat` |
| `dj_api_key` | unset | Prefer `SPOTATUI_DJ_API_KEY` |
| `dj_batch_size` | `6` | Tracks per round, max 8 |
| `dj_history_period` | `30d` | `7d`, `30d`, `month`, `year`, `all` |
| `dj_avoid_library` | `false` | Start in "only tracks I don't already have" mode |
| `dj_configured` | unset | Written by the picker; `true` stops spotatui asking which AI to use |

An invalid value is reported with a warning in the log and the default is kept —
a bad config never stops spotatui from starting.
